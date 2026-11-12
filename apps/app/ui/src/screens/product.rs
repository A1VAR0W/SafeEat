use leptos::prelude::*;
use leptos::task::spawn_local;
use safeeat_contracts::{AiCheck, AiCheckStatus, GlutenStatus, Product, VerdictValue};
use serde_json::json;

use crate::api::{self, Failure};
use crate::icons;
use crate::nav::{use_nav, use_session, Route};
use crate::ui::{ErrorBox, Header, Icon, Loading};
use crate::verdict::{self, status_title, status_tone};

fn path_for(id: &str) -> String {
    if id.bytes().all(|b| b.is_ascii_digit()) {
        format!("/products/barcode/{id}")
    } else {
        format!("/products/{}", api::enc(id))
    }
}

#[component]
pub fn ProductScreen(id: String) -> impl IntoView {
    let state = RwSignal::new(None::<Result<Product, Failure>>);
    let path = StoredValue::new(path_for(&id));
    let load = move || {
        spawn_local(async move {
            let result = api::get::<Product>(&path.get_value()).await;
            state.set(Some(result));
        });
    };
    load();

    // While the server's AI is checking, look again a few times (cheap GET, cached).
    let polls = StoredValue::new(0u8);
    Effect::new(move |_| {
        let pending = state.with(|s| {
            matches!(s, Some(Ok(p)) if p.ai_check.as_ref().is_some_and(|a| a.status == AiCheckStatus::Pending))
        });
        if pending && polls.get_value() < 12 {
            polls.update_value(|n| *n += 1);
            set_timeout(load, std::time::Duration::from_secs(10));
        }
    });

    let title = Signal::derive(move || {
        state.with(|s| match s {
            Some(Ok(p)) => p.name.clone(),
            _ => "Producto".into(),
        })
    });
    view! {
        <Header title=title />
        {move || match state.get() {
            None => view! { <Loading label="Buscando el producto…" /> }.into_any(),
            Some(Err(e)) if e.status == 404 => view! {
                <div class="stack">
                    <div class="notice tone-unknown">
                        <Icon svg=icons::HELP />
                        <div class="notice-body">
                            <p><strong>"No conocemos este producto"</strong></p>
                            <p>"Ni Open Food Facts ni la comunidad lo tienen todavía. Revisa la etiqueta del envase."</p>
                        </div>
                    </div>
                </div>
            }.into_any(),
            Some(Err(e)) => view! { <div class="stack"><ErrorBox message=e.message retry=Callback::new(move |_| { state.set(None); load(); }) /></div> }.into_any(),
            Some(Ok(p)) => view! { <ProductDetail product=p state=state /> }.into_any(),
        }}
    }
}

#[component]
fn ProductDetail(product: Product, state: RwSignal<Option<Result<Product, Failure>>>) -> impl IntoView {
    let p = product;
    let status = p.assessment.status;
    let tone = status_tone(status);
    let summary = verdict::summary(&p);
    view! {
        <article class="stack">
            <div class="product-hero">
                <div class="hero-image">
                    {p.image_url.clone().map(|src| view! { <img src=src alt="" decoding="async" /> })}
                </div>
                <div class="hero-text">
                    <h2 class="product-name">{p.name.clone()}</h2>
                    {p.brand.clone().map(|b| view! { <p class="muted">{b}</p> })}
                    {p.barcode.clone().map(|b| view! { <p class="muted small mono">{b}</p> })}
                </div>
                <FavoriteButton id=p.id.clone() />
            </div>

            // 1. The answer, in one line, big.
            <section class=format!("verdict {}", summary.tone.class()) aria-live="polite">
                <span class="verdict-icon" inner_html=summary.tone.icon()></span>
                <div class="verdict-body">
                    <p class="verdict-text">{summary.text.clone()}</p>
                    {summary.warning.map(|w| view! {
                        <p class="verdict-warning"><span class="icon" inner_html=icons::ALERT></span>{w}</p>
                    })}
                </div>
            </section>

            // 2. Her own verdict.
            <PersonalVerdictCard product=p.clone() state=state />

            // 3. Certified seal / declaration.
            {p.assessment.certification.map(|c| view! {
                <div class="notice tone-safe">
                    <Icon svg=icons::SEAL />
                    <div class="notice-body"><p><strong>{c.label()}</strong></p></div>
                </div>
            })}

            // 4. Automatic analysis.
            <section class=format!("card analysis {}", tone.class())>
                <h3 class="card-title">
                    <span class="icon" inner_html=tone.icon()></span>
                    "Análisis: " {status_title(status)}
                </h3>
                <ul class="reasons">
                    {p.assessment.reasons.iter().map(|r| view! { <li>{r.message.clone()}</li> }).collect_view()}
                </ul>
                {(!p.assessment.evidence.is_empty()).then(|| view! {
                    <details class="evidence">
                        <summary>"¿En qué se basa?"</summary>
                        <ul>
                            {p.assessment.evidence.iter().map(|e| view! { <li>"«" {e.text.clone()} "»"</li> }).collect_view()}
                        </ul>
                    </details>
                })}
                <p class="muted small">
                    "Confianza en la evidencia: " {format!("{:.0} %", p.assessment.confidence * 100.0)}
                </p>
            </section>

            // 5. On-server AI second opinion (only when the analysis found nothing).
            {(status == GlutenStatus::Unknown).then(|| p.ai_check.clone()).flatten().map(|ai| view! { <AiCard ai=ai /> })}

            <Ingredients product=p.clone() />
            <Community product=p.clone() />
            <p class="muted small source">
                "Fuente: " {p.source.clone()}
                {p.stale.then_some(" · datos sin actualizar")}
            </p>
        </article>
    }
}

