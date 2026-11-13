use leptos::prelude::*;
use leptos::task::spawn_local;
use safeeat_contracts::{AiOutcome, AiVerdict, InformationSource, Product, VerdictValue};
use serde_json::json;

use crate::api::{self, Failure};
use crate::icons;
use crate::nav::{use_nav, use_session, Route};
use crate::screens::community::{short_date, CommunityReviews, ProductReviewForm, ShareButton};
use crate::ui::{ErrorBox, Header, Icon, Loading};
use crate::verdict::{self, status_title, AiNote};

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
    // Lives here because saving a review re-renders the whole detail.
    let review_saved = RwSignal::new(false);
    let part = RwSignal::new(Part::Analysis);
    let path = StoredValue::new(path_for(&id));
    let load = move || {
        spawn_local(async move {
            // Also runs from the AI poll timer, possibly after leaving the screen.
            let Some(path) = path.try_get_value() else {
                return;
            };
            let result = api::get::<Product>(&path).await;
            if let Ok(p) = &result {
                crate::history::remember_product(p);
            }
            state.try_set(Some(result));
        });
    };
    load();

    // While the server's AI is checking, look again a few times (cheap GET, cached).
    let polls = StoredValue::new(0u8);
    Effect::new(move |_| {
        let pending = state.with(|s| {
            matches!(s, Some(Ok(p)) if verdict::ai_note(p).is_some_and(|a| a.outcome == AiOutcome::Pending))
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
                    <button class="button" on:click=move |_| use_nav().push(Route::LabelPhoto)>
                        <Icon svg=icons::CAMERA />
                        "Leer los ingredientes con una foto"
                    </button>
                </div>
            }.into_any(),
            Some(Err(e)) => view! { <div class="stack"><ErrorBox message=e.message retry=Callback::new(move |_| { state.set(None); load(); }) /></div> }.into_any(),
            Some(Ok(p)) => view! { <ProductDetail product=p state=state review_saved=review_saved part=part /> }.into_any(),
        }}
    }
}

/// The three parts of the detail below the answer.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Part {
    Analysis,
    Ingredients,
    Community,
}

impl Part {
    const ALL: [Part; 3] = [Part::Analysis, Part::Ingredients, Part::Community];

    fn label(self) -> &'static str {
        match self {
            Part::Analysis => "Análisis",
            Part::Ingredients => "Ingredientes",
            Part::Community => "Opiniones",
        }
    }

    fn id(self) -> &'static str {
        match self {
            Part::Analysis => "analysis",
            Part::Ingredients => "ingredients",
            Part::Community => "community",
        }
    }
}

