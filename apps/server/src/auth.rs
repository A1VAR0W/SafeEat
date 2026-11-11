//! Accounts and sessions (port of apps/api/src/modules/auth).
//!
//! Compatible with sessions and passwords created by the TypeScript API:
//! HS256 access tokens (10 min), 48-byte refresh tokens stored as HMAC-SHA256,
//! rotation with family revocation, and Argon2id PHC hashes.

use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use argon2::{Algorithm, Argon2, Params, Version};
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::{DateTime, Utc};
use hmac::{Hmac, Mac};
use once_cell::sync::Lazy;
use rand::RngCore;
use safeeat_contracts::{Role, Session, User};
use sha2::Sha256;
use sqlx::{Postgres, Transaction};
use uuid::Uuid;

use crate::error::{is_unique_violation, ApiError, ApiResult};
use crate::extract::{AuthUser, Claims, JsonBody};
use crate::ratelimit::{per_minute, Limit};
use crate::state::AppState;
use crate::util::{self, iso};

pub const AUTH_LIMIT: Limit = per_minute("auth", 10);

pub const USER_COLUMNS: &str = "id,email,password_hash,first_name,last_name,date_of_birth::text AS date_of_birth,product_updates_consent,product_updates_consent_at,product_updates_consent_version,COALESCE((SELECT array_agg(role_code ORDER BY role_code) FROM user_roles WHERE user_id=users.id),'{}') AS roles";

#[derive(sqlx::FromRow, Clone)]
pub struct UserRow {
    pub id: Uuid,
    pub email: String,
    pub password_hash: String,
    pub first_name: Option<String>,
    pub last_name: Option<String>,
    pub date_of_birth: Option<String>,
    pub product_updates_consent: bool,
    pub product_updates_consent_at: Option<DateTime<Utc>>,
    pub product_updates_consent_version: Option<String>,
    pub roles: Vec<String>,
}

pub fn public_user(user: &UserRow) -> User {
    User {
        id: user.id.to_string(),
        email: user.email.clone(),
        roles: user
            .roles
            .iter()
            .filter_map(|r| match r.as_str() {
                "ADMIN" => Some(Role::Admin),
                "authorized_celiac" => Some(Role::AuthorizedCeliac),
                _ => None,
            })
            .collect(),
        first_name: user.first_name.clone(),
        last_name: user.last_name.clone(),
        date_of_birth: user.date_of_birth.clone(),
        product_updates_consent: user.product_updates_consent,
        product_updates_consent_at: user.product_updates_consent_at.map(iso),
        product_updates_consent_version: user.product_updates_consent_version.clone(),
    }
}

fn argon() -> Argon2<'static> {
    Argon2::new(
        Algorithm::Argon2id,
        Version::V0x13,
        Params::new(19456, 2, 1, Some(32)).expect("argon2 params"),
    )
}

pub async fn hash_password(password: String) -> ApiResult<String> {
    tokio::task::spawn_blocking(move || {
        let salt = SaltString::generate(&mut rand::rngs::OsRng);
        argon()
            .hash_password(password.as_bytes(), &salt)
            .map(|h| h.to_string())
    })
    .await
    .map_err(|_| ApiError::internal())?
    .map_err(|_| ApiError::internal())
}

pub async fn verify_password(hash: String, password: String) -> bool {
    tokio::task::spawn_blocking(move || {
        PasswordHash::new(&hash)
            .map(|parsed| {
                argon()
                    .verify_password(password.as_bytes(), &parsed)
                    .is_ok()
            })
            .unwrap_or(false)
    })
    .await
    .unwrap_or(false)
}

static DUMMY_HASH: Lazy<String> = Lazy::new(|| {
    let mut bytes = [0u8; 32];
    rand::rngs::OsRng.fill_bytes(&mut bytes);
    let salt = SaltString::generate(&mut rand::rngs::OsRng);
    argon()
        .hash_password(hex::encode(bytes).as_bytes(), &salt)
        .expect("dummy hash")
        .to_string()
});

fn refresh_hash(state: &AppState, token: &str) -> String {
    let mut mac = Hmac::<Sha256>::new_from_slice(state.config.jwt_refresh_secret.as_bytes())
        .expect("hmac key");
    mac.update(token.as_bytes());
    hex::encode(mac.finalize().into_bytes())
}