#[component]
fn AiCard(ai: AiCheck) -> impl IntoView {
    match ai.status {
        AiCheckStatus::Verified => view! {
            <section class="card ai ai-verified">
                <h3 class="card-title"><span class="icon" inner_html=icons::SPARKLES></span>"Verificado por IA"</h3>
                <div class="ai-warning">
                    <span class="icon" inner_html=icons::ALERT></span>
                    <p>{ai.message.clone()}</p>
                </div>
                <p class="muted small">{ai.model.clone()}</p>
            </section>
        }
        .into_any(),
        AiCheckStatus::Suspected => view! {
            <section class="card ai tone-caution">
                <h3 class="card-title"><span class="icon" inner_html=icons::SPARKLES></span>"La IA ve posible gluten"</h3>
                <p>{ai.message.clone()}</p>
            </section>
        }
        .into_any(),
        AiCheckStatus::Pending => view! {
            <section class="card ai tone-unknown" role="status">
                <h3 class="card-title"><span class="spinner" aria-hidden="true"></span>"Comprobando con IA…"</h3>
                <p class="muted">{ai.message.clone()}</p>
            </section>
        }
        .into_any(),
    }
}

#[component]
fn Ingredients(product: Product) -> impl IntoView {
    let show_original = RwSignal::new(false);
    let translated = product.ingredients.clone();
    let original = product.ingredients_original.clone();
    let language = product.ingredients_language.clone().unwrap_or_default();
    let has_original = original.is_some();
    view! {
        <section class="card">
            <h3 class="card-title">"Ingredientes"</h3>
            {match translated {
                None => view! { <p class="muted">"Sin lista de ingredientes. Revisa el envase."</p> }.into_any(),
                Some(text) => {
                    let text = StoredValue::new(text);
                    let original = StoredValue::new(original.unwrap_or_default());
                    view! {
                        <p class="ingredients" lang=move || if show_original.get() { language.clone() } else { "es".into() }>
                            {move || if show_original.get() {
                                allergen_marks(&original.get_value()).into_any()
                            } else {
                                text.get_value().into_any()
                            }}
                        </p>
                    }.into_any()
                }
            }}
            {has_original.then(|| {
                let name = verdict::language_name(&product.ingredients_language.clone().unwrap_or_default());
                view! {
                    <p class="muted small"><Icon svg=icons::LANGUAGES />" Traducido del " {name} " con los nombres oficiales de Open Food Facts."</p>
                    <button class="button ghost small" on:click=move |_| show_original.update(|v| *v = !*v)
                        aria-pressed=move || show_original.get().to_string()>
                        {move || if show_original.get() { "Ver en español" } else { "Ver original" }}
                    </button>
                }
            })}
        </section>
    }
}

