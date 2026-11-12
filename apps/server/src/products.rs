//! Products (port of apps/api/src/modules/products) plus SafeEat 2.0 verification:
//! personal verdicts, certification seals, AI label reading and batched name search.

use std::collections::HashMap;
use std::sync::Arc;

use axum::body::Body;
use axum::extract::{Path, State};
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post, put};
use axum::{Json, Router};
use base64::Engine as _;
use safeeat_contracts::{
    GlutenStatus, InformationSource, Items, LabelAnalysis, NutritionInput, PersonalVerdict,
    Product, ProductReportCounts, Reason, VerdictValue,
};
use safeeat_gluten_engine::{assess_gluten, score_nutrition, GlutenInput, Origin};
use serde_json::Value;
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::ai_check;
use crate::error::{ApiError, ApiResult};
use crate::extract::{AuthUser, JsonBody, MaybeUser, QueryMap};
use crate::permissions::review_note;
use crate::providers::food_data_central::enriched_barcode;
use crate::providers::ExternalProduct;
use crate::ratelimit::{per_hour, per_minute, Limit};
use crate::reviews::{self, Kind};
use crate::state::AppState;
use crate::translate::{self, Translation};
use crate::util::{self, now_iso};

const SEARCH_LIMIT: Limit = per_minute("product-search", 15);
const REPORT_LIMIT: Limit = per_hour("product-report", 8);
const PHOTO_LIMIT: Limit = per_hour("ingredients-photo", 6);
const AI_LIMIT: Limit = per_hour("label-ai", 10);
const VERDICT_LIMIT: Limit = per_hour("product-verdict", 60);
const COMMUNITY_SOURCE: &str = "Comunidad SafeEat";

#[derive(sqlx::FromRow, Clone)]
pub struct Contribution {
    pub id: Uuid,
    pub product_name: String,
    pub ingredients: String,
}

fn parse(data: Value) -> ApiResult<Product> {
    serde_json::from_value(data).map_err(|error| {
        tracing::error!(%error, "stored product does not match the contract");
        ApiError::internal()
    })
}

fn photo_url(state: &AppState, code: &str, contribution: Uuid) -> String {
    format!(
        "{}/api/v1/products/{code}/ingredients-photo?v={contribution}",
        state.config.api_base()
    )
}

fn community_assessment(ingredients: &str) -> safeeat_contracts::Assessment {
    assess_gluten(&GlutenInput {
        ingredients: Some(ingredients.into()),
        source: "Etiqueta revisada por un usuario".into(),
        origin: Some(Origin::Ocr),
        reviewed: Some(true),
        ..Default::default()
    })
}

pub async fn latest(state: &AppState, code: &str) -> ApiResult<Option<Contribution>> {
    Ok(sqlx::query_as::<_, Contribution>(
        "SELECT id,product_name,ingredients FROM product_contributions WHERE barcode=$1 ORDER BY created_at DESC LIMIT 1",
    )
    .bind(code)
    .fetch_optional(&state.db)
    .await?)
}

async fn latest_many(
    state: &AppState,
    codes: &[String],
) -> ApiResult<HashMap<String, Contribution>> {
    let rows = sqlx::query_as::<_, (String, Uuid, String, String)>(
        "SELECT DISTINCT ON (barcode) barcode,id,product_name,ingredients FROM product_contributions WHERE barcode=ANY($1) ORDER BY barcode,created_at DESC",
    )
    .bind(codes)
    .fetch_all(&state.db)
    .await?;
    Ok(rows
        .into_iter()
        .map(|(code, id, product_name, ingredients)| {
            (
                code,
                Contribution {
                    id,
                    product_name,
                    ingredients,
                },
            )
        })
        .collect())
}

fn contributed(state: &AppState, item: Product, contribution: Option<&Contribution>) -> Product {
    let Some(c) = contribution else { return item };
    let community = community_assessment(&c.ingredients);
    let use_community = item.ingredients.is_none()
        || item.assessment.status == GlutenStatus::Unknown
        || risk(community.status) > risk(item.assessment.status);
    let barcode = item.barcode.clone().unwrap_or_default();
    Product {
        name: if item.name.starts_with("Producto ") {
            c.product_name.clone()
        } else {
            item.name.clone()
        },
        ingredients: if use_community {
            Some(c.ingredients.clone())
        } else {
            item.ingredients.clone()
        },
        assessment: if use_community {
            community
        } else {
            item.assessment.clone()
        },
        information_source: if use_community {
            InformationSource::Community
        } else {
            item.information_source
        },
        community_contribution: true,
        ingredients_image_url: Some(photo_url(state, &barcode, c.id)),
        ingredients_original: if use_community {
            None
        } else {
            item.ingredients_original.clone()
        },
        ingredients_language: if use_community {
            None
        } else {
            item.ingredients_language.clone()
        },
        ..item
    }
}

