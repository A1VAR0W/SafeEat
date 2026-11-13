//! Admin panel (port of apps/api/src/modules/admin.ts).

use axum::extract::{Path, State};
use axum::routing::{get, post, put};
use axum::{Json, Router};
use serde_json::{json, Value};
use sqlx::{Postgres, Transaction};
use uuid::Uuid;

use crate::error::{ApiError, ApiResult};
use crate::extract::{has_role, AuthUser, JsonBody, QueryMap};
use crate::ratelimit::{per_minute, Limit};
use crate::state::AppState;
use crate::util::{self, sql_iso};

const ADMIN_LIMIT: Limit = per_minute("admin", 60);

async fn require_admin(state: &AppState, auth: &AuthUser) -> ApiResult<()> {
    if !has_role(state, auth.id, "ADMIN").await? {
        return Err(ApiError::new(
            403,
            "ADMIN_REQUIRED",
            "Solo los administradores pueden acceder a este apartado.",
        ));
    }
    Ok(())
}

/// Transaction with the per-user lock, a live session and a still-valid ADMIN role.
async fn begin_admin(
    state: &AppState,
    auth: &AuthUser,
) -> ApiResult<Transaction<'static, Postgres>> {
    let mut tx = state.db.begin().await?;
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1,0))")
        .bind(format!("user:{}", auth.id))
        .execute(&mut *tx)
        .await?;
    let session = sqlx::query_scalar::<_, i32>(
        "SELECT 1 FROM refresh_sessions WHERE id=$1 AND user_id=$2 AND revoked_at IS NULL AND expires_at>now() FOR SHARE",
    )
    .bind(auth.sid)
    .bind(auth.id)
    .fetch_optional(&mut *tx)
    .await?;
    if session.is_none() {
        return Err(ApiError::unauthorized());
    }
    let admin = sqlx::query_scalar::<_, i32>(
        "SELECT 1 FROM user_roles WHERE user_id=$1 AND role_code='ADMIN' FOR SHARE",
    )
    .bind(auth.id)
    .fetch_optional(&mut *tx)
    .await?;
    if admin.is_none() {
        return Err(ApiError::new(
            403,
            "ADMIN_REQUIRED",
            "Ya no tienes permisos de administración.",
        ));
    }
    Ok(tx)
}

fn page(rows: Vec<Value>) -> Json<Value> {
    let next = if rows.len() > 20 {
        rows[19].get("id").cloned()
    } else {
        None
    };
    let items: Vec<Value> = rows.into_iter().take(20).collect();
    Json(json!({"items": items, "nextCursor": next}))
}

fn after_param(value: Option<&String>) -> ApiResult<Option<Uuid>> {
    value.map(|v| util::uuid_param(v)).transpose()
}

async fn users(
    State(state): State<AppState>,
    auth: AuthUser,
    QueryMap(query): QueryMap,
) -> ApiResult<Json<Value>> {
    require_admin(&state, &auth).await?;
    let q = util::trimmed(query.get("q").map(String::as_str).unwrap_or(""), 0, 120)?;
    let after = after_param(query.get("after"))?;
    let rows = sqlx::query_scalar::<_, Value>(
        r#"SELECT to_jsonb(x) FROM (SELECT u.id,u.email,u.first_name AS "firstName",u.last_name AS "lastName",
          COALESCE((SELECT array_agg(role_code ORDER BY role_code) FROM user_roles WHERE user_id=u.id),'{}') AS roles
         FROM users u WHERE ($1='' OR position(lower($1) in lower(concat_ws(' ',u.email,u.first_name,u.last_name)))>0
         OR (restaurant_brand_name($1)<>'' AND position(restaurant_brand_name($1) in restaurant_brand_name(concat_ws(' ',u.email,u.first_name,u.last_name)))>0))
         AND ($2::uuid IS NULL OR u.id>$2) ORDER BY u.id LIMIT 21) x"#,
    )
    .bind(&q)
    .bind(after)
    .fetch_all(&state.db)
    .await?;
    Ok(page(rows))
}