fn random_hex(bytes: usize) -> String {
    let mut buffer = vec![0u8; bytes];
    rand::rngs::OsRng.fill_bytes(&mut buffer);
    hex::encode(buffer)
}

fn access_token(state: &AppState, user: Uuid, sid: Uuid) -> ApiResult<String> {
    let iat = Utc::now().timestamp();
    let claims = Claims {
        sub: user.to_string(),
        sid: sid.to_string(),
        iat,
        exp: iat + 600,
        iss: "safeeat".into(),
        aud: "safeeat-mobile".into(),
    };
    jsonwebtoken::encode(
        &jsonwebtoken::Header::new(jsonwebtoken::Algorithm::HS256),
        &claims,
        &state.jwt_encoding,
    )
    .map_err(|_| ApiError::internal())
}

fn tokens(state: &AppState, user: &UserRow, sid: Uuid, refresh: String) -> ApiResult<Session> {
    Ok(Session {
        access_token: access_token(state, user.id, sid)?,
        refresh_token: refresh,
        user: public_user(user),
    })
}

pub async fn lock_user(tx: &mut Transaction<'_, Postgres>, user: Uuid) -> ApiResult<()> {
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1,0))")
        .bind(format!("user:{user}"))
        .execute(&mut **tx)
        .await?;
    Ok(())
}

async fn create_session(
    state: &AppState,
    tx: &mut Transaction<'_, Postgres>,
    user: &UserRow,
) -> ApiResult<Session> {
    let id = Uuid::new_v4();
    let token = random_hex(48);
    sqlx::query("INSERT INTO refresh_sessions(id,user_id,family_id,token_hash,expires_at) VALUES($1,$2,$1,$3,now()+interval '30 days')")
        .bind(id)
        .bind(user.id)
        .bind(refresh_hash(state, &token))
        .execute(&mut **tx)
        .await?;
    tokens(state, user, id, token)
}

pub async fn load_user<'e, E: sqlx::PgExecutor<'e>>(
    db: E,
    id: Uuid,
    for_update: bool,
) -> ApiResult<Option<UserRow>> {
    let sql = format!(
        "SELECT {USER_COLUMNS} FROM users WHERE id=$1{}",
        if for_update { " FOR UPDATE" } else { "" }
    );
    Ok(sqlx::query_as::<_, UserRow>(&sql)
        .bind(id)
        .fetch_optional(db)
        .await?)
}

/// Lock the user, check the session is still alive and return the row.
pub async fn current_user(
    tx: &mut Transaction<'_, Postgres>,
    auth: &AuthUser,
) -> ApiResult<UserRow> {
    lock_user(tx, auth.id).await?;
    let alive = sqlx::query_scalar::<_, i32>(
        "SELECT 1 FROM refresh_sessions WHERE id=$1 AND user_id=$2 AND revoked_at IS NULL AND expires_at>now()",
    )
    .bind(auth.sid)
    .bind(auth.id)
    .fetch_optional(&mut **tx)
    .await?;
    if alive.is_none() {
        return Err(ApiError::unauthorized());
    }
    load_user(&mut **tx, auth.id, true)
        .await?
        .ok_or_else(ApiError::unauthorized)
}

async fn reauthenticate(
    tx: &mut Transaction<'_, Postgres>,
    auth: &AuthUser,
    password: &str,
) -> ApiResult<UserRow> {
    let user = current_user(tx, auth).await?;
    if !verify_password(user.password_hash.clone(), password.to_string()).await {
        return Err(ApiError::new(
            403,
            "REAUTHENTICATION_FAILED",
            "La contraseña actual no es correcta.",
        ));
    }
    Ok(user)
}

fn password(value: &str, min: usize) -> ApiResult<String> {
    util::bounded(value, min, 128)?;
    Ok(value.to_string())
}

fn refresh_token(body: &serde_json::Value) -> ApiResult<String> {
    let object = util::strict_object(body, &["refreshToken"])?;
    let token = util::req_str(object, "refreshToken")?;
    if token.len() != 96
        || !token
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err(ApiError::invalid_input());
    }
    Ok(token.to_string())
}