pub fn community_product(state: &AppState, code: &str, c: &Contribution) -> Product {
    Product {
        id: format!("off-{code}"),
        barcode: Some(code.into()),
        name: c.product_name.clone(),
        brand: None,
        image_url: None,
        ingredients_image_url: Some(photo_url(state, code, c.id)),
        information_source: InformationSource::Community,
        community_contribution: true,
        gluten_reports: None,
        verified_gluten_reports: None,
        ingredients: Some(c.ingredients.clone()),
        assessment: community_assessment(&c.ingredients),
        nutrition: NutritionInput::default(),
        quality: score_nutrition(&NutritionInput::default()),
        nutri_score: None,
        source: COMMUNITY_SOURCE.into(),
        source_url: None,
        fetched_at: now_iso(),
        stale: false,
        user_created: false,
        my_verdict: None,
        ingredients_original: None,
        ingredients_language: None,
        ai_check: None,
    }
}

/// Community report counts for many products in one query.
pub async fn with_reports_many(state: &AppState, items: Vec<Product>) -> ApiResult<Vec<Product>> {
    let ids: Vec<String> = items
        .iter()
        .filter(|p| !p.user_created)
        .map(|p| p.id.clone())
        .collect();
    if ids.is_empty() {
        return Ok(ai_check::attach(state, items).await);
    }
    let rows = sqlx::query_as::<_, (String, i64, i64, i64, i64)>(
        "WITH reports AS (SELECT r.product_id,r.status,
          EXISTS(SELECT 1 FROM user_roles ur WHERE ur.user_id=r.user_id AND ur.role_code='authorized_celiac')
          OR EXISTS(SELECT 1 FROM review_confirmations c WHERE c.kind='products' AND c.report_id=r.id AND c.revision=r.revision) AS verified
          FROM product_gluten_reports r WHERE r.product_id=ANY($1) AND r.revoked_at IS NULL)
         SELECT product_id,count(*) FILTER (WHERE status='GLUTEN_FREE'),
         count(*) FILTER (WHERE status='CONTAINS_GLUTEN'),
         count(*) FILTER (WHERE status='GLUTEN_FREE' AND verified),
         count(*) FILTER (WHERE status='CONTAINS_GLUTEN' AND verified) FROM reports GROUP BY product_id",
    )
    .bind(&ids)
    .fetch_all(&state.db)
    .await?;
    let counts: HashMap<String, (i64, i64, i64, i64)> = rows
        .into_iter()
        .map(|(id, a, b, c, d)| (id, (a, b, c, d)))
        .collect();
    let items: Vec<Product> = items
        .into_iter()
        .map(|item| {
            if item.user_created {
                return item;
            }
            let (free, contains, vfree, vcontains) =
                counts.get(&item.id).copied().unwrap_or_default();
            Product {
                gluten_reports: Some(ProductReportCounts {
                    gluten_free: free,
                    contains_gluten: contains,
                }),
                verified_gluten_reports: Some(ProductReportCounts {
                    gluten_free: vfree,
                    contains_gluten: vcontains,
                }),
                ..item
            }
        })
        .collect();
    Ok(ai_check::attach(state, items).await)
}

pub async fn with_reports(state: &AppState, item: Product) -> ApiResult<Product> {
    Ok(with_reports_many(state, vec![item]).await?.remove(0))
}

/// Same hash on save and on read, so a verdict notices later ingredient changes.
/// Hashes the original list, so a new Spanish rendering is not "changed ingredients".
pub fn ingredients_hash(product: &Product) -> String {
    let text = product
        .ingredients_original
        .as_deref()
        .or(product.ingredients.as_deref())
        .unwrap_or("")
        .trim()
        .to_lowercase();
    hex::encode(Sha256::digest(text.as_bytes()))
}

/// Attach the caller's personal verdicts (never changes the automatic assessment).
pub async fn with_verdicts(
    state: &AppState,
    items: Vec<Product>,
    user: Option<Uuid>,
) -> ApiResult<Vec<Product>> {
    let Some(user) = user else { return Ok(items) };
    let ids: Vec<String> = items.iter().map(|p| p.id.clone()).collect();
    let rows = sqlx::query_as::<_, (String, String, String, Option<String>, String)>(&format!(
        "SELECT product_id,verdict,note,ingredients_hash,{} FROM user_product_verdicts WHERE user_id=$1 AND product_id=ANY($2)",
        util::sql_iso("updated_at")
    ))
    .bind(user)
    .bind(&ids)
    .fetch_all(&state.db)
    .await?;
    let verdicts: HashMap<String, (String, String, Option<String>, String)> = rows
        .into_iter()
        .map(|(id, v, n, h, u)| (id, (v, n, h, u)))
        .collect();
    Ok(items
        .into_iter()
        .map(|item| match verdicts.get(&item.id) {
            Some((verdict, note, hash, updated_at)) => Product {
                my_verdict: Some(PersonalVerdict {
                    verdict: if verdict == "SAFE" {
                        VerdictValue::Safe
                    } else {
                        VerdictValue::Unsafe
                    },
                    note: note.clone(),
                    updated_at: updated_at.clone(),
                    ingredients_changed: hash
                        .as_deref()
                        .is_some_and(|h| h != ingredients_hash(&item)),
                }),
                ..item
            },
            None => item,
        })
        .collect())
}