#[component]
fn ProductDetail(
    product: Product,
    state: RwSignal<Option<Result<Product, Failure>>>,
    review_saved: RwSignal<bool>,
    part: RwSignal<Part>,
) -> impl IntoView {
    let p = product;
    let summary = verdict::summary(&p);
    let brief = verdict::brief(&p, &summary);
    let reports = p.gluten_reports.unwrap_or_default();
    let opinions = reports.gluten_free + reports.contains_gluten;
    // Arrow keys move between tabs, as in any tab list.
    let on_key = move |ev: leptos::ev::KeyboardEvent| {
        let step: i32 = match ev.key().as_str() {
            "ArrowRight" => 1,
            "ArrowLeft" => -1,
            _ => return,
        };
        ev.prevent_default();
        let at = Part::ALL
            .iter()
            .position(|x| *x == part.get_untracked())
            .unwrap_or(0) as i32;
        let next = Part::ALL[(at + step).rem_euclid(Part::ALL.len() as i32) as usize];
        part.set(next);
        if let Some(el) = document().get_element_by_id(&format!("tab-{}", next.id())) {
            use wasm_bindgen::JsCast;
            if let Ok(el) = el.dyn_into::<web_sys::HtmlElement>() {
                let _ = el.focus();
            }
        }
    };
    let panel = {
        let p = p.clone();
        move || match part.get() {
            Part::Analysis => view! { <AnalysisPart product=p.clone() /> }.into_any(),
            Part::Ingredients => view! { <Ingredients product=p.clone() /> }.into_any(),
            // Public reviews only make sense for products identified by a barcode; the
            // list replaces the bare counts there.
            Part::Community => {
                if p.barcode.is_some() && !p.user_created {
                    view! {
                        <div class="stack">
                            <ProductReviewForm product=p.clone() state=state saved=review_saved />
                            <CommunityReviews kind="products" id=p.id.clone() />
                        </div>
                    }
                    .into_any()
                } else {
                    view! { <Community product=p.clone() /> }.into_any()
                }
            }
        }
    };
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
                <FavoriteButton kind="products" id=p.id.clone() />
            </div>

            // 1. The answer, in one line, big.
            <section class=format!("verdict {}", summary.tone.class()) aria-live="polite">
                <span class="verdict-icon" inner_html=summary.tone.icon()></span>
                <div class="verdict-body">
                    <p class="verdict-text">{summary.text.clone()}</p>
                    {brief.map(|b| view! { <p class="verdict-brief">{b}</p> })}
                    {summary.warning.map(|w| view! {
                        <p class="verdict-warning"><span class="icon" inner_html=icons::ALERT></span>{w}</p>
                    })}
                </div>
            </section>

            // Old information is said next to the answer, never only inside a tab.
            {verdict::freshness_notice(&p).map(|text| view! {
                <div class="notice tone-caution" role="status">
                    <Icon svg=icons::ALERT />
                    <div class="notice-body"><p><strong>{text}</strong></p></div>
                </div>
            })}

            // A user's label raised the warning: say so next to the answer, never in a tab.
            {p.provider_assessment.clone().map(|a| view! {
                <div class="notice tone-caution">
                    <Icon svg=icons::USERS />
                    <div class="notice-body">
                        <p><strong>"Aviso de la comunidad"</strong></p>
                        <p class="small">
                            "Una etiqueta aportada por un usuario indica más riesgo que los datos del proveedor («"
                            {status_title(a.status)} "»). Mostramos el aviso por precaución."
                        </p>
                    </div>
                </div>
            })}

            // 2. Her own verdict, one row.
            <PersonalVerdictCard product=p.clone() state=state />

            // 3. The rest, one part at a time.
            <div class="tabs" role="tablist" aria-label="Información del producto" on:keydown=on_key>
                {Part::ALL.into_iter().map(|x| {
                    let selected = move || part.get() == x;
                    view! {
                        <button type="button" role="tab" id=format!("tab-{}", x.id())
                            aria-controls=format!("panel-{}", x.id())
                            aria-selected=move || selected().to_string()
                            tabindex=move || if selected() { "0" } else { "-1" }
                            class:active=selected
                            on:click=move |_| part.set(x)>
                            {x.label()}
                            {(x == Part::Community && opinions > 0).then(|| view! { <span class="tab-count">{opinions}</span> })}
                        </button>
                    }
                }).collect_view()}
            </div>
            <div role="tabpanel" class="tab-panel"
                id=move || format!("panel-{}", part.get().id())
                aria-labelledby=move || format!("tab-{}", part.get().id())>
                {panel}
            </div>

            // 4. Keep and share.
            {(!p.user_created).then(|| view! { <CollectionPicker id=p.id.clone() /> })}
            <ShareButton kind="products" id=p.id.clone() />
            <p class="muted small source">
                "Fuente: " {p.source.clone()}
                {p.data_updated_at.as_deref().map(|d| format!(" · datos de la fuente del {}", short_date(d)))}
                {format!(" · consultado el {}", short_date(&p.fetched_at))}
            </p>
        </article>
    }
}

