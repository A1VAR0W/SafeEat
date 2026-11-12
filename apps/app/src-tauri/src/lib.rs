//! SafeEat 2.0 app shell: every network call goes through here (reqwest), so the
//! webview never talks to the API directly (no CORS, no tokens in JavaScript).
//!
//! Session: the access token and the refresh token live only in memory until the
//! secure-storage plugin (Android Keystore / iOS Keychain) exists. Closing the app
//! therefore signs the user out; it never writes a token to a plain file.

use std::time::Duration;

use serde::Serialize;
use serde_json::{json, Value};
use tauri::State;
use tokio::sync::Mutex;

/// Build-time API origin (no trailing slash). Preproduction unless the release
/// build sets `SAFEEAT_API_URL`.
const API_URL: &str = match option_env!("SAFEEAT_API_URL") {
    Some(url) => url,
    None => "https://chat-ink.tail552c89.ts.net:10000/preproduction",
};

#[derive(Default)]
struct Tokens {
    access: Option<String>,
    refresh: Option<String>,
}

struct AppState {
    http: reqwest::Client,
    /// Held across a refresh, so concurrent 401s refresh only once.
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

/// Exchanges the refresh token. Caller holds the tokens lock.
async fn refresh_locked(state: &AppState, tokens: &mut Tokens) -> bool {
    let Some(refresh) = tokens.refresh.clone() else {
        return false;
    };
    let body = json!({ "refreshToken": refresh });
    match send(state, &reqwest::Method::POST, "/auth/refresh", Some(&body), None).await {
        Ok((200, session)) => {
            tokens.access = session["accessToken"].as_str().map(str::to_string);
            tokens.refresh = session["refreshToken"].as_str().map(str::to_string);
            tokens.access.is_some()
        }
        Ok((401, _)) => {
            *tokens = Tokens::default();
            false
        }
        _ => false,
    }
}

/// Generic API call. `path` starts after `/api/v1`, e.g. `/products/barcode/123`.
#[tauri::command]
async fn api(
    state: State<'_, AppState>,
    method: String,
    path: String,
    body: Option<Value>,
) -> Result<Value, Failure> {
    if !path.starts_with('/') || path.contains("..") || path.starts_with("/auth/refresh") {
        return Err(failure(400, &Value::Null));
    }
    let method = reqwest::Method::from_bytes(method.as_bytes()).map_err(|_| failure(400, &Value::Null))?;
    let access = state.tokens.lock().await.access.clone();
    let (mut status, mut value) = send(&state, &method, &path, body.as_ref(), access.as_deref()).await?;
    if status == 401 && access.is_some() {
        let mut tokens = state.tokens.lock().await;
        // Another call may have refreshed while we waited for the lock.
        let fresh = if tokens.access != access {
            tokens.access.is_some()
        } else {
            refresh_locked(&state, &mut tokens).await
        };
        let retry = tokens.access.clone();
        drop(tokens);
        if fresh {
            (status, value) = send(&state, &method, &path, body.as_ref(), retry.as_deref()).await?;
        }
    }
    if (200..300).contains(&status) {
        Ok(value)
    } else {
        Err(failure(status, &value))
    }
}

/// Login or register: stores the tokens in memory and returns only the user.
#[tauri::command]
async fn authenticate(
    state: State<'_, AppState>,
    mode: String,
    body: Value,
) -> Result<Value, Failure> {
    let path = match mode.as_str() {
        "login" => "/auth/login",
        "register" => "/auth/register",
        _ => return Err(failure(400, &Value::Null)),
    };
    let (status, session) = send(&state, &reqwest::Method::POST, path, Some(&body), None).await?;
    if !(200..300).contains(&status) {
        return Err(failure(status, &session));
    }
    let mut tokens = state.tokens.lock().await;
    tokens.access = session["accessToken"].as_str().map(str::to_string);
    tokens.refresh = session["refreshToken"].as_str().map(str::to_string);
    Ok(session["user"].clone())
}

#[tauri::command]
async fn logout(state: State<'_, AppState>) -> Result<(), Failure> {
    let mut tokens = state.tokens.lock().await;
    if let Some(refresh) = tokens.refresh.take() {
        let body = json!({ "refreshToken": refresh });
        let _ = send(&state, &reqwest::Method::POST, "/auth/logout", Some(&body), None).await;
    }
    *tokens = Tokens::default();
    Ok(())
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
    let builder = tauri::Builder::default().manage(AppState {
        http,
        tokens: Mutex::new(Tokens::default()),
    });
    #[cfg(mobile)]
    let builder = builder
        .plugin(tauri_plugin_barcode_scanner::init())
        .plugin(tauri_plugin_geolocation::init())
        .plugin(tauri_plugin_haptics::init());
    builder
        .invoke_handler(tauri::generate_handler![api, authenticate, logout])
        .run(tauri::generate_context!())
        .expect("error while running SafeEat");
}