pub async fn decorate(state: &AppState, item: Product, user: Option<Uuid>) -> ApiResult<Product> {
    let item = with_reports(state, item).await?;
    Ok(with_verdicts(state, vec![item], user).await?.remove(0))
}

/// Stored product → current view (contribution overlay + reports).
pub async fn hydrate_stored(state: &AppState, data: Value, expired: bool) -> ApiResult<Product> {
    let item = Product {
        stale: expired,
        ..parse(data)?
    };
    let Some(code) = item.barcode.clone() else {
        return Ok(item);
    };
    let c = latest(state, &code).await?;
    let view = match &c {
        Some(c) if item.source == COMMUNITY_SOURCE => community_product(state, &code, c),
        _ => contributed(state, item, c.as_ref()),
    };
    with_reports(state, view).await
}

fn risk(status: GlutenStatus) -> u8 {
    match status {
        GlutenStatus::ContainsGluten => 3,
        GlutenStatus::MayContainTraces => 2,
        GlutenStatus::GlutenFree => 1,
        GlutenStatus::Unknown => 0,
    }
}

/// `translation` is the Spanish view of the ingredients. The analysis runs on the
/// original; the translation is analysed too and wins only when it finds gluten or
/// traces the original missed (e.g. "farine de blé" → "harina de trigo").
fn from_external(external: &ExternalProduct, translation: Option<&Translation>) -> Product {
    let mut assessment = assess_gluten(&external.gluten);
    if let Some(t) = translation {
        let translated = assess_gluten(&GlutenInput {
            ingredients: Some(t.text.clone()),
            ..external.gluten.clone()
        });
        if matches!(
            translated.status,
            GlutenStatus::ContainsGluten | GlutenStatus::MayContainTraces
        ) && risk(translated.status) > risk(assessment.status)
        {
            assessment = translated;
        }
    }
    assessment.certification = external.certification;
    if external.summary_only {
        assessment.reasons.push(Reason {
            code: "SEARCH_PREVIEW".into(),
            message: "Vista previa del buscador. Abre el producto para ver el análisis completo."
                .into(),
        });
    }
    Product {
        id: format!("off-{}", external.barcode),
        barcode: Some(external.barcode.clone()),
        name: external.name.clone(),
        brand: external.brand.clone(),
        image_url: external.image_url.clone(),
        ingredients_image_url: None,
        information_source: external.information_source,
        community_contribution: false,
        gluten_reports: None,
        verified_gluten_reports: None,
        ingredients: translation
            .map(|t| t.text.clone())
            .or_else(|| external.ingredients.clone()),
        assessment,
        nutrition: external.nutrition.clone(),
        quality: score_nutrition(&external.nutrition),
        nutri_score: external.nutri_score.clone(),
        source: external.source.clone(),
        source_url: Some(external.source_url.clone()),
        fetched_at: now_iso(),
        stale: external.summary_only,
        user_created: false,
        my_verdict: None,
        ingredients_original: translation.and(external.ingredients.clone()),
        ingredients_language: translation.map(|t| t.language.clone()),
        ai_check: None,
    }
}

fn to_json(item: &Product) -> ApiResult<Value> {
    serde_json::to_value(item).map_err(|_| ApiError::internal())
}

async fn save(state: &AppState, external: &ExternalProduct) -> ApiResult<Product> {
    let translation = translate::spanish_ingredients(state, &external.language).await;
    let item = from_external(external, translation.as_ref());
    if let Some(items) = ai_check::eligible(&item, &external.language) {
        ai_check::enqueue(state, &item, items).await;
    }
    sqlx::query(
        "INSERT INTO products(id,barcode,provider,data,expires_at) VALUES($1,$2,'openfoodfacts',$3,now()+interval '24 hours')
         ON CONFLICT(id) DO UPDATE SET provider='openfoodfacts',data=$3,fetched_at=now(),expires_at=now()+interval '24 hours'",
    )
    .bind(&item.id)
    .bind(&item.barcode)
    .bind(to_json(&item)?)
    .execute(&state.db)
    .await?;
    Ok(item)
}

/// Search previews never overwrite a full product: insert-if-absent, then read back what is stored.
async fn save_previews(
    state: &AppState,
    externals: &[ExternalProduct],
) -> ApiResult<Vec<(Value, bool)>> {
    let items: Vec<Product> = externals.iter().map(|e| from_external(e, None)).collect();
    let ids: Vec<String> = items.iter().map(|p| p.id.clone()).collect();
    let codes: Vec<String> = items
        .iter()
        .map(|p| p.barcode.clone().unwrap_or_default())
        .collect();
    let data: Vec<Value> = items.iter().map(to_json).collect::<ApiResult<_>>()?;
    sqlx::query(
        "INSERT INTO products(id,barcode,provider,data,expires_at)
         SELECT id,barcode,'openfoodfacts',data,now() FROM UNNEST($1::text[],$2::text[],$3::jsonb[]) AS t(id,barcode,data)
         ON CONFLICT DO NOTHING",
    )
    .bind(&ids)
    .bind(&codes)
    .bind(&data)
    .execute(&state.db)
    .await?;
    let rows = sqlx::query_as::<_, (String, Value, bool)>(
        "SELECT id,data,expires_at<now() FROM products WHERE id=ANY($1)",
    )
    .bind(&ids)
    .fetch_all(&state.db)
    .await?;
    let mut by_id: HashMap<String, (Value, bool)> =
        rows.into_iter().map(|(id, d, e)| (id, (d, e))).collect();
    Ok(ids.iter().filter_map(|id| by_id.remove(id)).collect())
}

