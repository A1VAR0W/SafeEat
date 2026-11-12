//! Calls into the Tauri shell (`src-tauri`), which owns the network and the session.

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = ["window", "__TAURI__", "core"], catch, js_name = invoke)]
    async fn tauri_invoke(cmd: &str, args: JsValue) -> Result<JsValue, JsValue>;
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Failure {
    pub status: u16,
    pub code: String,
    pub message: String,
}

impl Failure {
    fn local(message: &str) -> Self {
        Failure {
            status: 0,
            code: "APP".into(),
            message: message.into(),
        }
    }
}

fn to_js<T: Serialize>(value: &T) -> JsValue {
    value
        .serialize(&serde_wasm_bindgen::Serializer::json_compatible())
        .unwrap_or(JsValue::NULL)
}

/// Any Tauri command (also plugin commands such as `plugin:haptics|vibrate`).
pub async fn invoke<T: DeserializeOwned>(cmd: &str, args: impl Serialize) -> Result<T, Failure> {
    match tauri_invoke(cmd, to_js(&args)).await {
        Ok(value) => serde_wasm_bindgen::from_value(value)
            .map_err(|_| Failure::local("Respuesta inesperada del servidor.")),
        Err(error) => Err(serde_wasm_bindgen::from_value(error.clone()).unwrap_or_else(|_| {
            Failure::local(&error.as_string().unwrap_or_else(|| "Algo ha fallado.".into()))
        })),
    }
}

#[derive(Serialize)]
struct ApiArgs<'a> {
    method: &'a str,
    path: &'a str,
    body: Option<serde_json::Value>,
}

pub async fn get<T: DeserializeOwned>(path: &str) -> Result<T, Failure> {
    invoke(
        "api",
        ApiArgs {
            method: "GET",
            path,
            body: None,
        },
    )
    .await
}

pub async fn send<T: DeserializeOwned>(
    method: &str,
    path: &str,
    body: Option<serde_json::Value>,
) -> Result<T, Failure> {
    invoke("api", ApiArgs { method, path, body }).await
}

/// Percent-encodes a query value.
pub fn enc(value: &str) -> String {
    js_sys::encode_uri_component(value).into()
}

/// Short vibration; silently ignored where unsupported (desktop dev builds).
pub fn buzz() {
    leptos::task::spawn_local(async {
        #[derive(Serialize)]
        struct Args {
            duration: u32,
        }
        let _: Result<serde_json::Value, _> =
            invoke("plugin:haptics|vibrate", Args { duration: 40 }).await;
    });
}
