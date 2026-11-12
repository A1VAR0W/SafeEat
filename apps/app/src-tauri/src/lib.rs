//! SafeEat 2.0 app shell: every network call goes through here (reqwest), so the
//! webview never talks to the API directly (no CORS, no tokens in JavaScript).
//!
//! Session:
//! - The access token lives only in memory.
//! - The refresh token (and the signed-in user's profile, to show the session while
//!   offline) are kept in the device's secure storage: Android Keystore / iOS Keychain
//!   (`tauri-plugin-safeeat-secure`). Never in a plain file, preferences or the webview.
//! - On start `restore_session` exchanges the stored refresh token. The server rotates
//!   it on every use, so the new one is stored *before* the new access token is used.
//! - Only an explicit rejection (401) signs the user out; a network failure keeps the
//!   stored session for the next attempt.
//! - On desktop (development) nothing is stored: the session lasts while the app is open.

use std::time::Duration;

use serde::Serialize;
use serde_json::{json, Value};
use tauri::{AppHandle, State};
use tauri_plugin_safeeat_secure::SecureExt;
use tauri_plugin_safeeat_share::ShareExt;
use tokio::sync::Mutex;

/// Build-time API origin (no trailing slash). Debug builds default to preproduction;
/// release builds must set `SAFEEAT_API_URL` (see the assertion below).
const API_URL: &str = match option_env!("SAFEEAT_API_URL") {
    Some(url) => url,
    None => "https://chat-ink.tail552c89.ts.net:10000/preproduction",
};

// A release build must say which server it talks to: without this, an APK meant for
// production would silently use preproduction.
const _: () = assert!(
    cfg!(debug_assertions) || option_env!("SAFEEAT_API_URL").is_some(),
    "set SAFEEAT_API_URL for release builds (the API origin, without a trailing slash)"
);

const KEY_REFRESH: &str = "refresh";
const KEY_USER: &str = "user";

#[derive(Default)]
struct Tokens {
    access: Option<String>,
    refresh: Option<String>,
    /// The signed-in user as the server last returned it.
    user: Option<Value>,
    /// The secure storage has been read once in this run.
    loaded: bool,
}

struct AppState {
    http: reqwest::Client,
    /// Held across a refresh, a login and a logout, so they never interleave and
    /// concurrent 401s refresh only once.
    tokens: Mutex<Tokens>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Failure {
    /// HTTP status, or 0 when the server could not be reached.
    status: u16,
    code: String,
    message: String,
}

impl Failure {
    fn offline() -> Self {
        Failure {
            status: 0,
            code: "OFFLINE".into(),
            message: "No hay conexión con SafeEat. Comprueba tu conexión e inténtalo de nuevo."
                .into(),
        }
    }