fn not_found_product() -> ApiError {
    ApiError::new(
        404,
        "PRODUCT_NOT_FOUND",
        "No hemos encontrado este producto. Puedes fotografiar sus ingredientes.",
    )
}

struct CachedRow {
    data: Value,
    fresh: bool,
    provider: String,
}

async fn cached(state: &AppState, code: &str) -> ApiResult<Option<CachedRow>> {
    Ok(sqlx::query_as::<_, (Value, bool, String)>(
        "SELECT data,expires_at>now(),provider FROM products WHERE barcode=$1 AND owner_id IS NULL",
    )
    .bind(code)
    .fetch_optional(&state.db)
    .await?
    .map(|(data, fresh, provider)| CachedRow {
        data,
        fresh,
        provider,
    }))
}

async fn from_cache(state: &AppState, code: &str) -> ApiResult<Option<Product>> {
    let row = cached(state, code).await?;
    let contribution = latest(state, code).await?;
    match row {
        Some(row) if row.fresh && (row.provider != "community" || contribution.is_some()) => {
            Ok(Some(if row.provider == "community" {
                community_product(state, code, contribution.as_ref().expect("checked"))
            } else {
                contributed(state, parse(row.data)?, contribution.as_ref())
            }))
        }
        _ => Ok(None),
    }
}

/// Product by barcode: fresh cache, else provider (one request per barcode at a time).
pub async fn barcode(state: &AppState, code: &str) -> ApiResult<Product> {
    if let Some(hit) = from_cache(state, code).await? {
        return Ok(hit);
    }
    let lock = {
        let mut locks = state.barcode_locks.lock().await;
        locks
            .entry(code.to_string())
            .or_insert_with(|| Arc::new(tokio::sync::Mutex::new(())))
            .clone()
    };
    let result = {
        let _guard = lock.lock().await;
        match from_cache(state, code).await? {
            Some(hit) => Ok(hit),
            None => fetch(state, code).await,
        }
    };
    state.barcode_locks.lock().await.remove(code);
    result
}

async fn fetch(state: &AppState, code: &str) -> ApiResult<Product> {
    let row = cached(state, code).await?;
    let contribution = latest(state, code).await?;
    match enriched_barcode(&state.off, state.fdc.as_ref(), code).await {
        Ok(Some(external)) => {
            let saved = save(state, &external).await?;
            Ok(contributed(
                state,
                saved,
                latest(state, code).await?.as_ref(),
            ))
        }
        Ok(None) => match latest(state, code).await? {
            Some(c) => Ok(community_product(state, code, &c)),
            None => Err(not_found_product()),
        },
        Err(error) if error.is_provider_unavailable() => {
            if let Some(row) = row.filter(|r| r.provider != "community" || contribution.is_some()) {
                let item = Product {
                    stale: true,
                    ..parse(row.data)?
                };
                return Ok(contributed(state, item, contribution.as_ref()));
            }
            match latest(state, code).await? {
                Some(c) => Ok(community_product(state, code, &c)),
                None => Err(error),
            }
        }
        Err(error) => Err(error),
    }
}

/// Product by id (public, or owned by the caller).
pub async fn get_product(state: &AppState, id: &str, user: Option<Uuid>) -> ApiResult<Product> {
    let row = sqlx::query_as::<_, (Value, Option<String>)>(
        "SELECT data,barcode FROM products WHERE id=$1 AND (owner_id IS NULL OR owner_id=$2)",
    )
    .bind(id)
    .bind(user)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(ApiError::not_found)?;
    let item = match row.1 {
        Some(code) => barcode(state, &code).await?,
        None => parse(row.0)?,
    };
    with_reports(state, item).await
}

fn product_id(value: &str) -> ApiResult<String> {
    util::bounded(value, 0, 80)?;
    Ok(value.to_string())
}

async fn by_barcode(
    State(state): State<AppState>,
    MaybeUser(user): MaybeUser,
    Path(code): Path<String>,
) -> ApiResult<Json<Product>> {
    let code = util::barcode_param(&code)?;
    let item = barcode(&state, &code).await?;
    Ok(Json(decorate(&state, item, user.map(|u| u.id)).await?))
}