async fn set_authorized(
    state: &AppState,
    auth: &AuthUser,
    id: &str,
    grant: bool,
) -> ApiResult<Json<Value>> {
    require_admin(state, auth).await?;
    let id = util::uuid_param(id)?;
    let mut tx = begin_admin(state, auth).await?;
    if sqlx::query_scalar::<_, i32>("SELECT 1 FROM users WHERE id=$1")
        .bind(id)
        .fetch_optional(&mut *tx)
        .await?
        .is_none()
    {
        return Err(ApiError::not_found());
    }
    let changed = if grant {
        sqlx::query("INSERT INTO user_roles(user_id,role_code) VALUES($1,'authorized_celiac') ON CONFLICT DO NOTHING")
    } else {
        sqlx::query("DELETE FROM user_roles WHERE user_id=$1 AND role_code='authorized_celiac'")
    }
    .bind(id)
    .execute(&mut *tx)
    .await?
    .rows_affected();
    if changed > 0 {
        sqlx::query("INSERT INTO admin_audit_log(actor_id,action,target_id) VALUES($1,$2,$3)")
            .bind(auth.id)
            .bind(if grant {
                "authorized_granted"
            } else {
                "authorized_removed"
            })
            .bind(id.to_string())
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;
    Ok(Json(json!({"authorized": grant})))
}

async fn grant(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<String>,
) -> ApiResult<Json<Value>> {
    set_authorized(&state, &auth, &id, true).await
}

async fn revoke(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<String>,
) -> ApiResult<Json<Value>> {
    set_authorized(&state, &auth, &id, false).await
}

fn kind_param(value: &str) -> ApiResult<bool> {
    match value {
        "products" => Ok(false),
        "restaurants" => Ok(true),
        _ => Err(ApiError::invalid_input()),
    }
}

async fn reviews(
    State(state): State<AppState>,
    auth: AuthUser,
    QueryMap(query): QueryMap,
) -> ApiResult<Json<Value>> {
    require_admin(&state, &auth).await?;
    let kind = query
        .get("kind")
        .map(String::as_str)
        .ok_or_else(ApiError::invalid_input)?;
    let restaurant = kind_param(kind)?;
    let revoked = match query.get("state").map(String::as_str) {
        None | Some("active") => false,
        Some("revoked") => true,
        Some(_) => return Err(ApiError::invalid_input()),
    };
    let q = util::trimmed(query.get("q").map(String::as_str).unwrap_or(""), 0, 120)?;
    let after = after_param(query.get("after"))?;
    let (table, resource, status) = if restaurant {
        ("restaurant_gluten_reports", "restaurant_id", "support")
    } else {
        ("product_gluten_reports", "product_id", "status")
    };
    let searchable = if restaurant {
        "c.name,community.name,r.restaurant_id"
    } else {
        "p.data->>'name',r.product_id"
    };
    let sql = format!(
        r#"SELECT to_jsonb(x) FROM (SELECT r.id,r.revision,r.{resource} AS "resourceId",r.{status} AS status,r.note,
          {updated} AS "updatedAt",{revoked_at} AS "revokedAt",r.revocation_reason AS "revocationReason",
          u.id AS "authorId",u.email AS "authorEmail",concat_ws(' ',u.first_name,u.last_name) AS "authorName",
          {verified} AS "verifiedAt",
          CASE WHEN authorized.authorized_at IS NOT NULL THEN 'authorized' WHEN manual.confirmed_at IS NOT NULL THEN 'manual' END AS "verificationMethod",
          COALESCE(manual.confirmations,0)::int AS confirmations,
          {name} AS "resourceName",{scope} AS scope
         FROM {table} r JOIN users u ON u.id=r.user_id
         {joins}
         LEFT JOIN (SELECT user_id,max(assigned_at) AS authorized_at FROM user_roles WHERE role_code='authorized_celiac' GROUP BY user_id) authorized ON authorized.user_id=r.user_id
         LEFT JOIN LATERAL (SELECT min(confirmed_at) AS confirmed_at,count(*)::int AS confirmations FROM review_confirmations WHERE kind='{kind}' AND report_id=r.id AND revision=r.revision) manual ON true
         WHERE {state_filter}
         AND ($1='' OR position(lower($1) in lower(concat_ws(' ',u.email,u.first_name,r.note,{searchable})))>0
         OR (restaurant_brand_name($1)<>'' AND position(restaurant_brand_name($1) in restaurant_brand_name(concat_ws(' ',u.email,u.first_name,r.note,{searchable})))>0))
         AND ($2::uuid IS NULL OR r.id>$2) ORDER BY r.id LIMIT 21) x"#,
        updated = sql_iso("r.updated_at"),
        revoked_at = sql_iso("r.revoked_at"),
        verified = sql_iso("COALESCE(manual.confirmed_at,CASE WHEN authorized.authorized_at IS NOT NULL THEN GREATEST(r.updated_at,authorized.authorized_at) END)"),
        name = if restaurant { "COALESCE(c.name,community.name,r.restaurant_id)" } else { "COALESCE(p.data->>'name',r.product_id)" },
        scope = if restaurant { "r.scope" } else { "'local'" },
        joins = if restaurant {
            "LEFT JOIN restaurant_catalog c ON c.id=r.restaurant_id LEFT JOIN community_restaurants community ON community.id=r.restaurant_id"
        } else {
            "LEFT JOIN products p ON p.id=r.product_id"
        },
        state_filter = if revoked { "r.revoked_at IS NOT NULL" } else { "r.revoked_at IS NULL" },
    );
    let rows = sqlx::query_scalar::<_, Value>(&sql)
        .bind(&q)
        .bind(after)
        .fetch_all(&state.db)
        .await?;
    Ok(page(rows))
}

fn review_changed() -> ApiError {
    ApiError::new(
        409,
        "REVIEW_CHANGED",
        "La revisión ha cambiado. Vuelve a leerla antes de continuar.",
    )
}