    /// The server no longer accepts this session: the UI goes back to signed out.
    fn session_expired() -> Self {
        Failure {
            status: 401,
            code: "SESSION_EXPIRED".into(),
            message: "Tu sesión ha caducado. Inicia sesión de nuevo.".into(),
        }
    }
}

fn url(path: &str) -> String {
    format!("{API_URL}/api/v1{path}")
}

async fn send(
    state: &AppState,
    method: &reqwest::Method,
    path: &str,
    body: Option<&Value>,
    access: Option<&str>,
) -> Result<(u16, Value), Failure> {
    let mut request = state.http.request(method.clone(), url(path));
    if let Some(token) = access {
        request = request.bearer_auth(token);
    }
    if let Some(body) = body {
        request = request.json(body);
    }
    let response = request.send().await.map_err(|_| Failure::offline())?;
    let status = response.status().as_u16();
    let value = if status == 204 {
        Value::Null
    } else {
        response.json::<Value>().await.unwrap_or(Value::Null)
    };
    Ok((status, value))
}

fn failure(status: u16, value: &Value) -> Failure {
    let error = &value["error"];
    Failure {
        status,
        code: error["code"].as_str().unwrap_or("ERROR").to_string(),
        message: error["message"]
            .as_str()
            .unwrap_or("Algo ha fallado. Inténtalo de nuevo.")
            .to_string(),
    }
}

/// Forgets the session everywhere: memory and secure storage.
fn forget(app: &AppHandle, tokens: &mut Tokens) {
    app.secure().remove(KEY_REFRESH);
    app.secure().remove(KEY_USER);
    *tokens = Tokens {
        loaded: true,
        ..Tokens::default()
    };
}

/// Takes a session returned by login, register or refresh. The refresh token is
/// written to secure storage first: if the app dies right after, the stored token is
/// the valid one (the server has already revoked the previous one).
fn adopt(app: &AppHandle, tokens: &mut Tokens, session: &Value) -> bool {
    let (Some(access), Some(refresh)) = (
        session["accessToken"].as_str(),
        session["refreshToken"].as_str(),
    ) else {
        return false;
    };
    app.secure().set(KEY_REFRESH, refresh);
    if !session["user"].is_null() {
        app.secure().set(KEY_USER, &session["user"].to_string());
        tokens.user = Some(session["user"].clone());
    }
    tokens.refresh = Some(refresh.to_string());
    tokens.access = Some(access.to_string());
    tokens.loaded = true;
    true
}

/// Reads the stored session once per run (no network).
fn load(app: &AppHandle, tokens: &mut Tokens) {
    if tokens.loaded {
        return;
    }
    tokens.loaded = true;
    tokens.refresh = app.secure().get(KEY_REFRESH);
    tokens.user = tokens
        .refresh
        .as_ref()
        .and_then(|_| app.secure().get(KEY_USER))
        .and_then(|text| serde_json::from_str(&text).ok());
}

enum Refreshed {
    Yes,
    /// The server rejected the refresh token: signed out.
    Rejected,
    /// No answer (offline, server error): the stored session is kept.
    Unavailable,
}

/// Exchanges the refresh token. Caller holds the tokens lock.
async fn refresh_locked(app: &AppHandle, state: &AppState, tokens: &mut Tokens) -> Refreshed {
    let Some(refresh) = tokens.refresh.clone() else {
        return Refreshed::Rejected;
    };
    let body = json!({ "refreshToken": refresh });
    match send(
        state,
        &reqwest::Method::POST,
        "/auth/refresh",
        Some(&body),
        None,
    )
    .await
    {
        Ok((200, session)) if adopt(app, tokens, &session) => Refreshed::Yes,
        Ok((400 | 401 | 403, _)) => {
            forget(app, tokens);
            Refreshed::Rejected
        }
        _ => Refreshed::Unavailable,
    }
}

/// Generic API call. `path` starts after `/api/v1`, e.g. `/products/barcode/123`.
#[tauri::command]
async fn api(
    app: AppHandle,
    state: State<'_, AppState>,
    method: String,
    path: String,
    body: Option<Value>,
) -> Result<Value, Failure> {
    // Endpoints that return or consume tokens only go through the dedicated commands
    // (`authenticate`, `account_change`, `delete_account`, `logout`), so no token ever
    // reaches the webview.
    const TOKEN_PATHS: [&str; 7] = [
        "/auth/refresh",
        "/auth/login",
        "/auth/register",
        "/auth/logout",
        "/auth/email",
        "/auth/password",
        "/auth/me",
    ];
    let method =
        reqwest::Method::from_bytes(method.as_bytes()).map_err(|_| failure(400, &Value::Null))?;
    let reads_profile = path == "/auth/me" && method == reqwest::Method::GET;
    if !path.starts_with('/')
        || path.contains("..")
        || (TOKEN_PATHS.iter().any(|p| path.starts_with(p)) && !reads_profile)
    {
        return Err(failure(400, &Value::Null));
    }
    authorized(&app, &state, &method, &path, body.as_ref()).await
}

/// A request with the session's access token; refreshes it once when it has expired.
async fn authorized(
    app: &AppHandle,
    state: &AppState,
    method: &reqwest::Method,
    path: &str,
    body: Option<&Value>,
) -> Result<Value, Failure> {
    let (app, method, path) = (app.clone(), method.clone(), path.to_string());
    let body = body.cloned();
    let access = {
        let mut tokens = state.tokens.lock().await;
        load(&app, &mut tokens);
        // A session restored while offline has a refresh token but no access token yet.
        if tokens.access.is_none() && tokens.refresh.is_some() {
            if let Refreshed::Rejected = refresh_locked(&app, state, &mut tokens).await {
                return Err(Failure::session_expired());
            }
        }
        tokens.access.clone()
    };
    let (mut status, mut value) =
        send(state, &method, &path, body.as_ref(), access.as_deref()).await?;
    if status == 401 && access.is_some() {
        let mut tokens = state.tokens.lock().await;
        // Another call may have refreshed (or signed out) while we waited for the lock.
        let fresh = if tokens.access != access {
            tokens.access.is_some()
        } else {
            match refresh_locked(&app, state, &mut tokens).await {
                Refreshed::Yes => true,
                Refreshed::Rejected => return Err(Failure::session_expired()),
                Refreshed::Unavailable => return Err(Failure::offline()),
            }
        };
        let retry = tokens.access.clone();
        drop(tokens);
        if !fresh {
            return Err(Failure::session_expired());
        }
        (status, value) = send(state, &method, &path, body.as_ref(), retry.as_deref()).await?;
    }
    if (200..300).contains(&status) {
        Ok(value)
    } else {
        Err(failure(status, &value))
    }
}

/// Login or register: keeps the tokens and returns only the user.
#[tauri::command]
async fn authenticate(
    app: AppHandle,
    state: State<'_, AppState>,
    mode: String,
    body: Value,
) -> Result<Value, Failure> {
    let path = match mode.as_str() {
        "login" => "/auth/login",
        "register" => "/auth/register",
        _ => return Err(failure(400, &Value::Null)),
    };
    let mut tokens = state.tokens.lock().await;
    load(&app, &mut tokens);
    let (status, session) = send(&state, &reqwest::Method::POST, path, Some(&body), None).await?;
    if !(200..300).contains(&status) {
        return Err(failure(status, &session));
    }
    // A previous account on this device is dropped completely before the new one.
    let previous = tokens.refresh.take();
    forget(&app, &mut tokens);
    if !adopt(&app, &mut tokens, &session) {
        return Err(failure(502, &Value::Null));
    }
    drop(tokens);
    if let Some(refresh) = previous {
        let body = json!({ "refreshToken": refresh });
        let _ = send(
            &state,
            &reqwest::Method::POST,
            "/auth/logout",
            Some(&body),
            None,
        )
        .await;
    }
    Ok(session["user"].clone())
}

/// Change of e-mail (`kind` "email") or password ("password"). The server answers with
/// a new session; only the user goes back to the webview.
#[tauri::command]
async fn account_change(
    app: AppHandle,
    state: State<'_, AppState>,
    kind: String,
    body: Value,
) -> Result<Value, Failure> {
    let path = match kind.as_str() {
        "email" => "/auth/email",
        "password" => "/auth/password",
        _ => return Err(failure(400, &Value::Null)),
    };
    let session = authorized(&app, &state, &reqwest::Method::POST, path, Some(&body)).await?;
    let mut tokens = state.tokens.lock().await;
    if !adopt(&app, &mut tokens, &session) {
        return Err(failure(502, &Value::Null));
    }
    Ok(session["user"].clone())
}

/// Deletes the account on the server and then everything this device keeps about it.
#[tauri::command]
async fn delete_account(
    app: AppHandle,
    state: State<'_, AppState>,
    body: Value,
) -> Result<(), Failure> {
    authorized(
        &app,
        &state,
        &reqwest::Method::DELETE,
        "/auth/me",
        Some(&body),
    )
    .await?;
    let mut tokens = state.tokens.lock().await;
    forget(&app, &mut tokens);
    Ok(())
}

/// Called once when the app starts: the signed-in user, or `null`.
#[tauri::command]
async fn restore_session(app: AppHandle, state: State<'_, AppState>) -> Result<Value, Failure> {
    let mut tokens = state.tokens.lock().await;
    load(&app, &mut tokens);
    if tokens.refresh.is_none() {
        return Ok(Value::Null);
    }
    if tokens.access.is_none() {
        if let Refreshed::Rejected = refresh_locked(&app, &state, &mut tokens).await {
            return Ok(Value::Null);
        }
    }
    // Offline: the stored profile keeps the app signed in; the tokens are exchanged
    // on the first call that reaches the server.
    Ok(tokens.user.clone().unwrap_or(Value::Null))
}

#[tauri::command]
async fn logout(app: AppHandle, state: State<'_, AppState>) -> Result<(), Failure> {
    let mut tokens = state.tokens.lock().await;
    load(&app, &mut tokens);
    let refresh = tokens.refresh.take();
    // The device forgets the session first, even if the server cannot be reached.
    forget(&app, &mut tokens);
    drop(tokens);
    if let Some(refresh) = refresh {
        let body = json!({ "refreshToken": refresh });
        let _ = send(
            &state,
            &reqwest::Method::POST,
            "/auth/logout",
            Some(&body),
            None,
        )
        .await;
    }
    Ok(())
}

/// Creates a public link for a product or restaurant and opens the system share sheet.
/// Returns the link and whether the sheet opened (desktop builds have none: the UI then
/// shows the link so it can be copied).
#[tauri::command]
async fn share_link(
    app: AppHandle,
    state: State<'_, AppState>,
    kind: String,
    id: String,
) -> Result<Value, Failure> {
    let (kind, intro) = match kind.as_str() {
        "products" => ("products", "Mira este producto en SafeEat"),
        "restaurants" => ("restaurants", "Mira este restaurante en SafeEat"),
        _ => return Err(failure(400, &Value::Null)),
    };
    if id.is_empty() || id.len() > 200 || !id.bytes().all(|b| b.is_ascii_graphic()) {
        return Err(failure(400, &Value::Null));
    }
    let path = format!("/shares/{kind}/{}", encode_segment(&id));
    let created = authorized(&app, &state, &reqwest::Method::POST, &path, None).await?;
    let link = created["url"]
        .as_str()
        .filter(|u| u.starts_with("https://") || cfg!(debug_assertions))
        .ok_or_else(|| failure(502, &Value::Null))?
        .to_string();
    let text = format!("{intro}, con sus fuentes: {link}");
    let shared = app.share().text(&text, "Compartir");
    Ok(json!({ "url": link, "shared": shared }))
}

/// Percent-encodes one path segment (ids such as `osm_n_123` or user product UUIDs).
fn encode_segment(value: &str) -> String {
    value
        .bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (b as char).to_string()
            }
            _ => format!("%{b:02X}"),
        })
        .collect()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let http = reqwest::Client::builder()
        .timeout(Duration::from_secs(25))
        .connect_timeout(Duration::from_secs(8))
        .user_agent(concat!("SafeEat/", env!("CARGO_PKG_VERSION")))
        .https_only(API_URL.starts_with("https://"))
        .build()
        .expect("http client");
    let builder = tauri::Builder::default()
        .manage(AppState {
            http,
            tokens: Mutex::new(Tokens::default()),
        })
        .plugin(tauri_plugin_safeeat_secure::init())
        .plugin(tauri_plugin_safeeat_share::init());
    #[cfg(mobile)]
    let builder = builder
        .plugin(tauri_plugin_barcode_scanner::init())
        .plugin(tauri_plugin_geolocation::init())
        .plugin(tauri_plugin_haptics::init());
    builder
        .invoke_handler(tauri::generate_handler![
            api,
            authenticate,
            restore_session,
            account_change,
            delete_account,
            logout,
            share_link
        ])
        .run(tauri::generate_context!())
        .expect("error while running SafeEat");
}