fn is_gluten_free_choice(item: &Product) -> bool {
    match &item.my_verdict {
        Some(v) => v.verdict == VerdictValue::Safe,
        None => {
            item.assessment.status != GlutenStatus::ContainsGluten
                && item.assessment.status != GlutenStatus::MayContainTraces
                && (item.assessment.status == GlutenStatus::GlutenFree
                    || item.assessment.certification.is_some())
        }
    }
}

async fn search(
    State(state): State<AppState>,
    MaybeUser(user): MaybeUser,
    QueryMap(query): QueryMap,
) -> ApiResult<Json<Items<Product>>> {
    let q = util::trimmed(
        query
            .get("q")
            .map(String::as_str)
            .ok_or_else(ApiError::invalid_input)?,
        2,
        100,
    )?;
    let gluten_free = util::query_bool(query.get("glutenFree"))?.unwrap_or(false);
    let externals = state.off.search(&q).await?;
    let stored = save_previews(&state, &externals).await?;
    let mut items = Vec::with_capacity(stored.len());
    for (data, expired) in stored {
        items.push(Product {
            stale: expired,
            ..parse(data)?
        });
    }
    let mut codes: Vec<String> = items.iter().filter_map(|p| p.barcode.clone()).collect();
    let pattern = format!("%{}%", util::like_escape(&q));
    let community = sqlx::query_as::<_, (String, String, String, Uuid)>(
        "SELECT DISTINCT ON (barcode) barcode,product_name,ingredients,id FROM product_contributions
         WHERE product_name ILIKE $1 OR restaurant_brand_name(product_name) LIKE '%'||restaurant_brand_name($2)||'%'
         ORDER BY barcode,created_at DESC LIMIT 20",
    )
    .bind(&pattern)
    .bind(&q)
    .fetch_all(&state.db)
    .await?;
    codes.extend(community.iter().map(|c| c.0.clone()));
    let contributions = latest_many(&state, &codes).await?;
    let mut views: Vec<Product> = items
        .into_iter()
        .map(|item| {
            let code = item.barcode.clone().unwrap_or_default();
            match contributions.get(&code) {
                Some(c) if item.source == COMMUNITY_SOURCE => community_product(&state, &code, c),
                c => contributed(&state, item, c),
            }
        })
        .collect();
    let known: std::collections::HashSet<String> =
        views.iter().filter_map(|p| p.barcode.clone()).collect();
    for (code, product_name, ingredients, id) in community {
        if !known.contains(&code) {
            views.push(community_product(
                &state,
                &code,
                &Contribution {
                    id,
                    product_name,
                    ingredients,
                },
            ));
        }
    }
    let views = with_reports_many(&state, views).await?;
    let mut views = with_verdicts(&state, views, user.map(|u| u.id)).await?;
    if gluten_free {
        views.retain(is_gluten_free_choice);
    }
    Ok(Json(Items { items: views }))
}

async fn analyze_text(
    State(state): State<AppState>,
    auth: AuthUser,
    JsonBody(body): JsonBody,
) -> ApiResult<Response> {
    let object = util::strict_object(&body, &["name", "text", "reviewed"])?;
    let name = match util::opt_str(object, "name")? {
        Some(n) => util::trimmed(n, 1, 160)?,
        None => "Etiqueta escaneada".into(),
    };
    let text = util::trimmed(util::req_str(object, "text")?, 3, 12000)?;
    let reviewed = util::opt_bool(object, "reviewed")?.unwrap_or(false);
    let item = Product {
        id: Uuid::new_v4().to_string(),
        barcode: None,
        name,
        brand: None,
        image_url: None,
        ingredients_image_url: None,
        information_source: InformationSource::Community,
        community_contribution: false,
        gluten_reports: None,
        verified_gluten_reports: None,
        assessment: assess_gluten(&GlutenInput {
            ingredients: Some(text.clone()),
            source: "Etiqueta revisada por el usuario".into(),
            origin: Some(Origin::Ocr),
            reviewed: Some(reviewed),
            ..Default::default()
        }),
        ingredients: Some(text),
        nutrition: NutritionInput::default(),
        quality: score_nutrition(&NutritionInput::default()),
        nutri_score: None,
        source: "Etiqueta personal".into(),
        source_url: None,
        fetched_at: now_iso(),
        stale: false,
        user_created: true,
        my_verdict: None,
        ingredients_original: None,
        ingredients_language: None,
        ai_check: None,
    };
    sqlx::query("INSERT INTO products(id,provider,owner_id,data,expires_at) VALUES($1,'user',$2,$3,now()+interval '30 days')")
        .bind(&item.id)
        .bind(auth.id)
        .bind(to_json(&item)?)
        .execute(&state.db)
        .await?;
    Ok((StatusCode::CREATED, Json(item)).into_response())
}

async fn product(
    State(state): State<AppState>,
    MaybeUser(user): MaybeUser,
    Path(id): Path<String>,
) -> ApiResult<Json<Product>> {
    let id = product_id(&id)?;
    let user = user.map(|u| u.id);
    let item = get_product(&state, &id, user).await?;
    Ok(Json(
        with_verdicts(&state, vec![item], user).await?.remove(0),
    ))
}

