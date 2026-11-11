//! Favorites and collections (port of apps/api/src/modules/favorites).

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get};
use axum::{Json, Router};
use safeeat_contracts::{Collection, Items, Page, Product};
use serde_json::{json, Value};
use uuid::Uuid;

use crate::error::{ApiError, ApiResult};
use crate::extract::{AuthUser, JsonBody, QueryMap};
use crate::products::{get_product, hydrate_stored, with_verdicts};
use crate::restaurants::get_restaurant;
use crate::state::AppState;
use crate::util;

fn item_id(value: &str) -> ApiResult<String> {
    util::bounded(value, 1, 200)?;
    Ok(value.to_string())
}

async fn hydrate_all(
    state: &AppState,
    rows: Vec<(Value, bool)>,
    user: Uuid,
) -> ApiResult<Vec<Product>> {
    let mut items = Vec::with_capacity(rows.len());
    for (data, expired) in rows {
        items.push(hydrate_stored(state, data, expired).await?);
    }
    with_verdicts(state, items, Some(user)).await
}

async fn clear_saved(
    State(state): State<AppState>,
    auth: AuthUser,
    JsonBody(body): JsonBody,
) -> ApiResult<StatusCode> {
    let object = util::strict_object(&body, &["confirmation"])?;
    if util::req_str(object, "confirmation")? != "BORRAR" {
        return Err(ApiError::invalid_input());
    }
    let mut tx = state.db.begin().await?;
    sqlx::query("SELECT id FROM users WHERE id=$1 FOR UPDATE")
        .bind(auth.id)
        .execute(&mut *tx)
        .await?;
    for sql in [
        "DELETE FROM favorite_products WHERE user_id=$1",
        "DELETE FROM saved_collections WHERE user_id=$1",
        "DELETE FROM favorite_restaurants WHERE user_id=$1",
        "DELETE FROM shares WHERE kind='product' AND resource_id IN (SELECT id FROM products WHERE owner_id=$1)",
        "DELETE FROM products WHERE owner_id=$1",
    ] {
        sqlx::query(sql).bind(auth.id).execute(&mut *tx).await?;
    }
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn list_collections(
    State(state): State<AppState>,
    auth: AuthUser,
) -> ApiResult<Json<Items<Collection>>> {
    let rows = sqlx::query_as::<_, (Uuid, String, i64)>(
        "SELECT c.id,c.name,count(cp.product_id)::bigint FROM saved_collections c LEFT JOIN saved_collection_products cp ON cp.collection_id=c.id
         WHERE c.user_id=$1 GROUP BY c.id ORDER BY c.created_at DESC",
    )
    .bind(auth.id)
    .fetch_all(&state.db)
    .await?;
    Ok(Json(Items {
        items: rows
            .into_iter()
            .map(|(id, name, count)| Collection {
                id: id.to_string(),
                name,
                count,
            })
            .collect(),
    }))
}

async fn create_collection(
    State(state): State<AppState>,
    auth: AuthUser,
    JsonBody(body): JsonBody,
) -> ApiResult<Response> {
    let object = util::strict_object(&body, &["name"])?;
    let name = util::trimmed(util::req_str(object, "name")?, 1, 60)?;
    let id = Uuid::new_v4();
    sqlx::query("INSERT INTO saved_collections(id,user_id,name) VALUES($1,$2,$3)")
        .bind(id)
        .bind(auth.id)
        .bind(&name)
        .execute(&state.db)
        .await?;
    Ok((
        StatusCode::CREATED,
        Json(Collection {
            id: id.to_string(),
            name,
            count: 0,
        }),
    )
        .into_response())
}

async fn collections_for_product(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<String>,
) -> ApiResult<Json<Value>> {
    let id = item_id(&id)?;
    let ids = sqlx::query_scalar::<_, Uuid>(
        "SELECT c.id FROM saved_collections c JOIN saved_collection_products cp ON cp.collection_id=c.id WHERE c.user_id=$1 AND cp.product_id=$2",
    )
    .bind(auth.id)
    .bind(&id)
    .fetch_all(&state.db)
    .await?;
    Ok(Json(
        json!({"ids": ids.iter().map(Uuid::to_string).collect::<Vec<_>>()}),
    ))
}

async fn delete_collection(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(collection): Path<String>,
) -> ApiResult<StatusCode> {
    let collection = util::uuid_param(&collection)?;
    sqlx::query("DELETE FROM saved_collections WHERE id=$1 AND user_id=$2")
        .bind(collection)
        .bind(auth.id)
        .execute(&state.db)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn collection_products(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(collection): Path<String>,
) -> ApiResult<Json<Items<Product>>> {
    let collection = util::uuid_param(&collection)?;
    let rows = sqlx::query_as::<_, (Value, bool)>(
        "SELECT p.data,p.expires_at<now() FROM saved_collection_products cp JOIN saved_collections c ON c.id=cp.collection_id
         JOIN products p ON p.id=cp.product_id WHERE c.id=$1 AND c.user_id=$2 ORDER BY cp.added_at DESC LIMIT 100",
    )
    .bind(collection)
    .bind(auth.id)
    .fetch_all(&state.db)
    .await?;
    Ok(Json(Items {
        items: hydrate_all(&state, rows, auth.id).await?,
    }))
}

async fn in_collection(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((collection, id)): Path<(String, String)>,
) -> ApiResult<Json<Value>> {
    let collection = util::uuid_param(&collection)?;
    let id = item_id(&id)?;
    let saved = sqlx::query_scalar::<_, i32>(
        "SELECT 1 FROM saved_collection_products cp JOIN saved_collections c ON c.id=cp.collection_id WHERE c.id=$1 AND c.user_id=$2 AND cp.product_id=$3",
    )
    .bind(collection)
    .bind(auth.id)
    .bind(&id)
    .fetch_optional(&state.db)
    .await?
    .is_some();
    Ok(Json(json!({"saved": saved})))
}

async fn add_to_collection(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((collection, id)): Path<(String, String)>,
) -> ApiResult<StatusCode> {
    let collection = util::uuid_param(&collection)?;
    let item = get_product(&state, &item_id(&id)?, Some(auth.id)).await?;
    sqlx::query(
        "INSERT INTO saved_collection_products(collection_id,product_id) SELECT id,$3 FROM saved_collections WHERE id=$1 AND user_id=$2 ON CONFLICT DO NOTHING",
    )
    .bind(collection)
    .bind(auth.id)
    .bind(&item.id)
    .execute(&state.db)
    .await?;
    sqlx::query(
        "INSERT INTO favorite_products(user_id,product_id) VALUES($1,$2) ON CONFLICT DO NOTHING",
    )
    .bind(auth.id)
    .bind(&item.id)
    .execute(&state.db)
    .await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn remove_from_collection(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((collection, id)): Path<(String, String)>,
) -> ApiResult<StatusCode> {
    let collection = util::uuid_param(&collection)?;
    let id = item_id(&id)?;
    sqlx::query(
        "DELETE FROM saved_collection_products cp USING saved_collections c WHERE cp.collection_id=c.id AND c.id=$1 AND c.user_id=$2 AND cp.product_id=$3",
    )
    .bind(collection)
    .bind(auth.id)
    .bind(&id)
    .execute(&state.db)
    .await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn is_favorite_product(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<String>,
) -> ApiResult<Json<Value>> {
    let id = item_id(&id)?;
    let saved = sqlx::query_scalar::<_, i32>(
        "SELECT 1 FROM favorite_products WHERE user_id=$1 AND product_id=$2",
    )
    .bind(auth.id)
    .bind(&id)
    .fetch_optional(&state.db)
    .await?
    .is_some();
    Ok(Json(json!({"saved": saved})))
}

async fn is_favorite_restaurant(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<String>,
) -> ApiResult<Json<Value>> {
    let id = item_id(&id)?;
    let saved = sqlx::query_scalar::<_, i32>(
        "SELECT 1 FROM favorite_restaurants WHERE user_id=$1 AND restaurant_id=$2",
    )
    .bind(auth.id)
    .bind(&id)
    .fetch_optional(&state.db)
    .await?
    .is_some();
    Ok(Json(json!({"saved": saved})))
}

async fn favorite_products(
    State(state): State<AppState>,
    auth: AuthUser,
    QueryMap(query): QueryMap,
) -> ApiResult<Json<Page<Product>>> {
    let cursor = query.get("cursor").cloned();
    if let Some(c) = &cursor {
        util::bounded(c, 0, 200)?;
    }
    let rows = sqlx::query_as::<_, (Value, String, bool)>(
        "SELECT p.data,p.id,p.expires_at<now() FROM favorite_products f JOIN products p ON p.id=f.product_id
         WHERE f.user_id=$1 AND ($2::text IS NULL OR p.id>$2) ORDER BY p.id LIMIT 51",
    )
    .bind(auth.id)
    .bind(&cursor)
    .fetch_all(&state.db)
    .await?;
    let more = rows.len() > 50;
    let page: Vec<(Value, String, bool)> = rows.into_iter().take(50).collect();
    let next_cursor = if more {
        page.last().map(|r| r.1.clone())
    } else {
        None
    };
    let items = hydrate_all(
        &state,
        page.into_iter().map(|(d, _, e)| (d, e)).collect(),
        auth.id,
    )
    .await?;
    Ok(Json(Page { items, next_cursor }))
}

async fn add_favorite_product(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<String>,
) -> ApiResult<StatusCode> {
    let item = get_product(&state, &item_id(&id)?, Some(auth.id)).await?;
    sqlx::query(
        "INSERT INTO favorite_products(user_id,product_id) VALUES($1,$2) ON CONFLICT DO NOTHING",
    )
    .bind(auth.id)
    .bind(&item.id)
    .execute(&state.db)
    .await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn remove_favorite_product(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<String>,
) -> ApiResult<StatusCode> {
    let id = item_id(&id)?;
    sqlx::query(
        "DELETE FROM saved_collection_products cp USING saved_collections c WHERE cp.collection_id=c.id AND c.user_id=$1 AND cp.product_id=$2",
    )
    .bind(auth.id)
    .bind(&id)
    .execute(&state.db)
    .await?;
    sqlx::query("DELETE FROM favorite_products WHERE user_id=$1 AND product_id=$2")
        .bind(auth.id)
        .bind(&id)
        .execute(&state.db)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn favorite_restaurants(
    State(state): State<AppState>,
    auth: AuthUser,
    QueryMap(query): QueryMap,
) -> ApiResult<Json<Value>> {
    let cursor = query
        .get("cursor")
        .map(|c| util::restaurant_id(c))
        .transpose()?;
    let rows = sqlx::query_scalar::<_, String>(
        "SELECT restaurant_id FROM favorite_restaurants WHERE user_id=$1 AND ($2::text IS NULL OR restaurant_id>$2) ORDER BY restaurant_id LIMIT 51",
    )
    .bind(auth.id)
    .bind(&cursor)
    .fetch_all(&state.db)
    .await?;
    let next_cursor = if rows.len() > 50 {
        Some(rows[49].clone())
    } else {
        None
    };
    let items: Vec<Value> = rows
        .into_iter()
        .take(50)
        .map(|id| json!({"id": id}))
        .collect();
    Ok(Json(json!({"items": items, "nextCursor": next_cursor})))
}

async fn add_favorite_restaurant(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<String>,
) -> ApiResult<StatusCode> {
    let id = util::restaurant_id(&item_id(&id)?)?;
    let item = get_restaurant(&state, &id).await?;
    sqlx::query("INSERT INTO favorite_restaurants(user_id,restaurant_id) VALUES($1,$2) ON CONFLICT DO NOTHING")
        .bind(auth.id)
        .bind(&item.id)
        .execute(&state.db)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn remove_favorite_restaurant(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<String>,
) -> ApiResult<StatusCode> {
    let id = item_id(&id)?;
    sqlx::query("DELETE FROM favorite_restaurants WHERE user_id=$1 AND restaurant_id=$2")
        .bind(auth.id)
        .bind(&id)
        .execute(&state.db)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

pub fn routes(_state: &AppState) -> Router<AppState> {
    Router::new()
        .route("/api/v1/saved", delete(clear_saved))
        .route(
            "/api/v1/collections",
            get(list_collections).post(create_collection),
        )
        .route(
            "/api/v1/collections/for-product/{id}",
            get(collections_for_product),
        )
        .route(
            "/api/v1/collections/{collection}",
            delete(delete_collection),
        )
        .route(
            "/api/v1/collections/{collection}/products",
            get(collection_products),
        )
        .route(
            "/api/v1/collections/{collection}/products/{id}",
            get(in_collection)
                .post(add_to_collection)
                .delete(remove_from_collection),
        )
        .route("/api/v1/favorites/products", get(favorite_products))
        .route(
            "/api/v1/favorites/products/{id}",
            get(is_favorite_product)
                .post(add_favorite_product)
                .delete(remove_favorite_product),
        )
        .route("/api/v1/favorites/restaurants", get(favorite_restaurants))
        .route(
            "/api/v1/favorites/restaurants/{id}",
            get(is_favorite_restaurant)
                .post(add_favorite_restaurant)
                .delete(remove_favorite_restaurant),
        )
}
