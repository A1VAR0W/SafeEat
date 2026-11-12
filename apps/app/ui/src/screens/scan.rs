use leptos::prelude::*;
use leptos::task::spawn_local;
use safeeat_contracts::{Items, Product};
use serde::{Deserialize, Serialize};

use crate::api;
use crate::history::{self, RecentProduct};
use crate::icons;
use crate::nav::{use_nav, use_session, Route, Scanning};
use crate::ui::{Empty, ErrorBox, Icon, Loading, ProductRow};

#[derive(Serialize)]
struct ScanArgs {
    windowed: bool,
    formats: &'static [&'static str],
}

/// Product barcodes. The iOS scanner has no UPC-A (it reads them as EAN-13 with a
/// leading 0) and rejects the whole call if it is asked for it, so it is Android only.
fn scan_formats() -> &'static [&'static str] {
    let android = window()
        .navigator()
        .user_agent()
        .is_ok_and(|ua| ua.contains("Android"));
    if android {
        &["EAN_13", "EAN_8", "UPC_A", "UPC_E"]
    } else {
        &["EAN_13", "EAN_8", "UPC_E"]
    }
}

#[derive(Deserialize)]
struct Scanned {
    content: String,
}

#[derive(Serialize)]
struct NoArgs {}

/// Which of the two big buttons under «Escanear» is open.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    Code,
    Name,
}

/// Only digits, 8/12/13/14 long, like the server validates.
fn valid_code(code: &str) -> bool {
    matches!(code.len(), 8 | 12 | 13 | 14) && code.bytes().all(|b| b.is_ascii_digit())
}

#[component]
pub fn ScanScreen() -> impl IntoView {
    let nav = use_nav();
    let session = use_session();
    let scan_error = RwSignal::new(None::<String>);
    let searches = RwSignal::new(history::recent_searches());
    let scan_state = expect_context::<Scanning>();
    let scanning = scan_state.0;
    let code = RwSignal::new(String::new());
    let code_error = RwSignal::new(None::<&'static str>);
    let mode = RwSignal::new(None::<Mode>);
    let toggle =
        move |m: Mode| mode.update(|current| *current = (*current != Some(m)).then_some(m));
    let code_input = NodeRef::<leptos::html::Input>::new();
    // Opening «Escribir código» goes straight to the keyboard.
    Effect::new(move |_| {
        if let Some(input) = code_input.get() {
            let _ = input.focus();
        }
    });

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
            // The plugin answers `{ "camera": "granted" | "denied" | "prompt" }`.
            let denied = matches!(&permission, Ok(v) if v["camera"].as_str() == Some("denied"));
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
                    formats: scan_formats(),
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

            <div class="action-tiles">
                <button
                    type="button"
                    class="action-tile"
                    class:active=move || mode.get() == Some(Mode::Code)
                    aria-expanded=move || (mode.get() == Some(Mode::Code)).to_string()
                    aria-controls="code-panel"
                    on:click=move |_| toggle(Mode::Code)
                >
                    <span class="action-tile-icon" inner_html=icons::KEYBOARD></span>
                    <strong>"Escribir código"</strong>
                    <span>"Los números bajo las barras"</span>
                </button>
                <button
                    type="button"
                    class="action-tile"
                    class:active=move || mode.get() == Some(Mode::Name)
                    aria-expanded=move || (mode.get() == Some(Mode::Name)).to_string()
                    aria-controls="name-panel"
                    on:click=move |_| toggle(Mode::Name)
                >
                    <span class="action-tile-icon" inner_html=icons::SEARCH></span>
                    <strong>"Buscar por nombre"</strong>
                    <span>"Galletas, pan, cerveza…"</span>
                </button>
            </div>

            <Show when=move || mode.get() == Some(Mode::Code)>
            <form id="code-panel" class="card form-row" on:submit=move |ev| { ev.prevent_default(); open_code(); }>
                <label class="label" for="code">"Código de barras"</label>
                <div class="input-row">
                    <input
                        node_ref=code_input
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
            </Show>

            <Show when=move || mode.get() == Some(Mode::Name)>
                <ProductSearch searches=searches />
            </Show>
            <RecentProducts searches=searches />
        </section>
    }
}