struct Profile {
    first_name: String,
    last_name: Option<String>,
    date_of_birth: String,
}

fn profile(object: &serde_json::Map<String, serde_json::Value>) -> ApiResult<Profile> {
    let first_name = util::trimmed(util::req_str(object, "firstName")?, 1, 80)?;
    let last_name = util::opt_str(object, "lastName")?
        .map(|v| util::trimmed(v, 0, 120))
        .transpose()?;
    let date_of_birth = util::date_of_birth(util::req_str(object, "dateOfBirth")?)?;
    Ok(Profile {
        first_name,
        last_name: last_name.filter(|v| !v.is_empty()),
        date_of_birth,
    })
}

async fn register(State(state): State<AppState>, JsonBody(body): JsonBody) -> ApiResult<Response> {
    let object = util::strict_object(
        &body,
        &[
            "email",
            "password",
            "firstName",
            "lastName",
            "dateOfBirth",
            "productUpdatesConsent",
        ],
    )?;
    let email = util::email(util::req_str(object, "email")?)?;
    let password = password(util::req_str(object, "password")?, 12)?;
    let profile = profile(object)?;
    let consent = util::opt_bool(object, "productUpdatesConsent")?.unwrap_or(false);
    let hash = hash_password(password).await?;
    let mut tx = state.db.begin().await?;
    let sql = format!(
        "INSERT INTO users(id,email,password_hash,first_name,last_name,date_of_birth,product_updates_consent,product_updates_consent_at,product_updates_consent_version)
         VALUES($1,$2,$3,$4,$5,$6::date,$7,CASE WHEN $7 THEN now() END,CASE WHEN $7 THEN '2026-09-v1' END) RETURNING {USER_COLUMNS}"
    );
    let user = match sqlx::query_as::<_, UserRow>(&sql)
        .bind(Uuid::new_v4())
        .bind(&email)
        .bind(hash)
        .bind(&profile.first_name)
        .bind(&profile.last_name)
        .bind(&profile.date_of_birth)
        .bind(consent)
        .fetch_one(&mut *tx)
        .await
    {
        Ok(user) => user,
        Err(error) if is_unique_violation(&error) => {
            return Err(ApiError::new(
                409,
                "EMAIL_EXISTS",
                "No se puede registrar ese correo. Prueba a iniciar sesión.",
            ))
        }
        Err(error) => return Err(error.into()),
    };
    let session = create_session(&state, &mut tx, &user).await?;
    tx.commit().await?;
    Ok((StatusCode::CREATED, Json(session)).into_response())
}

fn invalid_credentials() -> ApiError {
    ApiError::new(
        401,
        "INVALID_CREDENTIALS",
        "El correo o la contraseña no son correctos.",
    )
}

async fn login(
    State(state): State<AppState>,
    JsonBody(body): JsonBody,
) -> ApiResult<Json<Session>> {
    let object = util::strict_object(&body, &["email", "password"])?;
    let email = util::email(util::req_str(object, "email")?)?;
    let password = password(util::req_str(object, "password")?, 1)?;
    let found = sqlx::query_scalar::<_, Uuid>("SELECT id FROM users WHERE email=$1")
        .bind(&email)
        .fetch_optional(&state.db)
        .await?;
    let Some(id) = found else {
        verify_password(DUMMY_HASH.clone(), password).await;
        return Err(invalid_credentials());
    };
    let mut tx = state.db.begin().await?;
    lock_user(&mut tx, id).await?;
    let user = load_user(&mut *tx, id, false).await?;
    let hash = user
        .as_ref()
        .map(|u| u.password_hash.clone())
        .unwrap_or_else(|| DUMMY_HASH.clone());
    let valid = verify_password(hash, password).await;
    let user = match user {
        Some(user) if valid && user.email == email => user,
        _ => return Err(invalid_credentials()),
    };
    let session = create_session(&state, &mut tx, &user).await?;
    tx.commit().await?;
    Ok(Json(session))
}

#[derive(sqlx::FromRow)]
struct SessionRow {
    id: Uuid,
    user_id: Uuid,
    family_id: Uuid,
    expires_at: DateTime<Utc>,
    revoked_at: Option<DateTime<Utc>>,
}