async fn gluten_report(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<String>,
    JsonBody(body): JsonBody,
) -> ApiResult<Json<Product>> {
    let id = product_id(&id)?;
    let object = util::strict_object(&body, &["status", "note"])?;
    let status = util::req_str(object, "status")?;
    if !matches!(status, "GLUTEN_FREE" | "CONTAINS_GLUTEN") {
        return Err(ApiError::invalid_input());
    }
    let note = util::trimmed(util::opt_str(object, "note")?.unwrap_or(""), 0, 500)?;
    let item = get_product(&state, &id, Some(auth.id)).await?;
    let note = review_note(&state, auth.id, &note).await?;
    if item.barcode.is_none() || item.user_created {
        return Err(ApiError::new(
            400,
            "REPORT_NOT_ALLOWED",
            "Esta ficha no admite revisiones públicas.",
        ));
    }
    let written = sqlx::query(
        "INSERT INTO product_gluten_reports(product_id,user_id,status,note) VALUES($1,$2,$3,$4)
         ON CONFLICT(product_id,user_id) DO UPDATE SET status=$3,note=$4,updated_at=now() WHERE product_gluten_reports.revoked_at IS NULL RETURNING id",
    )
    .bind(&item.id)
    .bind(auth.id)
    .bind(status)
    .bind(&note)
    .fetch_optional(&state.db)
    .await?;
    if written.is_none() {
        return Err(ApiError::new(
            409,
            "REVIEW_REVOKED",
            "Esta revisión ha sido retirada. Contacta con la administración para revisarla.",
        ));
    }
    let item = with_reports(&state, item).await?;
    Ok(Json(
        with_verdicts(&state, vec![item], Some(auth.id))
            .await?
            .remove(0),
    ))
}

// ---------- photos ----------

fn invalid_image() -> ApiError {
    ApiError::new(
        400,
        "INVALID_IMAGE",
        "La foto debe ser JPEG y ocupar menos de 1,5 MB.",
    )
}

/// Strict base64 JPEG (canonical encoding, JPEG magic, ≤1.5 MB).
pub fn decode_jpeg(encoded: &str) -> ApiResult<Vec<u8>> {
    let engine = base64::engine::general_purpose::STANDARD;
    let input = engine.decode(encoded).map_err(|_| invalid_image())?;
    if input.len() > 1_500_000
        || input.len() < 100
        || input[..3] != [0xff, 0xd8, 0xff]
        || engine.encode(&input).trim_end_matches('=') != encoded.trim_end_matches('=')
    {
        return Err(invalid_image());
    }
    Ok(input)
}

/// Decode, apply EXIF orientation, fit inside `max`×`max` (never enlarge) and re-encode as JPEG.
pub fn normalize_photo(input: &[u8], max: u32, quality: u8) -> Result<Vec<u8>, String> {
    use image::{ImageDecoder, ImageReader};
    let reader = ImageReader::with_format(std::io::Cursor::new(input), image::ImageFormat::Jpeg);
    let mut decoder = reader.into_decoder().map_err(|e| e.to_string())?;
    let (w, h) = decoder.dimensions();
    if u64::from(w) * u64::from(h) > 10_000_000 {
        return Err("too many pixels".into());
    }
    let orientation = decoder.orientation().map_err(|e| e.to_string())?;
    let mut img = image::DynamicImage::from_decoder(decoder).map_err(|e| e.to_string())?;
    img.apply_orientation(orientation);
    if img.width() > max || img.height() > max {
        img = img.resize(max, max, image::imageops::FilterType::Lanczos3);
    }
    let mut out = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, quality)
        .encode_image(&img.to_rgb8())
        .map_err(|e| e.to_string())?;
    Ok(out)
}

async fn normalize_async(input: Vec<u8>, max: u32, quality: u8) -> ApiResult<Vec<u8>> {
    tokio::task::spawn_blocking(move || normalize_photo(&input, max, quality))
        .await
        .map_err(|_| ApiError::internal())?
        .map_err(|_| {
            ApiError::new(
                400,
                "INVALID_IMAGE",
                "No hemos podido leer la foto. Haz una nueva captura.",
            )
        })
}

async fn upload_photo(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(code): Path<String>,
    JsonBody(body): JsonBody,
) -> ApiResult<Response> {
    let code = util::barcode_param(&code)?;
    let object = util::strict_object(&body, &["name", "ingredients", "imageBase64"])?;
    let name = util::trimmed(util::req_str(object, "name")?, 1, 160)?;
    let ingredients = util::trimmed(util::req_str(object, "ingredients")?, 3, 12000)?;
    let encoded = util::req_str(object, "imageBase64")?;
    util::bounded(encoded, 100, 2_000_000)?;
    let photo = normalize_async(decode_jpeg(encoded)?, 1600, 78).await?;
    let id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO product_contributions(id,barcode,product_name,user_id,ingredients,photo,photo_mime) VALUES($1,$2,$3,$4,$5,$6,'image/jpeg')",
    )
    .bind(id)
    .bind(&code)
    .bind(&name)
    .bind(auth.id)
    .bind(&ingredients)
    .bind(&photo)
    .execute(&state.db)
    .await?;
    let community = community_product(
        &state,
        &code,
        &Contribution {
            id,
            product_name: name,
            ingredients,
        },
    );
    sqlx::query(
        "INSERT INTO products(id,barcode,provider,data,expires_at) VALUES($1,$2,'community',$3,now()+interval '24 hours') ON CONFLICT(barcode) DO NOTHING",
    )
    .bind(format!("off-{code}"))
    .bind(&code)
    .bind(to_json(&community)?)
    .execute(&state.db)
    .await?;
    let item = barcode(&state, &code).await?;
    Ok((StatusCode::CREATED, Json(item)).into_response())
}

