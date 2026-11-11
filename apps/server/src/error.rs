//! API errors with the same `{ "error": { code, message } }` body as the TypeScript API.

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use safeeat_contracts::{ApiErrorBody, ApiErrorDetail};

#[derive(Debug, Clone)]
pub struct ApiError {
    pub status: StatusCode,
    pub code: &'static str,
    pub message: String,
}

pub type ApiResult<T> = Result<T, ApiError>;

impl ApiError {
    pub fn new(status: u16, code: &'static str, message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::from_u16(status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR),
            code,
            message: message.into(),
        }
    }
    pub fn invalid_input() -> Self {
        Self::new(400, "INVALID_INPUT", "Revisa los datos introducidos.")
    }
    pub fn invalid_request() -> Self {
        Self::new(400, "INVALID_REQUEST", "La solicitud no es válida.")
    }
    pub fn not_found() -> Self {
        Self::new(404, "NOT_FOUND", "No hemos encontrado este elemento.")
    }
    pub fn route_not_found() -> Self {
        Self::new(404, "NOT_FOUND", "No hemos encontrado este recurso.")
    }
    pub fn unauthorized() -> Self {
        Self::new(
            401,
            "UNAUTHORIZED",
            "Tu sesión no es válida. Inicia sesión de nuevo.",
        )
    }
    pub fn rate_limited() -> Self {
        Self::new(
            429,
            "RATE_LIMIT",
            "Demasiadas solicitudes. Espera un momento.",
        )
    }
    pub fn provider_unavailable() -> Self {
        Self::new(
            503,
            "PROVIDER_UNAVAILABLE",
            "La fuente de información no está disponible. Inténtalo de nuevo más tarde.",
        )
    }
    pub fn internal() -> Self {
        Self::new(
            500,
            "INTERNAL_ERROR",
            "No hemos podido completar la operación. Inténtalo más tarde.",
        )
    }
    pub fn is_provider_unavailable(&self) -> bool {
        self.status == StatusCode::SERVICE_UNAVAILABLE
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (
            self.status,
            Json(ApiErrorBody {
                error: ApiErrorDetail {
                    code: self.code.into(),
                    message: self.message,
                },
            }),
        )
            .into_response()
    }
}

impl From<sqlx::Error> for ApiError {
    fn from(error: sqlx::Error) -> Self {
        tracing::error!(error_type = %db_error_kind(&error), "database request failed");
        Self::internal()
    }
}

fn db_error_kind(error: &sqlx::Error) -> String {
    match error {
        sqlx::Error::Database(db) => format!("database:{}", db.code().unwrap_or_default()),
        other => format!("{other:?}").chars().take(80).collect(),
    }
}

/// PostgreSQL unique violation (23505).
pub fn is_unique_violation(error: &sqlx::Error) -> bool {
    matches!(error, sqlx::Error::Database(db) if db.code().as_deref() == Some("23505"))
}