#[component]
fn ProductSearch(searches: RwSignal<Vec<String>>) -> impl IntoView {
    let text = RwSignal::new(String::new());
    let only_free = RwSignal::new(false);
    let results = RwSignal::new(None::<Result<Vec<Product>, api::Failure>>);
    let loading = RwSignal::new(false);
    // Each search gets a number; late answers of older searches are ignored.
    let generation = StoredValue::new(0u32);
    let input = NodeRef::<leptos::html::Input>::new();
    Effect::new(move |_| {
        if let Some(el) = input.get() {
            let _ = el.focus();
        }
    });

    let run = move || {
        let q = text.get_untracked().trim().to_string();
        if q.chars().count() < 2 {
            results.set(None);
            return;
        }
        history::remember_search(&q);
        searches.set(history::recent_searches());
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
        <form id="name-panel" class="card form-row" role="search" on:submit=move |ev| { ev.prevent_default(); run(); }>
            <label class="label" for="q">"Nombre del producto"</label>
            <div class="input-row">
                <input
                    node_ref=input
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
            <Show when=move || results.with(Option::is_none) && searches.with(|s| !s.is_empty())>
                <div class="chips" role="group" aria-label="Búsquedas recientes">
                    {move || searches.get().into_iter().map(|q| {
                        let label = q.clone();
                        view! {
                            <button type="button" class="chip" on:click=move |_| { text.set(q.clone()); run(); }>{label}</button>
                        }
                    }).collect_view()}
                </div>
            </Show>
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

/// Products she opened lately on this device. Opening one always loads the current
/// analysis: the list itself stores no gluten status.
#[component]
fn RecentProducts(searches: RwSignal<Vec<String>>) -> impl IntoView {
    let nav = use_nav();
    let items = RwSignal::new(history::recent_products());
    let asking = RwSignal::new(false);
    let row = move |p: RecentProduct| {
        let id = p.id.clone();
        view! {
            <li>
                <button class="row" on:click=move |_| nav.push(Route::Product(id.clone()))>
                    <span class="thumb">
                        {p.image_url.map(|src| view! { <img src=src alt="" loading="lazy" decoding="async" /> })}
                    </span>
                    <span class="row-body">
                        <span class="row-title">{p.name}</span>
                        {p.brand.map(|b| view! { <span class="muted small">{b}</span> })}
                    </span>
                    <Icon svg=icons::CHEVRON class="chevron" />
                </button>
            </li>
        }
    };
    view! {
        <Show when=move || items.with(|i| !i.is_empty()) || searches.with(|s| !s.is_empty())>
            <section class="stack" aria-labelledby="recent-title">
                <h2 class="section-title" id="recent-title">"Vistos hace poco"</h2>
                <Show when=move || items.with(|i| !i.is_empty())
                    fallback=|| view! { <p class="muted small">"Aún no has abierto ningún producto."</p> }>
                    <p class="muted small">"Ábrelos para ver su análisis actual."</p>
                    <ul class="list">{move || items.get().into_iter().map(row).collect_view()}</ul>
                </Show>
                <Show
                    when=move || asking.get()
                    fallback=move || view! {
                        <button class="button ghost small" on:click=move |_| asking.set(true)>"Borrar historial"</button>
                    }
                >
                    <div class="confirm-box" role="group" aria-label="Borrar historial">
                        <p class="small">"Se borran los productos vistos y las búsquedas de este móvil. Tus guardados no cambian."</p>
                        <div class="two">
                            <button class="button ghost small" on:click=move |_| asking.set(false)>"Cancelar"</button>
                            <button class="button small" on:click=move |_| {
                                history::clear();
                                items.set(Vec::new());
                                searches.set(Vec::new());
                                asking.set(false);
                            }>"Borrar"</button>
                        </div>
                    </div>
                </Show>
            </section>
        </Show>
    }
}