#[component]
fn PersonalVerdictCard(product: Product, state: RwSignal<Option<Result<Product, Failure>>>) -> impl IntoView {
    let session = use_session();
    let nav = use_nav();
    let id = StoredValue::new(product.id.clone());
    let busy = RwSignal::new(false);
    let error = RwSignal::new(None::<String>);
    let mine = product.my_verdict.clone();
    let set = move |value: Option<VerdictValue>| {
        if busy.get_untracked() {
            return;
        }
        busy.set(true);
        spawn_local(async move {
            let path = format!("/products/{}/verdict", api::enc(&id.get_value()));
            let result: Result<Product, Failure> = match value {
                Some(v) => {
                    let verdict = if v == VerdictValue::Safe { "SAFE" } else { "UNSAFE" };
                    api::send("PUT", &path, Some(json!({ "verdict": verdict }))).await
                }
                None => api::send("DELETE", &path, None).await,
            };
            busy.set(false);
            match result {
                Ok(p) => state.set(Some(Ok(p))),
                Err(e) => error.set(Some(e.message)),
            }
        });
    };
    view! {
        <section class="card">
            <h3 class="card-title">"Tu veredicto"</h3>
            {match (session.0.with(Option::is_some), mine) {
                (false, _) => view! {
                    <p class="muted">"Inicia sesión para guardar si este producto te sienta bien."</p>
                    <button class="button ghost" on:click=move |_| nav.push(Route::Login)>"Iniciar sesión"</button>
                }.into_any(),
                (true, Some(v)) => view! {
                    {v.ingredients_changed.then(|| view! {
                        <div class="notice tone-caution">
                            <Icon svg=icons::ALERT />
                            <div class="notice-body"><p><strong>"Los ingredientes han cambiado, revísalo."</strong></p></div>
                        </div>
                    })}
                    <p>{if v.verdict == VerdictValue::Safe { "Lo marcaste como seguro para ti." } else { "Lo marcaste como no seguro para ti." }}</p>
                    <button class="button ghost small" disabled=move || busy.get() on:click=move |_| set(None)>"Quitar mi veredicto"</button>
                }.into_any(),
                (true, None) => view! {
                    <div class="two">
                        <button class="button safe" disabled=move || busy.get() on:click=move |_| set(Some(VerdictValue::Safe))>
                            <Icon svg=icons::THUMB_UP />"Me sienta bien"
                        </button>
                        <button class="button danger" disabled=move || busy.get() on:click=move |_| set(Some(VerdictValue::Unsafe))>
                            <Icon svg=icons::THUMB_DOWN />"No es seguro"
                        </button>
                    </div>
                    <p class="muted small">"Solo lo ves tú. No cambia el análisis."</p>
                }.into_any(),
            }}
            {move || error.get().map(|m| view! { <ErrorBox message=m /> })}
        </section>
    }
}

#[component]
fn Community(product: Product) -> impl IntoView {
    let reports = product.gluten_reports.unwrap_or_default();
    let verified = product.verified_gluten_reports.unwrap_or_default();
    view! {
        <section class="card">
            <h3 class="card-title"><span class="icon" inner_html=icons::USERS></span>"Comunidad"</h3>
            {if reports.gluten_free == 0 && reports.contains_gluten == 0 {
                view! { <p class="muted">"Nadie ha revisado este producto todavía."</p> }.into_any()
            } else {
                view! {
                    <ul class="reasons">
                        <li>{format!("Sin gluten: {} ({} de cuentas de confianza)", reports.gluten_free, verified.gluten_free)}</li>
                        <li>{format!("Con gluten: {} ({} de cuentas de confianza)", reports.contains_gluten, verified.contains_gluten)}</li>
                    </ul>
                }.into_any()
            }}
            <p class="muted small">"Lo que dice la comunidad no certifica el producto."</p>
        </section>
    }
}

#[component]
fn FavoriteButton(id: String) -> impl IntoView {
    let session = use_session();
    let nav = use_nav();
    let saved = RwSignal::new(false);
    let path = StoredValue::new(format!("/favorites/products/{}", api::enc(&id)));
    if session.0.with_untracked(Option::is_some) {
        spawn_local(async move {
            if let Ok(v) = api::get::<serde_json::Value>(&path.get_value()).await {
                saved.set(v["saved"].as_bool().unwrap_or(false));
            }
        });
    }
    let toggle = move |_| {
        if session.0.with_untracked(Option::is_none) {
            nav.push(Route::Login);
            return;
        }
        let next = !saved.get_untracked();
        saved.set(next); // optimistic; reverted on error
        spawn_local(async move {
            let method = if next { "POST" } else { "DELETE" };
            if api::send::<serde_json::Value>(method, &path.get_value(), None).await.is_err() {
                saved.set(!next);
            }
        });
    };
    view! {
        <button
            class="icon-button fav"
            class:on=move || saved.get()
            aria-pressed=move || saved.get().to_string()
            aria-label=move || if saved.get() { "Quitar de guardados" } else { "Guardar producto" }
            on:click=toggle
        >
            <Icon svg=icons::HEART />
        </button>
    }
}

/// Open Food Facts marks allergens as `_blé_`: show them in bold instead.
fn allergen_marks(text: &str) -> impl IntoView {
    text.split('_')
        .enumerate()
        .map(|(i, part)| {
            let part = part.to_string();
            if i % 2 == 1 {
                view! { <strong>{part}</strong> }.into_any()
            } else {
                part.into_any()
            }
        })
        .collect_view()
}