async fn ingredients_photo(
    State(state): State<AppState>,
    Path(code): Path<String>,
) -> ApiResult<Response> {
    let code = util::barcode_param(&code)?;
    let photo = sqlx::query_scalar::<_, Vec<u8>>(
        "SELECT photo FROM product_contributions WHERE barcode=$1 ORDER BY created_at DESC LIMIT 1",
    )
    .bind(&code)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(ApiError::not_found)?;
    Ok((
        [
            (header::CONTENT_TYPE, "image/jpeg"),
            (header::CACHE_CONTROL, "public, max-age=300"),
        ],
        Body::from(photo),
    )
        .into_response())
}

/// AI reading of a label photo. The photo is sent to Anthropic and never stored.
async fn analyze_photo(
    State(state): State<AppState>,
    auth: AuthUser,
    JsonBody(body): JsonBody,
) -> ApiResult<Json<LabelAnalysis>> {
    let object = util::strict_object(&body, &["imageBase64", "barcode"])?;
    let encoded = util::req_str(object, "imageBase64")?;
    util::bounded(encoded, 100, 2_000_000)?;
    let code = util::opt_str(object, "barcode")?
        .map(util::barcode_param)
        .transpose()?;
    let ai = state.ai.as_ref().ok_or_else(|| {
        ApiError::new(
            503,
            "AI_NOT_CONFIGURED",
            "El análisis con IA no está activado en el servidor.",
        )
    })?;
    // Claude works best with images up to ~1568 px on the long edge.
    let photo = normalize_async(decode_jpeg(encoded)?, 1568, 85).await?;
    let analysis = crate::providers::ai::combine(ai.extract(&photo).await?);
    sqlx::query("INSERT INTO label_analyses(user_id,barcode,ai_status,final_status,model) VALUES($1,$2,$3,$4,$5)")
        .bind(auth.id)
        .bind(&code)
        .bind(analysis.ai_status.as_str())
        .bind(analysis.assessment.status.as_str())
        .bind(&state.config.anthropic_model)
        .execute(&state.db)
        .await?;
    Ok(Json(analysis))
}

// ---------- personal verdicts ----------

async fn put_verdict(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<String>,
    JsonBody(body): JsonBody,
) -> ApiResult<Json<Product>> {
    let id = product_id(&id)?;
    let object = util::strict_object(&body, &["verdict", "note"])?;
    let verdict = util::req_str(object, "verdict")?;
    if !matches!(verdict, "SAFE" | "UNSAFE") {
        return Err(ApiError::invalid_input());
    }
    let note = util::trimmed(util::opt_str(object, "note")?.unwrap_or(""), 0, 500)?;
    let item = get_product(&state, &id, Some(auth.id)).await?;
    sqlx::query(
        "INSERT INTO user_product_verdicts(user_id,product_id,verdict,note,ingredients_hash) VALUES($1,$2,$3,$4,$5)
         ON CONFLICT(user_id,product_id) DO UPDATE SET verdict=$3,note=$4,ingredients_hash=$5,updated_at=now()",
    )
    .bind(auth.id)
    .bind(&item.id)
    .bind(verdict)
    .bind(&note)
    .bind(ingredients_hash(&item))
    .execute(&state.db)
    .await?;
    Ok(Json(
        with_verdicts(&state, vec![item], Some(auth.id))
            .await?
            .remove(0),
    ))
}