async fn verify_product(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((kind, id)): Path<(String, String)>,
    JsonBody(body): JsonBody,
) -> ApiResult<Json<Value>> {
    if kind != "products" {
        return Err(ApiError::route_not_found());
    }
    require_admin(&state, &auth).await?;
    let id = util::uuid_param(&id)?;
    let object = util::strict_object(&body, &["revision"])?;
    let revision = util::uuid_param(util::req_str(object, "revision")?)?;
    let mut tx = begin_admin(&state, &auth).await?;
    let (current, owner, revoked) = sqlx::query_as::<_, (Uuid, Uuid, bool)>(
        "SELECT revision,user_id,revoked_at IS NOT NULL FROM product_gluten_reports WHERE id=$1 FOR UPDATE",
    )
    .bind(id)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or_else(ApiError::not_found)?;
    if current != revision {
        return Err(review_changed());
    }
    if revoked {
        return Err(ApiError::new(
            409,
            "REVIEW_REVOKED",
            "No se puede verificar una revisión revocada.",
        ));
    }
    if owner == auth.id {
        return Err(ApiError::new(
            403,
            "SELF_CONFIRMATION",
            "No puedes verificar tu propia revisión.",
        ));
    }
    let inserted = sqlx::query(
        "INSERT INTO review_confirmations(kind,report_id,revision,user_id) VALUES('products',$1,$2,$3) ON CONFLICT DO NOTHING",
    )
    .bind(id)
    .bind(revision)
    .bind(auth.id)
    .execute(&mut *tx)
    .await?
    .rows_affected();
    if inserted > 0 {
        sqlx::query("INSERT INTO admin_audit_log(actor_id,action,target_id,details) VALUES($1,'product_review_verified',$2,$3)")
            .bind(auth.id)
            .bind(id.to_string())
            .bind(json!({"revision": revision}))
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;
    Ok(Json(
        json!({"verified": true, "alreadyVerifiedByAdmin": inserted == 0}),
    ))
}

async fn moderate(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((kind, id)): Path<(String, String)>,
    JsonBody(body): JsonBody,
) -> ApiResult<Json<Value>> {
    require_admin(&state, &auth).await?;
    let id = util::uuid_param(&id)?;
    let restaurant = kind_param(&kind)?;
    let object = util::strict_object(&body, &["revision", "action", "reason"])?;
    let revision = util::uuid_param(util::req_str(object, "revision")?)?;
    let revoke = match util::req_str(object, "action")? {
        "revoke" => true,
        "restore" => false,
        _ => return Err(ApiError::invalid_input()),
    };
    let reason = util::trimmed(util::req_str(object, "reason")?, 5, 500)?;
    let table = if restaurant {
        "restaurant_gluten_reports"
    } else {
        "product_gluten_reports"
    };
    let mut tx = begin_admin(&state, &auth).await?;
    let (current, revoked) = sqlx::query_as::<_, (Uuid, bool)>(&format!(
        "SELECT revision,revoked_at IS NOT NULL FROM {table} WHERE id=$1 FOR UPDATE"
    ))
    .bind(id)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or_else(ApiError::not_found)?;
    if current != revision {
        return Err(review_changed());
    }
    if revoke == revoked {
        return Err(ApiError::new(
            409,
            "MODERATION_CHANGED",
            "El estado de la revisión ha cambiado. Actualiza la lista.",
        ));
    }
    let updated = sqlx::query_scalar::<_, Uuid>(&format!(
        "UPDATE {table} SET revoked_at=CASE WHEN $4::boolean THEN now() ELSE NULL END,revoked_by=CASE WHEN $4 THEN $2::uuid ELSE NULL END,revocation_reason=$3 WHERE id=$1 RETURNING revision"
    ))
    .bind(id)
    .bind(auth.id)
    .bind(&reason)
    .bind(revoke)
    .fetch_one(&mut *tx)
    .await?;
    sqlx::query(
        "INSERT INTO admin_audit_log(actor_id,action,target_id,details) VALUES($1,$2,$3,$4)",
    )
    .bind(auth.id)
    .bind(if revoke {
        "review_revoked"
    } else {
        "review_restored"
    })
    .bind(id.to_string())
    .bind(json!({"kind": kind, "reason": reason, "previousRevision": revision}))
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(Json(json!({"revoked": revoke, "revision": updated})))
}

/// Gluten analysis and on-server AI counters since the last restart (no personal data).
async fn analysis_metrics(State(state): State<AppState>, auth: AuthUser) -> ApiResult<Json<Value>> {
    require_admin(&state, &auth).await?;
    Ok(Json(crate::metrics::snapshot()))
}

pub fn routes(state: &AppState) -> Router<AppState> {
    Router::new()
        .route("/api/v1/admin/analysis-metrics", get(analysis_metrics))
        .route("/api/v1/admin/users", get(users))
        .route(
            "/api/v1/admin/users/{id}/authorized",
            put(grant).delete(revoke),
        )
        .route("/api/v1/admin/reviews", get(reviews))
        .route(
            "/api/v1/admin/reviews/{kind}/{id}/verify",
            post(verify_product),
        )
        .route("/api/v1/admin/reviews/{kind}/{id}/moderate", post(moderate))
        .route_layer(crate::app::limit(state, ADMIN_LIMIT))
}
