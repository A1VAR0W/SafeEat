//! API router (port of apps/api/src/app.ts).

use std::time::Duration;

use axum::extract::{DefaultBodyLimit, Request, State};
use axum::http::{header, HeaderName, HeaderValue, Method, StatusCode};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use serde_json::json;
use tower_http::cors::{AllowOrigin, CorsLayer};

use crate::error::ApiError;
use crate::ratelimit::{self, per_minute, Limit};
use crate::state::AppState;

pub const DEFAULT_BODY_LIMIT: usize = 32_768;
pub const PHOTO_BODY_LIMIT: usize = 2_200_000;
const GLOBAL_LIMIT: Limit = per_minute("global", 100);

type LimitFn =
    fn(State<(AppState, Limit)>, Request, Next) -> futures::future::BoxFuture<'static, Response>;
pub type LimitLayer =
    axum::middleware::FromFnLayer<LimitFn, (AppState, Limit), (State<(AppState, Limit)>, Request)>;

/// A per-route rate limit layer (`config.rateLimit` in Fastify).
pub fn limit(state: &AppState, limit: Limit) -> LimitLayer {
    fn run(
        state: State<(AppState, Limit)>,
        request: Request,
        next: Next,
    ) -> futures::future::BoxFuture<'static, Response> {
        Box::pin(ratelimit::enforce(state, request, next))
    }
    middleware::from_fn_with_state((state.clone(), limit), run as fn(_, _, _) -> _)
}

/// Larger body limit for photo uploads.
pub fn photo_body() -> DefaultBodyLimit {
    DefaultBodyLimit::max(PHOTO_BODY_LIMIT)
}

async fn metadata(State(state): State<AppState>) -> Json<serde_json::Value> {
    Json(json!({"status": "ok", "environment": state.config.app_env, "version": state.version}))
}

async fn ready(State(state): State<AppState>) -> Response {
    match sqlx::query("SELECT 1").execute(&state.db).await {
        Ok(_) => Json(json!({"status": "ready"})).into_response(),
        Err(_) => (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({"status": "unavailable"})),
        )
            .into_response(),
    }
}

async fn not_found() -> ApiError {
    ApiError::route_not_found()
}

/// `Cache-Control: no-store` on every API response except the public ingredient photo.
async fn no_store(request: Request, next: Next) -> Response {
    let path = request.uri().path().to_string();
    let photo = request.method() == Method::GET && path.ends_with("/ingredients-photo");
    let mut response = next.run(request).await;
    if path.starts_with("/api/") && !photo {
        response
            .headers_mut()
            .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    }
    response
}

/// Helmet's default headers (only when a handler did not set its own).
pub async fn security_headers(request: Request, next: Next) -> Response {
    let mut response = next.run(request).await;
    let headers = response.headers_mut();
    for (name, value) in [
        ("content-security-policy", "default-src 'self';base-uri 'self';font-src 'self' https: data:;form-action 'self';frame-ancestors 'self';img-src 'self' data:;object-src 'none';script-src 'self';script-src-attr 'none';style-src 'self' https: 'unsafe-inline';upgrade-insecure-requests"),
        ("cross-origin-opener-policy", "same-origin"),
        ("cross-origin-resource-policy", "same-origin"),
        ("origin-agent-cluster", "?1"),
        ("referrer-policy", "no-referrer"),
        ("strict-transport-security", "max-age=31536000; includeSubDomains"),
        ("x-content-type-options", "nosniff"),
        ("x-dns-prefetch-control", "off"),
        ("x-download-options", "noopen"),
        ("x-frame-options", "SAMEORIGIN"),
        ("x-permitted-cross-domain-policies", "none"),
        ("x-xss-protection", "0"),
    ] {
        let name = HeaderName::from_static(name);
        if !headers.contains_key(&name) {
            headers.insert(name, HeaderValue::from_static(value));
        }
    }
    response
}

fn cors(state: &AppState) -> CorsLayer {
    let origin = url::Url::parse(&state.config.public_app_url)
        .ok()
        .and_then(|u| HeaderValue::from_str(&u.origin().ascii_serialization()).ok());
    let layer = CorsLayer::new()
        .allow_methods([Method::GET, Method::POST, Method::PUT, Method::DELETE])
        .allow_headers([header::CONTENT_TYPE, header::AUTHORIZATION]);
    match origin {
        Some(origin) => layer.allow_origin(AllowOrigin::exact(origin)),
        None => layer,
    }
}

pub fn router(state: AppState) -> Router {
    let api = Router::new()
        .route("/health", get(metadata))
        .route("/api/v1/meta", get(metadata))
        .route("/ready", get(ready))
        .merge(crate::auth::routes(&state))
        .merge(crate::admin::routes(&state))
        .merge(crate::products::routes(&state))
        .merge(crate::restaurants::routes(&state))
        .merge(crate::favorites::routes(&state))
        .merge(crate::sharing::routes(&state))
        .fallback(not_found);
    api.layer(DefaultBodyLimit::max(DEFAULT_BODY_LIMIT))
        .layer(middleware::from_fn(no_store))
        .layer(limit(&state, GLOBAL_LIMIT))
        .layer(middleware::from_fn(security_headers))
        .layer(cors(&state))
        .layer(tower_http::timeout::TimeoutLayer::with_status_code(
            StatusCode::REQUEST_TIMEOUT,
            Duration::from_secs(120),
        ))
        .with_state(state)
}
