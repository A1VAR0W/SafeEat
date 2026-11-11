//! Request extractors: JSON bodies and authenticated users.

use axum::body::Bytes;
use axum::extract::{FromRequest, FromRequestParts, Request};
use axum::http::header::AUTHORIZATION;
use axum::http::request::Parts;
use jsonwebtoken::{Algorithm, Validation};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use crate::error::ApiError;
use crate::state::AppState;

/// Raw JSON body. Syntax errors are `INVALID_REQUEST`; an empty body is `null`
/// (so schema validation reports `INVALID_INPUT`, as with Fastify + Zod).
pub struct JsonBody(pub Value);

impl<S: Send + Sync> FromRequest<S> for JsonBody {
    type Rejection = ApiError;

    async fn from_request(request: Request, state: &S) -> Result<Self, Self::Rejection> {
        let bytes = Bytes::from_request(request, state)
            .await
            .map_err(|rejection| {
                if rejection.status() == axum::http::StatusCode::PAYLOAD_TOO_LARGE {
                    ApiError::new(413, "INVALID_REQUEST", "La solicitud no es válida.")
                } else {
                    ApiError::invalid_request()
                }
            })?;
        if bytes.iter().all(u8::is_ascii_whitespace) {
            return Ok(JsonBody(Value::Null));
        }
        serde_json::from_slice(&bytes)
            .map(JsonBody)
            .map_err(|_| ApiError::invalid_request())
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Claims {
    pub sub: String,
    pub sid: String,
    pub iat: i64,
    pub exp: i64,
    pub iss: String,
    pub aud: String,
}

/// What an access token must contain; `iss`/`aud` are validated only when present.
#[derive(Debug, Deserialize)]
struct IncomingClaims {
    sub: String,
    sid: String,
    iss: Option<String>,
    aud: Option<Value>,
}

/// An authenticated user with a live session.
#[derive(Debug, Clone, Copy)]
pub struct AuthUser {
    pub id: Uuid,
    pub sid: Uuid,
}

pub fn verify_token(state: &AppState, header: Option<&str>) -> Result<AuthUser, ApiError> {
    let token = header
        .and_then(|h| {
            h.strip_prefix("Bearer ")
                .or_else(|| h.strip_prefix("bearer "))
        })
        .map(str::trim)
        .ok_or_else(ApiError::unauthorized)?;
    decode_access(token, &state.jwt_decoding)
}

fn decode_access(token: &str, key: &jsonwebtoken::DecodingKey) -> Result<AuthUser, ApiError> {
    let mut validation = Validation::new(Algorithm::HS256);
    // Checked by hand below: tokens issued by the TypeScript API carry no iss/aud even though
    // it was configured to add them, and they must keep working across the 2.0 switch.
    validation.validate_aud = false;
    validation.leeway = 0;
    let data = jsonwebtoken::decode::<IncomingClaims>(token, key, &validation)
        .map_err(|_| ApiError::unauthorized())?;
    let aud_ok = match &data.claims.aud {
        None => true,
        Some(Value::String(aud)) => aud == "safeeat-mobile",
        Some(Value::Array(auds)) => auds.iter().any(|a| a == "safeeat-mobile"),
        Some(_) => false,
    };
    if !aud_ok
        || data
            .claims
            .iss
            .as_deref()
            .is_some_and(|iss| iss != "safeeat")
    {
        return Err(ApiError::unauthorized());
    }
    Ok(AuthUser {
        id: Uuid::parse_str(&data.claims.sub).map_err(|_| ApiError::unauthorized())?,
        sid: Uuid::parse_str(&data.claims.sid).map_err(|_| ApiError::unauthorized())?,
    })
}

async fn session_alive(state: &AppState, user: &AuthUser) -> Result<bool, ApiError> {
    Ok(sqlx::query_scalar::<_, i32>(
        "SELECT 1 FROM refresh_sessions WHERE id=$1 AND user_id=$2 AND revoked_at IS NULL AND expires_at>now()",
    )
    .bind(user.sid)
    .bind(user.id)
    .fetch_optional(&state.db)
    .await?
    .is_some())
}

impl FromRequestParts<AppState> for AuthUser {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let header = parts
            .headers
            .get(AUTHORIZATION)
            .and_then(|v| v.to_str().ok());
        let user = verify_token(state, header)?;
        if !session_alive(state, &user).await? {
            return Err(ApiError::unauthorized());
        }
        Ok(user)
    }
}

/// Authenticated when an `Authorization` header is present (an invalid one is a 401).
pub struct MaybeUser(pub Option<AuthUser>);

impl FromRequestParts<AppState> for MaybeUser {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        if parts.headers.get(AUTHORIZATION).is_none() {
            return Ok(MaybeUser(None));
        }
        AuthUser::from_request_parts(parts, state)
            .await
            .map(|u| MaybeUser(Some(u)))
    }
}