async fn lock_family(
    state: &AppState,
    tx: &mut Transaction<'_, Postgres>,
    token: &str,
) -> ApiResult<Option<Uuid>> {
    let family = sqlx::query_as::<_, (Uuid, Uuid)>(
        "SELECT family_id,user_id FROM refresh_sessions WHERE token_hash=$1",
    )
    .bind(refresh_hash(state, token))
    .fetch_optional(&mut **tx)
    .await?;
    let Some((family_id, user_id)) = family else {
        return Ok(None);
    };
    lock_user(tx, user_id).await?;
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1,0))")
        .bind(family_id.to_string())
        .execute(&mut **tx)
        .await?;
    Ok(Some(family_id))
}

async fn refresh(
    State(state): State<AppState>,
    JsonBody(body): JsonBody,
) -> ApiResult<Json<Session>> {
    let token = refresh_token(&body)?;
    let mut tx = state.db.begin().await?;
    // Revocations are committed before answering 401.
    let result: Option<Session> = async {
        if lock_family(&state, &mut tx, &token).await?.is_none() {
            return Ok::<_, ApiError>(None);
        }
        let session = sqlx::query_as::<_, SessionRow>(
            "SELECT id,user_id,family_id,expires_at,revoked_at FROM refresh_sessions WHERE token_hash=$1 FOR UPDATE",
        )
        .bind(refresh_hash(&state, &token))
        .fetch_optional(&mut *tx)
        .await?;
        let Some(session) = session else { return Ok(None) };
        if session.revoked_at.is_some() || session.expires_at <= Utc::now() {
            sqlx::query("UPDATE refresh_sessions SET revoked_at=COALESCE(revoked_at,now()) WHERE family_id=$1")
                .bind(session.family_id)
                .execute(&mut *tx)
                .await?;
            return Ok(None);
        }
        let Some(user) = load_user(&mut *tx, session.user_id, false).await? else { return Ok(None) };
        let id = Uuid::new_v4();
        let next = random_hex(48);
        sqlx::query("UPDATE refresh_sessions SET revoked_at=now() WHERE id=$1").bind(session.id).execute(&mut *tx).await?;
        sqlx::query("INSERT INTO refresh_sessions(id,user_id,family_id,token_hash,expires_at) VALUES($1,$2,$3,$4,$5)")
            .bind(id)
            .bind(user.id)
            .bind(session.family_id)
            .bind(refresh_hash(&state, &next))
            .bind(session.expires_at)
            .execute(&mut *tx)
            .await?;
        Ok(Some(tokens(&state, &user, id, next)?))
    }
    .await?;
    tx.commit().await?;
    result.map(Json).ok_or_else(ApiError::unauthorized)
}

