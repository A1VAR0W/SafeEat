//! Community reviews and confirmations (port of apps/api/src/modules/reviews.ts).
//! Table and column identifiers are constants, never request input.

use axum::Json;
use safeeat_contracts::{CommunityReview, Items};
use serde_json::{json, Value};
use uuid::Uuid;

use crate::error::{ApiError, ApiResult};
use crate::extract::has_role;
use crate::ratelimit::{per_hour, Limit};
use crate::state::AppState;
use crate::util::{self, sql_iso};

pub const CONFIRM_PRODUCT_LIMIT: Limit = per_hour("products-confirm", 30);
pub const CONFIRM_RESTAURANT_LIMIT: Limit = per_hour("restaurants-confirm", 30);

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Products,
    Restaurants,
}

impl Kind {
    fn name(self) -> &'static str {
        match self {
            Kind::Products => "products",
            Kind::Restaurants => "restaurants",
        }
    }
    fn table(self) -> &'static str {
        match self {
            Kind::Products => "product_gluten_reports",
            Kind::Restaurants => "restaurant_gluten_reports",
        }
    }
    fn resource(self) -> &'static str {
        match self {
            Kind::Products => "product_id",
            Kind::Restaurants => "restaurant_id",
        }
    }
    fn status(self) -> &'static str {
        match self {
            Kind::Products => "status",
            Kind::Restaurants => "support",
        }
    }
    /// `$1` = resource id, `$2` = group key (restaurants only).
    fn scope(self) -> &'static str {
        match self {
            Kind::Products => "r.product_id=$1",
            Kind::Restaurants => {
                "(r.restaurant_id=$1 OR (r.chain_key=$2 AND r.chain_key IS NOT NULL))"
            }
        }
    }
}

#[derive(sqlx::FromRow)]
struct Row {
    id: String,
    revision: String,
    own: bool,
    confirmed_by_me: bool,
    note: String,
    status: String,
    updated_at: String,
    verified_at: Option<String>,
    verification_method: Option<String>,
    confirmations: i64,
    inherited: bool,
    scope: Option<String>,
}

pub async fn list(
    state: &AppState,
    kind: Kind,
    id: &str,
    group_key: Option<&str>,
    user: Option<Uuid>,
) -> ApiResult<Json<Items<CommunityReview>>> {
    let restaurant = kind == Kind::Restaurants;
    let user_param = if restaurant { "$3" } else { "$2" };
    let scope_column = if restaurant { "r.scope" } else { "NULL::text" };
    let sql = format!(
        "SELECT r.id::text AS id,r.revision::text AS revision,COALESCE(r.user_id={user_param}::uuid,false) AS own,
         COALESCE(bool_or(c.user_id={user_param}::uuid),false) AS confirmed_by_me,r.note,r.{status} AS status,
         {updated} AS updated_at,
         {verified} AS verified_at,
         CASE WHEN a.authorized_at IS NOT NULL THEN 'authorized' WHEN min(c.confirmed_at) IS NOT NULL THEN 'manual' END AS verification_method,
         count(c.user_id)::bigint AS confirmations,(r.{resource}<>$1) AS inherited,{scope_column} AS scope
         FROM {table} r LEFT JOIN review_confirmations c ON c.report_id=r.id AND c.revision=r.revision AND c.kind='{kind}'
         LEFT JOIN (SELECT user_id,max(assigned_at) AS authorized_at FROM user_roles WHERE role_code='authorized_celiac' GROUP BY user_id) a ON a.user_id=r.user_id
         WHERE {scope} AND r.revoked_at IS NULL
         GROUP BY r.id,r.revision,r.user_id,r.note,r.{status},r.updated_at,r.{resource},a.authorized_at{group_scope}
         ORDER BY r.updated_at DESC LIMIT 100",
        status = kind.status(),
        resource = kind.resource(),
        table = kind.table(),
        kind = kind.name(),
        scope = kind.scope(),
        updated = sql_iso("r.updated_at"),
        verified = sql_iso("COALESCE(min(c.confirmed_at),CASE WHEN a.authorized_at IS NOT NULL THEN GREATEST(r.updated_at,a.authorized_at) END)"),
        group_scope = if restaurant { ",r.scope" } else { "" },
    );
    let mut query = sqlx::query_as::<_, Row>(&sql).bind(id);
    if restaurant {
        query = query.bind(group_key);
    }
    let rows = query.bind(user).fetch_all(&state.db).await?;
    Ok(Json(Items {
        items: rows
            .into_iter()
            .map(|r| CommunityReview {
                id: r.id,
                revision: r.revision,
                own: r.own,
                confirmed_by_me: r.confirmed_by_me,
                note: r.note,
                status: r.status,
                updated_at: r.updated_at,
                verified_at: r.verified_at,
                verification_method: r.verification_method,
                confirmations: r.confirmations,
                inherited: r.inherited,
                scope: r.scope,
            })
            .collect(),
    }))
}

pub async fn confirm(
    state: &AppState,
    kind: Kind,
    id: &str,
    group_key: Option<&str>,
    review_id: &str,
    user: Uuid,
    body: &Value,
) -> ApiResult<Json<Value>> {
    let review_id = util::uuid_param(review_id)?;
    let object = util::strict_object(body, &["revision"])?;
    let revision = util::uuid_param(util::req_str(object, "revision")?)?;
    if kind == Kind::Products && has_role(state, user, "ADMIN").await? {
        return Err(ApiError::new(
            403,
            "ADMIN_PANEL_REQUIRED",
            "Los administradores deben verificar productos desde el panel de administración.",
        ));
    }
    let restaurant = kind == Kind::Restaurants;
    let sql = format!(
        "SELECT r.user_id,r.revision FROM {} r WHERE {} AND r.revoked_at IS NULL AND r.id=${} FOR UPDATE",
        kind.table(),
        kind.scope(),
        if restaurant { 3 } else { 2 }
    );
    let mut tx = state.db.begin().await?;
    let mut query = sqlx::query_as::<_, (Uuid, Uuid)>(&sql).bind(id);
    if restaurant {
        query = query.bind(group_key);
    }
    let (owner, current) = query
        .bind(review_id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or_else(ApiError::not_found)?;
    if owner == user {
        return Err(ApiError::new(
            403,
            "SELF_CONFIRMATION",
            "Otra persona debe confirmar tu revisión.",
        ));
    }
    if current != revision {
        return Err(ApiError::new(
            409,
            "REVIEW_CHANGED",
            "El motivo ha cambiado. Vuelve a leerlo antes de confirmarlo.",
        ));
    }
    sqlx::query(
        "INSERT INTO review_confirmations(kind,report_id,revision,user_id) VALUES($1,$2,$3,$4) ON CONFLICT DO NOTHING",
    )
    .bind(kind.name())
    .bind(review_id)
    .bind(revision)
    .bind(user)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(Json(json!({"confirmed": true})))
}
