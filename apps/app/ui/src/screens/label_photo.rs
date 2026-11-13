//! Photo of an ingredient list. The text is read on the phone (Tesseract, `ocr.js`):
//! the photo never leaves it. She can correct the text; then the server's
//! deterministic engine analyses it. A reading she has not checked against the label
//! never gives a favourable answer (the engine treats it as unchecked OCR).

use leptos::prelude::*;
use leptos::task::spawn_local;
use safeeat_contracts::Product;
use serde::Deserialize;
use serde_json::json;
use wasm_bindgen::prelude::*;

use crate::api;
use crate::icons;
use crate::nav::{use_nav, use_session, Route};
use crate::ui::{ErrorBox, Header, Icon};

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = window, js_name = safeeatOcr, catch)]
    async fn safeeat_ocr(
        file: web_sys::File,
        progress: &js_sys::Function,
    ) -> Result<JsValue, JsValue>;
    #[wasm_bindgen(js_namespace = window, js_name = safeeatOcrStop)]
    fn safeeat_ocr_stop();
}

/// The server's limit for an ingredient text.
const MAX_TEXT: usize = 12000;

pub const OCR_WARNING: &str = "El reconocimiento automático de texto puede cometer errores. Comprueba siempre el etiquetado original.";

#[derive(Deserialize)]
struct Reading {
    text: String,
    confidence: f64,
}

/// How well the photo was read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Legibility {
    /// Almost nothing readable: she must take another photo or type the list.
    Illegible,
    /// Readable, but with doubts: she has to correct it.
    Doubtful,
    Good,
}

/// Tesseract's mean word confidence (0–100) and how much text came out.
pub fn legibility(text: &str, confidence: f64) -> Legibility {
    let letters = text.chars().filter(|c| c.is_alphabetic()).count();
    let words = text
        .split_whitespace()
        .filter(|w| w.chars().filter(|c| c.is_alphabetic()).count() >= 3)
        .count();
    if letters < 12 || words < 2 || confidence < 40.0 {
        Legibility::Illegible
    } else if confidence < 70.0 {
        Legibility::Doubtful
    } else {
        Legibility::Good
    }
}

/// One paragraph: words split across lines ("tri-\ngo") joined, lines and spaces
/// collapsed. Nothing else changes: what was read is what she checks.
pub fn clean_text(raw: &str) -> String {
    let mut joined = String::with_capacity(raw.len());
    let mut lines = raw
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .peekable();
    while let Some(line) = lines.next() {
        let next_starts_lower = lines
            .peek()
            .and_then(|n| n.chars().next())
            .is_some_and(char::is_lowercase);
        let ends_in_letter_hyphen =
            line.ends_with('-') && line.chars().rev().nth(1).is_some_and(char::is_alphabetic);
        if ends_in_letter_hyphen && next_starts_lower {
            joined.push_str(&line[..line.len() - 1]);
        } else {
            joined.push_str(line);
            joined.push(' ');
        }
    }
    let text = joined.split_whitespace().collect::<Vec<_>>().join(" ");
    text.chars().take(MAX_TEXT).collect()
}

/// She may say the text matches the label only once it can be read, or after
/// correcting it herself.
pub fn can_confirm(legibility: Option<Legibility>, edited: bool, text: &str) -> bool {
    text.trim().chars().count() >= 3
        && (edited || legibility.is_some_and(|l| l != Legibility::Illegible))
}

#[derive(Clone, Copy, PartialEq)]
enum Step {
    Idle,
    Preparing,
    Reading(u8),
}