/// Role checks.
pub async fn has_role(state: &AppState, user: Uuid, role: &str) -> Result<bool, ApiError> {
    Ok(
        sqlx::query_scalar::<_, i32>("SELECT 1 FROM user_roles WHERE user_id=$1 AND role_code=$2")
            .bind(user)
            .bind(role)
            .fetch_optional(&state.db)
            .await?
            .is_some(),
    )
}

/// ADMIN or authorized_celiac.
pub async fn is_trusted(state: &AppState, user: Uuid) -> Result<bool, ApiError> {
    Ok(sqlx::query_scalar::<_, i32>(
        "SELECT 1 FROM user_roles WHERE user_id=$1 AND role_code IN ('ADMIN','authorized_celiac') LIMIT 1",
    )
    .bind(user)
    .fetch_optional(&state.db)
    .await?
    .is_some())
}

/// Query string as a map (never rejects; values are validated by the handlers, like Zod).
pub struct QueryMap(pub std::collections::HashMap<String, String>);

impl<S: Send + Sync> FromRequestParts<S> for QueryMap {
    type Rejection = std::convert::Infallible;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        let query = parts.uri.query().unwrap_or("");
        Ok(QueryMap(
            url::form_urlencoded::parse(query.as_bytes())
                .into_owned()
                .collect(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use jsonwebtoken::{encode, DecodingKey, EncodingKey, Header};
    use serde_json::json;

    const SECRET: &[u8] = b"test-secret-test-secret-test-secret-test-secret";

    fn token(extra: Value, exp_offset: i64) -> String {
        let now = chrono::Utc::now().timestamp();
        let mut claims =
            json!({"sub": Uuid::nil(), "sid": Uuid::nil(), "iat": now, "exp": now + exp_offset});
        claims
            .as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        encode(
            &Header::default(),
            &claims,
            &EncodingKey::from_secret(SECRET),
        )
        .unwrap()
    }

    fn accepted(extra: Value, exp_offset: i64) -> bool {
        decode_access(&token(extra, exp_offset), &DecodingKey::from_secret(SECRET)).is_ok()
    }

    #[test]
    fn accepts_typescript_tokens_without_issuer_or_audience() {
        assert!(accepted(json!({}), 600));
        assert!(accepted(
            json!({"iss": "safeeat", "aud": "safeeat-mobile"}),
            600
        ));
        assert!(accepted(json!({"aud": ["other", "safeeat-mobile"]}), 600));
    }

    #[test]
    fn rejects_foreign_or_expired_tokens() {
        assert!(!accepted(json!({"iss": "someone-else"}), 600));
        assert!(!accepted(json!({"aud": "web"}), 600));
        assert!(!accepted(json!({}), -5));
        let forged = encode(&Header::default(), &json!({"sub": Uuid::nil(), "sid": Uuid::nil(), "exp": chrono::Utc::now().timestamp() + 600}), &EncodingKey::from_secret(b"another-secret")).unwrap();
        assert!(decode_access(&forged, &DecodingKey::from_secret(SECRET)).is_err());
    }
}