async fn delete_verdict(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<String>,
) -> ApiResult<StatusCode> {
    let id = product_id(&id)?;
    sqlx::query("DELETE FROM user_product_verdicts WHERE user_id=$1 AND product_id=$2")
        .bind(auth.id)
        .bind(&id)
        .execute(&state.db)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

/// "Mis productos": every product with a personal verdict, newest first (from storage, no provider calls).
async fn my_verdicts(
    State(state): State<AppState>,
    auth: AuthUser,
) -> ApiResult<Json<Items<Product>>> {
    let rows = sqlx::query_as::<_, (Value, bool)>(
        "SELECT p.data,p.expires_at<now() FROM user_product_verdicts v JOIN products p ON p.id=v.product_id
         WHERE v.user_id=$1 AND (p.owner_id IS NULL OR p.owner_id=$1) ORDER BY v.updated_at DESC LIMIT 300",
    )
    .bind(auth.id)
    .fetch_all(&state.db)
    .await?;
    let mut items = Vec::with_capacity(rows.len());
    for (data, expired) in rows {
        items.push(hydrate_stored(&state, data, expired).await?);
    }
    Ok(Json(Items {
        items: with_verdicts(&state, items, Some(auth.id)).await?,
    }))
}

// ---------- reviews ----------

async fn list_reviews(
    State(state): State<AppState>,
    MaybeUser(user): MaybeUser,
    Path(id): Path<String>,
) -> ApiResult<Response> {
    util::bounded(&id, 0, 200)?;
    let user = user.map(|u| u.id);
    get_product(&state, &id, user).await?;
    Ok(reviews::list(&state, Kind::Products, &id, None, user)
        .await?
        .into_response())
}

async fn confirm_review(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((id, review)): Path<(String, String)>,
    JsonBody(body): JsonBody,
) -> ApiResult<Json<Value>> {
    util::bounded(&id, 0, 200)?;
    util::uuid_param(&review)?;
    get_product(&state, &id, Some(auth.id)).await?;
    reviews::confirm(&state, Kind::Products, &id, None, &review, auth.id, &body).await
}

pub fn routes(state: &AppState) -> Router<AppState> {
    use crate::app::{limit, photo_body};
    Router::new()
        .route("/api/v1/products/barcode/{code}", get(by_barcode))
        .route(
            "/api/v1/products/search",
            get(search).route_layer(limit(state, SEARCH_LIMIT)),
        )
        .route("/api/v1/products/analyze-text", post(analyze_text))
        .route(
            "/api/v1/products/analyze-photo",
            post(analyze_photo)
                .route_layer(limit(state, AI_LIMIT))
                .layer(photo_body()),
        )
        .route("/api/v1/products/verdicts", get(my_verdicts))
        .route("/api/v1/products/{id}", get(product))
        .route(
            "/api/v1/products/{id}/gluten-report",
            post(gluten_report).route_layer(limit(state, REPORT_LIMIT)),
        )
        .route(
            "/api/v1/products/{id}/ingredients-photo",
            get(ingredients_photo).merge(
                post(upload_photo)
                    .route_layer(limit(state, PHOTO_LIMIT))
                    .layer(photo_body()),
            ),
        )
        .route(
            "/api/v1/products/{id}/verdict",
            put(put_verdict)
                .route_layer(limit(state, VERDICT_LIMIT))
                .merge(axum::routing::delete(delete_verdict)),
        )
        .route("/api/v1/products/{id}/reviews", get(list_reviews))
        .route(
            "/api/v1/products/{id}/reviews/{review}/confirm",
            post(confirm_review).route_layer(limit(state, reviews::CONFIRM_PRODUCT_LIMIT)),
        )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::IngredientLanguage;

    fn french(ingredients: &str) -> ExternalProduct {
        ExternalProduct {
            summary_only: false,
            barcode: "3000000000001".into(),
            name: "Biscuits".into(),
            brand: None,
            image_url: None,
            information_source: InformationSource::Api,
            ingredients: Some(ingredients.into()),
            gluten: GlutenInput {
                ingredients: Some(ingredients.into()),
                source: "Open Food Facts".into(),
                origin: Some(Origin::Provider),
                ..Default::default()
            },
            nutrition: NutritionInput::default(),
            nutri_score: None,
            source: "Open Food Facts".into(),
            source_url: "https://world.openfoodfacts.org/product/3000000000001".into(),
            certification: None,
            language: IngredientLanguage {
                code: Some("fr".into()),
                ..Default::default()
            },
        }
    }

    #[test]
    fn translation_only_makes_the_assessment_more_cautious() {
        // The engine has no French vocabulary: "farine de blé" alone is UNKNOWN.
        let ext = french("Farine de blé, sucre, beurre.");
        assert_eq!(
            from_external(&ext, None).assessment.status,
            GlutenStatus::Unknown
        );
        let t = Translation {
            text: "Harina de trigo, azúcar, mantequilla.".into(),
            language: "fr".into(),
        };
        let item = from_external(&ext, Some(&t));
        assert_eq!(item.assessment.status, GlutenStatus::ContainsGluten);
        assert_eq!(item.ingredients.as_deref(), Some(t.text.as_str()));
        assert_eq!(
            item.ingredients_original.as_deref(),
            Some("Farine de blé, sucre, beurre.")
        );
        assert_eq!(item.ingredients_language.as_deref(), Some("fr"));
        // The verdict hash follows the original, not the rendering.
        assert_eq!(
            ingredients_hash(&item),
            ingredients_hash(&from_external(&ext, None))
        );

        // A translated "sin gluten" never upgrades an UNKNOWN original.
        let claim = Translation {
            text: "Harina de arroz. Sin gluten.".into(),
            language: "fr".into(),
        };
        let ext = french("Farine de riz.");
        assert_eq!(
            from_external(&ext, Some(&claim)).assessment.status,
            GlutenStatus::Unknown
        );
    }
}