/// Seal, automatic analysis and the AI's second opinion.
#[component]
fn AnalysisPart(product: Product) -> impl IntoView {
    let p = product;
    let tone = verdict::analysis_tone(&p);
    let (level, level_why) = verdict::evidence_level(&p.assessment);
    let groups = verdict::ingredient_groups(&p.assessment.items);
    let reasons = verdict::shown_reasons(&p.assessment);
    let has_details = !groups.is_empty() || !p.assessment.evidence.is_empty();
    view! {
        <div class="stack">
            // Certified seal / declaration, with where it comes from.
            {p.assessment.certification.map(|c| view! {
                <div class=if verdict::community_claim(&p.assessment) || verdict::not_current(&p) { "notice tone-info" } else { "notice tone-safe" }>
                    <Icon svg=icons::SEAL />
                    <div class="notice-body">
                        <p><strong>{c.label()}</strong></p>
                        <p class="small">"Según " {p.source.clone()} ". SafeEat no puede comprobar el certificado: confirma el sello en el envase."</p>
                    </div>
                </div>
            })}
            <section class=format!("card analysis {}", tone.class())>
                <h3 class="card-title">
                    <span class="icon" inner_html=tone.icon()></span>
                    {verdict::analysis_heading(&p)}
                </h3>
                <ul class="reasons">
                    {reasons.into_iter().map(|r| view! { <li>{r}</li> }).collect_view()}
                </ul>
                <p class="muted small">
                    "Evidencia: " <strong>{level}</strong> ". " {level_why}
                </p>
                {has_details.then(|| view! {
                    <details class="evidence full-analysis">
                        <summary>"Ver análisis completo"</summary>
                        {groups.into_iter().map(|g| view! {
                            <section class="analysis-group">
                                <h4 class="group-title">{g.title} <span class="muted">{format!(" ({})", g.items.len())}</span></h4>
                                <ul>
                                    {g.items.into_iter().map(|(text, label)| view! {
                                        <li>{text} {label.map(|l| view! { " · " <span class="muted">{l}</span> })}</li>
                                    }).collect_view()}
                                </ul>
                            </section>
                        }).collect_view()}
                        {(!p.assessment.evidence.is_empty()).then(|| view! {
                            <section class="analysis-group">
                                <h4 class="group-title">"Lo que declara la fuente"</h4>
                                <ul>
                                    {p.assessment.evidence.iter().map(|e| view! { <li>"«" {e.text.clone()} "»"</li> }).collect_view()}
                                </ul>
                            </section>
                        })}
                        <p class="muted small">
                            "Fuente: " {p.source.clone()} " · motor " {p.assessment.engine_version.clone()}
                            ". El texto original está en la pestaña Ingredientes."
                        </p>
                    </details>
                })}
            </section>
            // On-server AI second opinion (only when the analysis left it UNKNOWN).
            {verdict::ai_note(&p).map(|ai| view! { <AiCard ai=ai /> })}
        </div>
    }
}

#[component]
fn AiCard(ai: AiNote) -> impl IntoView {
    if ai.outcome == AiOutcome::Pending {
        return view! {
            <section class="card ai tone-unknown" role="status">
                <h3 class="card-title"><span class="spinner" aria-hidden="true"></span>{ai.title}</h3>
                <p class="muted">{ai.message.clone()}</p>
            </section>
        }
        .into_any();
    }
    let icon = if ai.outcome == AiOutcome::NoGlutenSeen {
        icons::SPARKLES
    } else {
        icons::HELP
    };
    view! {
        <section class=format!("card ai {}", ai.tone.class())>
            <h3 class="card-title"><span class="icon" inner_html=icon></span>{ai.title}</h3>
            <p>{ai.message.clone()}</p>
            {(!ai.items.is_empty()).then(|| view! {
                <ul class="reasons">
                    {ai.items.iter().map(|i| view! { <li>{i.explanation.clone()}</li> }).collect_view()}
                </ul>
            })}
            {ai.items.iter().any(|i| i.verdict == AiVerdict::KnownGluten).then(|| view! {
                <p class="muted small">"Una sospecha de la IA no es una advertencia del fabricante: compruébalo en el envase."</p>
            })}
            <div class="ai-warning">
                <span class="icon" inner_html=icons::ALERT></span>
                <p>"No es una certificación. La IA puede equivocarse y no detecta trazas ni contaminación cruzada. Revisa siempre la etiqueta."</p>
            </div>
            <p class="muted small">{ai.model.clone()}</p>
        </section>
    }
    .into_any()
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
            // A user's label is shown next to the provider's list, never instead of it.
            {product.community_ingredients.clone()
                .filter(|label| product.ingredients.as_deref() != Some(label.as_str()) || product.information_source == InformationSource::Community)
                .map(|label| view! {
                    <div class="community-label">
                        <p class="label"><Icon svg=icons::USERS />" Etiqueta aportada por un usuario"</p>
                        {(product.ingredients.as_deref() != Some(label.as_str())).then(|| view! { <p class="ingredients">{label.clone()}</p> })}
                        <p class="muted small">"No está comprobada por SafeEat ni por el fabricante. Compárala con el envase."</p>
                    </div>
                })}
            {product.brand.as_deref().and_then(|b| crate::manufacturer::search_url(b, &product.name)).map(|(domain, url)| view! {
                <div class="manufacturer-link">
                    <a class="button ghost small" href=url target="_blank" rel="noopener noreferrer">
                        <Icon svg=icons::GLOBE />
                        {format!("Buscar en {domain}")}
                    </a>
                    <p class="muted small">"Abre un buscador externo limitado a la web oficial de la marca. SafeEat no comprueba lo que encuentres allí: lo que vale es el envase."</p>
                </div>
            })}
        </section>
    }
}

