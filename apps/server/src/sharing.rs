//! Share links (port of apps/api/src/modules/sharing).

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use once_cell::sync::Lazy;
use rand::RngCore;
use regex::Regex;
use safeeat_contracts::PublicShare;
use serde_json::json;
use uuid::Uuid;

use crate::error::{ApiError, ApiResult};
use crate::extract::MaybeUser;
use crate::products::get_product;
use crate::ratelimit::{per_minute, Limit};
use crate::restaurants::get_restaurant;
use crate::state::AppState;
use crate::util;

const SHARE_LIMIT: Limit = per_minute("shares", 20);
static TOKEN: Lazy<Regex> = Lazy::new(|| Regex::new(r"^[a-f0-9]{48}$").unwrap());

async fn create(
    state: &AppState,
    kind: &str,
    id: &str,
    owner: Option<Uuid>,
) -> ApiResult<Response> {
    let mut bytes = [0u8; 24];
    rand::thread_rng().fill_bytes(&mut bytes);
    let token = hex::encode(bytes);
    sqlx::query("INSERT INTO shares(token,kind,resource_id,owner_id) VALUES($1,$2,$3,$4)")
        .bind(&token)
        .bind(kind)
        .bind(id)
        .bind(owner)
        .execute(&state.db)
        .await?;
    let prefix = if kind == "product" { "p" } else { "r" };
    let url = format!("{}/{prefix}/{token}", state.config.public_base());
    Ok((
        StatusCode::CREATED,
        Json(json!({"token": token, "url": url})),
    )
        .into_response())
}

async fn share_product(
    State(state): State<AppState>,
    MaybeUser(user): MaybeUser,
    Path(id): Path<String>,
) -> ApiResult<Response> {
    util::bounded(&id, 0, 80)?;
    let user = user.map(|u| u.id);
    let item = get_product(&state, &id, user).await?;
    create(
        &state,
        "product",
        &id,
        if item.user_created { user } else { None },
    )
    .await
}

async fn share_restaurant(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<Response> {
    let id = util::restaurant_id(&id)?;
    get_restaurant(&state, &id).await?;
    create(&state, "restaurant", &id, None).await
}

/// Public contract: no ownership, sessions or personal verdicts.
async fn public_share(
    State(state): State<AppState>,
    Path(token): Path<String>,
) -> ApiResult<Json<PublicShare>> {
    if !TOKEN.is_match(&token) {
        return Err(ApiError::invalid_input());
    }
    let (kind, resource, owner) = sqlx::query_as::<_, (String, String, Option<Uuid>)>(
        "SELECT kind,resource_id,owner_id FROM shares WHERE token=$1",
    )
    .bind(&token)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(ApiError::not_found)?;
    Ok(Json(if kind == "product" {
        PublicShare::Product(Box::new(get_product(&state, &resource, owner).await?))
    } else {
        PublicShare::Restaurant(Box::new(get_restaurant(&state, &resource).await?))
    }))
}

pub fn routes(state: &AppState) -> Router<AppState> {
    use crate::app::limit;
    Router::new()
        .route(
            "/api/v1/shares/products/{id}",
            post(share_product).route_layer(limit(state, SHARE_LIMIT)),
        )
        .route(
            "/api/v1/shares/restaurants/{id}",
            post(share_restaurant).route_layer(limit(state, SHARE_LIMIT)),
        )
        .route("/api/v1/public/shares/{token}", get(public_share))
}