#[component]
pub fn LabelPhotoScreen() -> impl IntoView {
    let nav = use_nav();
    let session = use_session();
    let step = RwSignal::new(Step::Idle);
    let preview = RwSignal::new(None::<String>);
    let text = RwSignal::new(String::new());
    let read_quality = RwSignal::new(None::<Legibility>);
    let edited = RwSignal::new(false);
    let typing = RwSignal::new(false);
    let reviewed = RwSignal::new(false);
    let name = RwSignal::new(String::new());
    let error = RwSignal::new(None::<String>);
    let sending = RwSignal::new(false);

    on_cleanup(move || {
        if let Some(url) = preview.try_get_untracked().flatten() {
            let _ = web_sys::Url::revoke_object_url(&url);
        }
        safeeat_ocr_stop();
    });

    let read = move |file: web_sys::File| {
        if let Some(old) = preview.get_untracked() {
            let _ = web_sys::Url::revoke_object_url(&old);
        }
        preview.set(web_sys::Url::create_object_url_with_blob(&file).ok());
        error.set(None);
        reviewed.set(false);
        edited.set(false);
        step.set(Step::Preparing);
        let progress = Closure::<dyn FnMut(String, f64)>::new(move |phase: String, p: f64| {
            let next = if phase == "read" {
                Step::Reading((p.clamp(0.0, 1.0) * 100.0).round() as u8)
            } else {
                Step::Preparing
            };
            step.try_set(next);
        });
        spawn_local(async move {
            let result = safeeat_ocr(file, progress.as_ref().unchecked_ref()).await;
            drop(progress);
            step.try_set(Step::Idle);
            let reading = result
                .ok()
                .and_then(|v| serde_wasm_bindgen::from_value::<Reading>(v).ok());
            match reading {
                Some(r) => {
                    let cleaned = clean_text(&r.text);
                    read_quality.try_set(Some(legibility(&cleaned, r.confidence)));
                    text.try_set(cleaned);
                    typing.try_set(true);
                }
                None => {
                    read_quality.try_set(None);
                    error.try_set(Some(
                        "No se ha podido leer la foto. Prueba con otra o escribe los ingredientes."
                            .into(),
                    ));
                }
            }
        });
    };
    let on_pick = move |ev: leptos::ev::Event| {
        let input = event_target::<web_sys::HtmlInputElement>(&ev);
        let file = input.files().and_then(|f| f.get(0));
        // The same photo can be chosen again.
        input.set_value("");
        if let Some(file) = file {
            read(file);
        }
    };

    let analyze = move || {
        let body_text = text.get_untracked().trim().to_string();
        if body_text.chars().count() < 3 || sending.get_untracked() {
            return;
        }
        let mut body = json!({
            "text": body_text,
            "reviewed": reviewed.get_untracked(),
        });
        let product_name = name.get_untracked().trim().to_string();
        if !product_name.is_empty() {
            body["name"] = json!(product_name);
        }
        sending.set(true);
        error.set(None);
        spawn_local(async move {
            let result = api::send::<Product>("POST", "/products/analyze-text", Some(body)).await;
            sending.try_set(false);
            match result {
                Ok(p) => nav.push(Route::Product(p.id)),
                Err(e) => {
                    error.try_set(Some(e.message));
                }
            }
        });
    };

    let busy = move || step.get() != Step::Idle;
    let confirmable = move || can_confirm(read_quality.get(), edited.get(), &text.get());

    view! {
        <Header title="Foto de ingredientes".to_string() />
        <section class="stack">
            <p class="muted">
                "Haz una foto solo de la lista de ingredientes, recta y con buena luz. "
                "El texto se lee en tu móvil: la foto no se envía a ningún sitio."
            </p>
            <div class="photo-picks">
                <label class="button" class:disabled=busy>
                    <Icon svg=icons::CAMERA />
                    "Hacer foto"
                    <input
                        class="visually-hidden"
                        type="file"
                        accept="image/*"
                        capture="environment"
                        disabled=busy
                        on:change=on_pick
                    />
                </label>
                <label class="button ghost" class:disabled=busy>
                    <Icon svg=icons::IMAGE />
                    "Elegir de la galería"
                    <input
                        class="visually-hidden"
                        type="file"
                        accept="image/*"
                        disabled=busy
                        on:change=on_pick
                    />
                </label>
            </div>
            {move || preview.get().map(|src| view! {
                <img class="photo-preview" src=src alt="Foto elegida de la lista de ingredientes" />
            })}
            {move || match step.get() {
                Step::Idle => ().into_any(),
                Step::Preparing => view! {
                    <div class="ocr-progress" role="status">
                        <span>"Preparando el lector de texto…"</span>
                        <progress aria-label="Preparando el lector de texto"></progress>
                    </div>
                }.into_any(),
                Step::Reading(p) => view! {
                    <div class="ocr-progress" role="status">
                        <span>{format!("Leyendo el texto… {p} %")}</span>
                        <progress max="100" value=p.to_string() aria-label="Progreso de la lectura"></progress>
                    </div>
                }.into_any(),
            }}
            {move || error.get().map(|m| view! { <ErrorBox message=m /> })}

            <Show
                when=move || typing.get()
                fallback=move || view! {
                    <button class="button ghost small" disabled=busy on:click=move |_| typing.set(true)>
                        "Prefiero escribir los ingredientes"
                    </button>
                }
            >
                <div class="notice tone-caution" role="note">
                    <Icon svg=icons::ALERT />
                    <div class="notice-body"><p>{OCR_WARNING}</p></div>
                </div>
                {move || match read_quality.get() {
                    Some(Legibility::Illegible) if !edited.get() => view! {
                        <div class="notice tone-danger" role="alert">
                            <Icon svg=icons::ALERT />
                            <div class="notice-body">
                                <p><strong>"Apenas se ha podido leer la foto"</strong></p>
                                <p>"Haz otra más cerca, recta y con más luz, o escribe tú los ingredientes."</p>
                            </div>
                        </div>
                    }.into_any(),
                    Some(Legibility::Doubtful) if !edited.get() => view! {
                        <div class="notice tone-caution" role="status">
                            <Icon svg=icons::HELP />
                            <div class="notice-body">
                                <p>"Parte del texto se ha leído con dudas. Corrige lo que no coincida con la etiqueta."</p>
                            </div>
                        </div>
                    }.into_any(),
                    _ => ().into_any(),
                }}
                <form class="card form-row" on:submit=move |ev| { ev.prevent_default(); analyze(); }>
                    <label class="label" for="ocr-text">"Ingredientes (puedes corregirlos)"</label>
                    <textarea
                        id="ocr-text"
                        class="input ocr-text"
                        maxlength=MAX_TEXT.to_string()
                        autocapitalize="sentences"
                        placeholder="Ingredientes: harina de arroz, azúcar…"
                        prop:value=move || text.get()
                        on:input=move |ev| {
                            text.set(event_target_value(&ev));
                            edited.set(true);
                        }
                    ></textarea>
                    <label class="label" for="ocr-name">"Nombre del producto (opcional)"</label>
                    <input
                        id="ocr-name"
                        class="input"
                        maxlength="160"
                        autocomplete="off"
                        prop:value=move || name.get()
                        on:input=move |ev| name.set(event_target_value(&ev))
                    />
                    <label class="check">
                        <input
                            type="checkbox"
                            disabled=move || !confirmable()
                            prop:checked=move || reviewed.get() && confirmable()
                            on:change=move |ev| reviewed.set(event_target_checked(&ev))
                        />
                        "He comparado el texto con la etiqueta y está completo"
                    </label>
                    <Show when=move || !(reviewed.get() && confirmable())>
                        <p class="muted small">
                            "Sin esa comprobación, SafeEat no dará un resultado favorable: la lectura puede haberse saltado palabras."
                        </p>
                    </Show>
                    <Show
                        when=move || session.0.with(Option::is_some)
                        fallback=move || view! {
                            <p class="muted small">"Inicia sesión para analizar el texto."</p>
                            <button type="button" class="button" on:click=move |_| nav.push(Route::Login)>"Iniciar sesión"</button>
                        }
                    >
                        <button
                            class="button"
                            type="submit"
                            disabled=move || sending.get() || busy() || text.with(|t| t.trim().chars().count() < 3)
                        >
                            {move || if sending.get() { "Analizando…" } else { "Analizar ingredientes" }}
                        </button>
                    </Show>
                </form>
            </Show>
        </section>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_split_word_is_joined_and_lines_collapse() {
        assert_eq!(
            clean_text("Ingredientes: harina de tri-\ngo, azúcar,\n\n  sal.\n"),
            "Ingredientes: harina de trigo, azúcar, sal."
        );
        // A hyphen before a capital or a number is kept as it is.
        assert_eq!(clean_text("E-\n471"), "E- 471");
        assert_eq!(clean_text("Agua-\nSal"), "Agua- Sal");
    }

    #[test]
    fn a_long_text_is_cut_to_the_server_limit() {
        assert_eq!(clean_text(&"a ".repeat(20000)).chars().count(), MAX_TEXT);
    }

    #[test]
    fn noise_is_illegible() {
        assert_eq!(legibility("~ | ;; ..", 80.0), Legibility::Illegible);
        assert_eq!(
            legibility("Ingredientes: harina, sal", 20.0),
            Legibility::Illegible
        );
        assert_eq!(
            legibility("Ingredientes: harina, sal", 55.0),
            Legibility::Doubtful
        );
        assert_eq!(
            legibility("Ingredientes: harina, sal", 88.0),
            Legibility::Good
        );
    }

    #[test]
    fn an_illegible_reading_cannot_be_confirmed_unless_corrected() {
        assert!(!can_confirm(Some(Legibility::Illegible), false, "xx yy zz"));
        assert!(can_confirm(Some(Legibility::Illegible), true, "Arroz, sal"));
        assert!(can_confirm(Some(Legibility::Doubtful), false, "Arroz, sal"));
        assert!(!can_confirm(Some(Legibility::Good), true, "  "));
        // Typed by her without a photo.
        assert!(can_confirm(None, true, "Arroz, sal"));
        assert!(!can_confirm(None, false, "Arroz, sal"));
    }
}
