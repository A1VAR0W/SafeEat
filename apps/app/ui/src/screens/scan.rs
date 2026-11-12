use leptos::prelude::*;
use leptos::task::spawn_local;
use safeeat_contracts::{Items, Product};
use serde::{Deserialize, Serialize};

use crate::api;
use crate::icons;
use crate::nav::{use_nav, use_session, Route, Scanning};
use crate::ui::{Empty, ErrorBox, Icon, Loading, ProductRow};

#[derive(Serialize)]
struct ScanArgs {
    windowed: bool,
    formats: [&'static str; 4],
}

#[derive(Deserialize)]
struct Scanned {
    content: String,
}

#[derive(Serialize)]
struct NoArgs {}

/// Only digits, 8/12/13/14 long, like the server validates.
fn valid_code(code: &str) -> bool {
    matches!(code.len(), 8 | 12 | 13 | 14) && code.bytes().all(|b| b.is_ascii_digit())
}

#[component]
pub fn ScanScreen() -> impl IntoView {
    let nav = use_nav();
    let session = use_session();
    let scan_error = RwSignal::new(None::<String>);
    let scan_state = expect_context::<Scanning>();
    let scanning = scan_state.0;
    let code = RwSignal::new(String::new());
    let code_error = RwSignal::new(None::<&'static str>);

    let scan = move |_| {
        if scanning.get_untracked() {
            return;
        }
        scanning.set(true);
        scan_state.1.set_value(false);
        scan_error.set(None);
        spawn_local(async move {
            let permission: Result<serde_json::Value, _> =
                api::invoke("plugin:barcode-scanner|request_permissions", NoArgs {}).await;
            let denied = matches!(&permission, Ok(v) if v.as_str() == Some("denied"));
            if denied {
                scan_error.try_set(Some(
                    "SafeEat no tiene permiso para usar la cámara. Actívalo en los ajustes del móvil o escribe el código abajo."
                        .into(),
                ));
                scanning.set(false);
                return;
            }
            // A history entry so the Android back button cancels the scan.
            if let Ok(h) = window().history() {
                let _ = h.push_state_with_url(&wasm_bindgen::JsValue::NULL, "", None);
            }
            let result: Result<Scanned, _> = api::invoke(
                "plugin:barcode-scanner|scan",
                ScanArgs {
                    windowed: true,
                    formats: ["EAN_13", "EAN_8", "UPC_A", "UPC_E"],
                },
            )
            .await;
            scanning.set(false);
            match result {
                Ok(found) if valid_code(found.content.trim()) => {
                    api::buzz();
                    // Reuse the history entry pushed for the scanner.
                    nav.push_without_history(Route::Product(found.content.trim().to_string()));
                }
                Ok(_) => {
                    nav.drop_history_entry();
                    scan_error.try_set(Some(
                        "Ese código no es de un producto. Prueba otra vez.".into(),
                    ));
                }
                // Cancelled by back/«Cancelar»: that already consumed the history entry.
                Err(_) if scan_state.1.get_value() => {}
                Err(e) => {
                    web_sys::console::warn_1(&format!("scan: {}", e.message).into());
                    nav.drop_history_entry();
                    scan_error.try_set(Some(
                        "No se pudo abrir la cámara. Escribe el código de barras abajo.".into(),
                    ));
                }
            }
        });
    };

    let open_code = move || {
        let value: String = code
            .get_untracked()
            .chars()
            .filter(char::is_ascii_digit)
            .collect();
        if valid_code(&value) {
            code_error.set(None);
            nav.push(Route::Product(value));
        } else {
            code_error.set(Some("El código tiene 8, 12 o 13 números."));
        }
    };

    view! {
        <header class="header home-header">
            <div>
                <p class="eyebrow">"SafeEat"</p>
                <h1 class="title">"¿Tiene gluten?"</h1>
            </div>
            <button
                class="icon-button"
                aria-label=move || if session.0.with(Option::is_some) { "Tu cuenta" } else { "Iniciar sesión" }
                on:click=move |_| {
                    nav.push(if session.0.with_untracked(Option::is_some) { Route::Profile } else { Route::Login })
                }
            >
                <Icon svg=icons::USER />
            </button>
        </header>
        <section class="stack">
            <button class="scan-cta" on:click=scan disabled=move || scanning.get()>
                <span class="scan-cta-icon" inner_html=icons::SCAN></span>
                <span class="scan-cta-text">
                    <strong>{move || if scanning.get() { "Abriendo cámara…" } else { "Escanear código de barras" }}</strong>
                    <span>"Apunta al código del envase"</span>
                </span>
            </button>
            {move || scan_error.get().map(|m| view! { <ErrorBox message=m /> })}

            <form class="card form-row" on:submit=move |ev| { ev.prevent_default(); open_code(); }>
                <label class="label" for="code">
                    <Icon svg=icons::KEYBOARD /> "Escribir el código"
                </label>
                <div class="input-row">
                    <input
                        id="code"
                        class="input"
                        inputmode="numeric"
                        autocomplete="off"
                        enterkeyhint="search"
                        placeholder="8410000000000"
                        prop:value=move || code.get()
                        on:input=move |ev| code.set(event_target_value(&ev))
                        aria-invalid=move || code_error.get().is_some().then_some("true")
                        aria-describedby="code-error"
                    />
                    <button class="button" type="submit">"Ver"</button>
                </div>
                <p id="code-error" class="field-error" role="alert">{move || code_error.get()}</p>
            </form>

            <ProductSearch />
        </section>
    }
}

#[component]
fn ProductSearch() -> impl IntoView {
    let text = RwSignal::new(String::new());
    let only_free = RwSignal::new(false);
    let results = RwSignal::new(None::<Result<Vec<Product>, api::Failure>>);
    let loading = RwSignal::new(false);
    // Each search gets a number; late answers of older searches are ignored.
    let generation = StoredValue::new(0u32);

    let run = move || {
        let q = text.get_untracked().trim().to_string();
        if q.chars().count() < 2 {
            results.set(None);
            return;
        }
        let ticket = generation.get_value() + 1;
        generation.set_value(ticket);
        loading.set(true);
        let free = only_free.get_untracked();
        spawn_local(async move {
            let path = format!(
                "/products/search?q={}{}",
                api::enc(&q),
                if free { "&glutenFree=true" } else { "" }
            );
            let result = api::get::<Items<Product>>(&path).await.map(|r| r.items);
            // The screen may be gone (tab switch) before the answer arrives.
            if generation.try_get_value() == Some(ticket) {
                results.try_set(Some(result));
                loading.try_set(false);
            }
        });
    };

    view! {
        <form class="card form-row" role="search" on:submit=move |ev| { ev.prevent_default(); run(); }>
            <label class="label" for="q"><Icon svg=icons::SEARCH /> "Buscar por nombre"</label>
            <div class="input-row">
                <input
                    id="q"
                    class="input"
                    type="search"
                    enterkeyhint="search"
                    placeholder="Galletas, pan, cerveza…"
                    prop:value=move || text.get()
                    on:input=move |ev| text.set(event_target_value(&ev))
                />
                <button class="button" type="submit">"Buscar"</button>
            </div>
            <label class="check">
                <input
                    type="checkbox"
                    prop:checked=move || only_free.get()
                    on:change=move |ev| { only_free.set(event_target_checked(&ev)); run(); }
                />
                "Solo sin gluten declarado"
            </label>
        </form>
        <Show when=move || loading.get()>
            <Loading label="Buscando productos…" />
        </Show>
        {move || match results.get() {
            None => ().into_any(),
            Some(Err(e)) => view! { <ErrorBox message=e.message /> }.into_any(),
            Some(Ok(items)) if items.is_empty() => view! {
                <Empty title="Sin resultados" text="Prueba con otro nombre o escanea el código." />
            }.into_any(),
            Some(Ok(items)) => view! {
                <ul class="list" aria-label="Resultados">
                    {items.into_iter().map(|p| view! { <li><ProductRow product=p /></li> }).collect_view()}
                </ul>
            }.into_any(),
        }}
    }
}