#[component]
fn PersonalVerdictCard(
    product: Product,
    state: RwSignal<Option<Result<Product, Failure>>>,
) -> impl IntoView {
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
                    let verdict = if v == VerdictValue::Safe {
                        "SAFE"
                    } else {
                        "UNSAFE"
                    };
                    api::send("PUT", &path, Some(json!({ "verdict": verdict }))).await
                }
                None => api::send("DELETE", &path, None).await,
            };
            busy.try_set(false);
            match result {
                Ok(p) => {
                    state.try_set(Some(Ok(p)));
                }
                Err(e) => {
                    error.try_set(Some(e.message));
                }
            }
        });
    };
    view! {
        <section class="card my-verdict" aria-label="Tu veredicto">
            {match (session.0.with(Option::is_some), mine) {
                (false, _) => view! {
                    <div class="my-verdict-row">
                        <p class="my-verdict-q">"¿Te sienta bien?"</p>
                        <button class="button ghost small" on:click=move |_| nav.push(Route::Login)>"Inicia sesión"</button>
                    </div>
                }.into_any(),
                (true, Some(v)) => {
                    let safe = v.verdict == VerdictValue::Safe;
                    view! {
                        {v.ingredients_changed.then(|| view! {
                            <div class="notice tone-caution">
                                <Icon svg=icons::ALERT />
                                <div class="notice-body"><p><strong>"Los ingredientes han cambiado, revísalo."</strong></p></div>
                            </div>
                        })}
                        {(v.evidence_changed && !v.ingredients_changed).then(|| view! {
                            <div class="notice tone-caution">
                                <Icon svg=icons::ALERT />
                                <div class="notice-body"><p><strong>"La información de alérgenos o trazas ha cambiado desde tu valoración: revísala."</strong></p></div>
                            </div>
                        })}
                        <div class="my-verdict-row">
                            <span class=if safe { "badge tone-safe" } else { "badge tone-danger" }>
                                <Icon svg=if safe { icons::THUMB_UP } else { icons::THUMB_DOWN } />
                                {if safe { "Te sienta bien" } else { "No es seguro para ti" }}
                            </span>
                            <button class="button ghost small" disabled=move || busy.get() on:click=move |_| set(None)>"Cambiar"</button>
                        </div>
                    }.into_any()
                }
                (true, None) => view! {
                    <p class="my-verdict-q">"¿Te sienta bien? " <span class="muted small">"Solo lo ves tú"</span></p>
                    <div class="two">
                        <button class="button safe small" disabled=move || busy.get() on:click=move |_| set(Some(VerdictValue::Safe))>
                            <Icon svg=icons::THUMB_UP />"Me sienta bien"
                        </button>
                        <button class="button danger small" disabled=move || busy.get() on:click=move |_| set(Some(VerdictValue::Unsafe))>
                            <Icon svg=icons::THUMB_DOWN />"No es seguro"
                        </button>
                    </div>
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

/// Heart toggle for a product (`kind = "products"`) or a restaurant (`"restaurants"`).
#[component]
pub fn FavoriteButton(kind: &'static str, id: String) -> impl IntoView {
    let session = use_session();
    let nav = use_nav();
    let saved = RwSignal::new(false);
    let path = StoredValue::new(format!("/favorites/{kind}/{}", api::enc(&id)));
    if session.0.with_untracked(Option::is_some) {
        spawn_local(async move {
            if let Ok(v) = api::get::<serde_json::Value>(&path.get_value()).await {
                saved.try_set(v["saved"].as_bool().unwrap_or(false));
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
            if api::send::<serde_json::Value>(method, &path.get_value(), None)
                .await
                .is_err()
            {
                saved.try_set(!next);
            }
        });
    };
    view! {
        <button
            class="icon-button fav"
            class:on=move || saved.get()
            aria-pressed=move || saved.get().to_string()
            aria-label=move || match (saved.get(), kind) {
                (true, _) => "Quitar de guardados",
                (false, "restaurants") => "Guardar restaurante",
                (false, _) => "Guardar producto",
            }
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

/// "Organizar en marcadores": tick the bookmarks this product belongs to.
#[component]
fn CollectionPicker(id: String) -> impl IntoView {
    let session = use_session();
    let collections = RwSignal::new(Vec::<safeeat_contracts::Collection>::new());
    let member = RwSignal::new(Vec::<String>::new());
    let busy = RwSignal::new(None::<String>);
    let error = RwSignal::new(None::<String>);
    let id = StoredValue::new(id);
    if session.0.with_untracked(Option::is_some) {
        spawn_local(async move {
            let list =
                api::get::<safeeat_contracts::Items<safeeat_contracts::Collection>>("/collections")
                    .await;
            let ids = api::get::<serde_json::Value>(&format!(
                "/collections/for-product/{}",
                api::enc(&id.get_value())
            ))
            .await;
            if let (Ok(list), Ok(ids)) = (list, ids) {
                let ids = ids["ids"]
                    .as_array()
                    .map(|a| {
                        a.iter()
                            .filter_map(|v| v.as_str().map(String::from))
                            .collect()
                    })
                    .unwrap_or_default();
                member.try_set(ids);
                collections.try_set(list.items);
            }
        });
    }
    let toggle = move |collection: String| {
        if busy.get_untracked().is_some() {
            return;
        }
        let inside = member.with_untracked(|m| m.contains(&collection));
        busy.set(Some(collection.clone()));
        error.set(None);
        spawn_local(async move {
            let path = format!(
                "/collections/{}/products/{}",
                api::enc(&collection),
                api::enc(&id.get_value())
            );
            let method = if inside { "DELETE" } else { "POST" };
            let result = api::send::<serde_json::Value>(method, &path, None).await;
            busy.try_set(None);
            match result {
                Ok(_) => {
                    member.try_update(|m| {
                        if inside {
                            m.retain(|c| c != &collection);
                        } else {
                            m.push(collection);
                        }
                    });
                }
                Err(e) => {
                    error.try_set(Some(e.message));
                }
            }
        });
    };
    view! {
        <Show when=move || collections.with(|c| !c.is_empty())>
            <details class="card disclosure">
                <summary><span class="icon" inner_html=icons::BOOKMARK></span>" Organizar en marcadores"</summary>
                <div class="stack-tight">
                    {move || collections.get().into_iter().map(|c| {
                        let cid = c.id.clone();
                        let checked = {
                            let cid = cid.clone();
                            move || member.with(|m| m.contains(&cid))
                        };
                        let pending = {
                            let cid = cid.clone();
                            move || busy.with(|b| b.as_deref() == Some(cid.as_str()))
                        };
                        view! {
                            <label class="check">
                                <input type="checkbox" prop:checked=checked disabled=move || busy.with(Option::is_some)
                                    on:change=move |_| toggle(cid.clone()) />
                                {c.name.clone()}
                                <Show when=pending><span class="spinner small" aria-hidden="true"></span></Show>
                            </label>
                        }
                    }).collect_view()}
                </div>
                {move || error.get().map(|m| view! { <ErrorBox message=m /> })}
            </details>
        </Show>
    }
}