async fn logout(State(state): State<AppState>, JsonBody(body): JsonBody) -> ApiResult<StatusCode> {
    let token = refresh_token(&body)?;
    let mut tx = state.db.begin().await?;
    if let Some(family) = lock_family(&state, &mut tx, &token).await? {
        sqlx::query(
            "UPDATE refresh_sessions SET revoked_at=COALESCE(revoked_at,now()) WHERE family_id=$1",
        )
        .bind(family)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn me(State(state): State<AppState>, auth: AuthUser) -> ApiResult<Json<User>> {
    let user = load_user(&state.db, auth.id, false)
        .await?
        .ok_or_else(ApiError::unauthorized)?;
    Ok(Json(public_user(&user)))
}

async fn change(
    state: AppState,
    auth: AuthUser,
    body: serde_json::Value,
    email_change: bool,
) -> ApiResult<Json<Session>> {
    let (current, new_value) = if email_change {
        let object = util::strict_object(&body, &["email", "currentPassword"])?;
        (
            password(util::req_str(object, "currentPassword")?, 1)?,
            util::email(util::req_str(object, "email")?)?,
        )
    } else {
        let object = util::strict_object(&body, &["currentPassword", "newPassword"])?;
        (
            password(util::req_str(object, "currentPassword")?, 1)?,
            password(util::req_str(object, "newPassword")?, 12)?,
        )
    };
    let mut tx = state.db.begin().await?;
    let user = reauthenticate(&mut tx, &auth, &current).await?;
    let changed = if email_change {
        sqlx::query_as::<_, UserRow>(&format!(
            "UPDATE users SET email=$2,updated_at=now() WHERE id=$1 RETURNING {USER_COLUMNS}"
        ))
        .bind(user.id)
        .bind(&new_value)
        .fetch_one(&mut *tx)
        .await
    } else {
        let hash = hash_password(new_value).await?;
        sqlx::query_as::<_, UserRow>(&format!("UPDATE users SET password_hash=$2,updated_at=now() WHERE id=$1 RETURNING {USER_COLUMNS}"))
            .bind(user.id)
            .bind(hash)
            .fetch_one(&mut *tx)
            .await
    };
    let changed = match changed {
        Ok(row) => row,
        Err(error) if is_unique_violation(&error) => {
            return Err(ApiError::new(
                409,
                "EMAIL_EXISTS",
                "No se puede utilizar ese correo.",
            ))
        }
        Err(error) => return Err(error.into()),
    };
    sqlx::query(
        "UPDATE refresh_sessions SET revoked_at=COALESCE(revoked_at,now()) WHERE user_id=$1",
    )
    .bind(user.id)
    .execute(&mut *tx)
    .await?;
    let session = create_session(&state, &mut tx, &changed).await?;
    tx.commit().await?;
    Ok(Json(session))
}

async fn change_email(
    State(state): State<AppState>,
    auth: AuthUser,
    JsonBody(body): JsonBody,
) -> ApiResult<Json<Session>> {
    change(state, auth, body, true).await
}

async fn change_password(
    State(state): State<AppState>,
    auth: AuthUser,
    JsonBody(body): JsonBody,
) -> ApiResult<Json<Session>> {
    change(state, auth, body, false).await
}

async fn complete_profile(
    State(state): State<AppState>,
    auth: AuthUser,
    JsonBody(body): JsonBody,
) -> ApiResult<Json<User>> {
    let object = util::strict_object(&body, &["firstName", "lastName", "dateOfBirth"])?;
    let profile = profile(object)?;
    let mut tx = state.db.begin().await?;
    let user = current_user(&mut tx, &auth).await?;
    if user.first_name.is_some() || user.date_of_birth.is_some() {
        return Err(ApiError::new(
            409,
            "PROFILE_LOCKED",
            "Los datos personales ya están completos y no se pueden modificar.",
        ));
    }
    let updated = sqlx::query_as::<_, UserRow>(&format!(
        "UPDATE users SET first_name=$2,last_name=$3,date_of_birth=$4::date,updated_at=now() WHERE id=$1 RETURNING {USER_COLUMNS}"
    ))
    .bind(user.id)
    .bind(&profile.first_name)
    .bind(&profile.last_name)
    .bind(&profile.date_of_birth)
    .fetch_one(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(Json(public_user(&updated)))
}

async fn delete_account(
    State(state): State<AppState>,
    auth: AuthUser,
    JsonBody(body): JsonBody,
) -> ApiResult<StatusCode> {
    let object = util::strict_object(&body, &["currentPassword", "confirmation"])?;
    let current = password(util::req_str(object, "currentPassword")?, 1)?;
    if util::req_str(object, "confirmation")? != "ELIMINAR" {
        return Err(ApiError::invalid_input());
    }
    let mut tx = state.db.begin().await?;
    reauthenticate(&mut tx, &auth, &current).await?;
    sqlx::query("DELETE FROM users WHERE id=$1")
        .bind(auth.id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

pub fn routes(state: &AppState) -> Router<AppState> {
    Router::new()
        .route("/api/v1/auth/register", post(register))
        .route("/api/v1/auth/login", post(login))
        .route("/api/v1/auth/refresh", post(refresh))
        .route("/api/v1/auth/logout", post(logout))
        .route("/api/v1/auth/me", axum::routing::delete(delete_account))
        .route("/api/v1/auth/email", post(change_email))
        .route("/api/v1/auth/password", post(change_password))
        .route("/api/v1/auth/profile/complete", post(complete_profile))
        .route_layer(crate::app::limit(state, AUTH_LIMIT))
        .route("/api/v1/auth/me", get(me))
}
