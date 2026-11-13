use leptos::prelude::*;
use leptos::task::spawn_local;
use safeeat_contracts::{ChainDetail, GlutenSupport, Restaurant};
use serde_json::json;

use crate::api::{self, Failure};
use crate::icons;
use crate::nav::MapCenter;
use crate::nav::{use_nav, use_session, Route};
use crate::screens::community::{CommunityReviews, RatingCard, ShareButton};
use crate::screens::product::FavoriteButton;
use crate::screens::restaurants::{support_tone, RestaurantRow};
use crate::ui::{ErrorBox, Header, Icon, Loading};

#[component]
pub fn RestaurantScreen(id: String) -> impl IntoView {
    let state = RwSignal::new(None::<Result<Restaurant, Failure>>);
    // Lives here because saving a review re-renders the whole detail.
    let saved = RwSignal::new(false);
    let path = StoredValue::new(format!("/restaurants/{}", api::enc(&id)));
    let load = move || {
        spawn_local(async move {
            let result = api::get(&path.get_value()).await;
            state.try_set(Some(result));
        })
    };
    load();
    let title = Signal::derive(move || {
        state.with(|s| match s {
            Some(Ok(r)) => r.name.clone(),
            _ => "Restaurante".into(),
        })
    });
    view! {
        <Header title=title />
        {move || match state.get() {
            None => view! { <Loading /> }.into_any(),
            Some(Err(e)) => view! { <div class="stack"><ErrorBox message=e.message retry=Callback::new(move |_| load()) /></div> }.into_any(),
            Some(Ok(r)) => view! { <RestaurantDetail item=r state=state saved=saved /> }.into_any(),
        }}
    }
}

#[component]
fn RestaurantDetail(
    item: Restaurant,
    state: RwSignal<Option<Result<Restaurant, Failure>>>,
    saved: RwSignal<bool>,
) -> impl IntoView {
    let nav = use_nav();
    let support = item.gluten_support;
    let chain = item.chain.clone();
    view! {
        <article class="stack">
            <section class=format!("verdict {}", support_tone(support))>
                <span class="verdict-icon" inner_html=if support.is_positive() { icons::CHECK } else if support == GlutenSupport::NoGlutenFreeOptions { icons::X_CIRCLE } else { icons::HELP }></span>
                <div>
                    <p class="verdict-text">{support.label()}</p>
                    {item.support_source.map(|s| view! { <p class="verdict-source">{s.explanation()}</p> })}
                </div>
            </section>
            <section class="card">
                <div class="card-head">
                    <p>{item.address.clone()}</p>
                    <FavoriteButton kind="restaurants" id=item.id.clone() />
                </div>
                {item.rating.clone().map(|r| view! { <p class="muted">{format!("{:.1} ★ · {} {}", r.average, r.count, if r.count == 1 { "valoración" } else { "valoraciones" })}</p> })}
                <div class="two">
                    <a class="button ghost" href=item.maps_url.clone() target="_blank" rel="noopener">"Cómo llegar"</a>
                    {item.phone.clone().map(|p| view! { <a class="button ghost" href=format!("tel:{p}")>"Llamar"</a> })}
                </div>
            </section>
            {chain.clone().map(|c| {
                let key = c.key.clone();
                view! {
                    <button class="row card" on:click=move |_| nav.push(Route::Chain(key.clone()))>
                        <span class="thumb icon-thumb" inner_html=icons::STORE></span>
                        <span class="row-body">
                            <span class="row-title">"Cadena " {c.name.clone()}</span>
                            <span class="muted small">
                                {if item.has_local_review == Some(true) { "Este local tiene su propia revisión, que manda sobre la de la cadena." } else { "Ver otros locales y el veredicto de la cadena" }}
                            </span>
                        </span>
                        <Icon svg=icons::CHEVRON class="chevron" />
                    </button>
                }
            })}
            {(!item.evidence.is_empty()).then(|| view! {
                <section class="card">
                    <h3 class="card-title">"¿En qué se basa?"</h3>
                    <ul class="reasons">{item.evidence.iter().map(|e| view! { <li>{e.text.clone()}</li> }).collect_view()}</ul>
                </section>
            })}
            <ReviewForm item=item.clone() state=state saved=saved />
            <CommunityReviews kind="restaurants" id=item.id.clone() />
            <RatingCard item=item.clone() state=state />
            <ShareButton kind="restaurants" id=item.id.clone() />
            <p class="muted small source">"Esta información la aportan otros usuarios y OpenStreetMap; SafeEat no la verifica ni garantiza cómo se prepara la comida. Pregunta siempre en el local por la contaminación cruzada."</p>
        </article>
    }
}

#[component]
fn ReviewForm(
    item: Restaurant,
    state: RwSignal<Option<Result<Restaurant, Failure>>>,
    saved: RwSignal<bool>,
) -> impl IntoView {
    let session = use_session();
    let nav = use_nav();
    let trusted = session
        .0
        .with(|u| u.as_ref().is_some_and(|u| u.is_trusted()));
    // The server asks everyone except authorised coeliacs to say what they checked.
    let needs_note = !session.0.with(|u| {
        u.as_ref().is_some_and(|u| {
            u.roles
                .iter()
                .any(|r| matches!(r, safeeat_contracts::Role::AuthorizedCeliac))
        })
    });
    let note = RwSignal::new(String::new());
    let note_short = move || needs_note && note.with(|n| n.trim().chars().count() < 10);
    let in_chain = item.chain.is_some();
    let scope = RwSignal::new("local");
    let busy = RwSignal::new(false);
    let error = RwSignal::new(None::<String>);
    let id = StoredValue::new(item.id.clone());
    let send = move |support: GlutenSupport| {
        if busy.get_untracked() || note_short() {
            return;
        }
        busy.set(true);
        error.set(None);
        saved.set(false);
        spawn_local(async move {
            let body = json!({
                "support": support.as_str(),
                "scope": scope.get_untracked(),
                "note": note.get_untracked().trim(),
            });
            let path = format!("/restaurants/{}/gluten-report", api::enc(&id.get_value()));
            let result = api::send::<Restaurant>("POST", &path, Some(body)).await;
            busy.try_set(false);
            match result {
                Ok(r) => {
                    saved.try_set(true);
                    state.try_set(Some(Ok(r)));
                }
                Err(e) => {
                    error.try_set(Some(e.message));
                }
            }
        });
    };
    view! {
        <section class="card">
            <h3 class="card-title">"¿Has comido aquí?"</h3>
            {if session.0.with(Option::is_none) {
                view! {
                    <p class="muted">"Inicia sesión para contar tu experiencia."</p>
                    <button class="button ghost" on:click=move |_| nav.push(Route::Login)>"Iniciar sesión"</button>
                }.into_any()
            } else {
                view! {
                    {(in_chain && trusted).then(|| view! {
                        <div class="segmented" role="radiogroup" aria-label="Aplicar a">
                            <button role="radio" aria-checked=move || (scope.get() == "local").to_string()
                                class:active=move || scope.get() == "local" on:click=move |_| scope.set("local")>"Solo este restaurante"</button>
                            <button role="radio" aria-checked=move || (scope.get() == "chain").to_string()
                                class:active=move || scope.get() == "chain" on:click=move |_| scope.set("chain")>"Toda la cadena"</button>
                        </div>
                    })}
                    <label class="label" for="review-note">"¿Qué has comprobado?"</label>
                    <textarea
                        id="review-note"
                        class="input textarea"
                        rows="3"
                        maxlength="500"
                        placeholder="Ej.: carta sin gluten, cocinan aparte, preguntamos al camarero…"
                        prop:value=move || note.get()
                        on:input=move |ev| note.set(event_target_value(&ev))
                        aria-describedby="review-note-hint"
                    ></textarea>
                    <p id="review-note-hint" class="muted small">
                        {move || if note_short() { "Escribe al menos 10 caracteres y elige una opción." } else { "Elige una opción para guardar tu revisión." }}
                    </p>
                    <div class="stack-tight">
                        <button class="button safe" disabled=move || busy.get() || note_short() on:click=move |_| send(GlutenSupport::DedicatedGlutenFree)>"Todo es sin gluten"</button>
                        <button class="button safe-soft" disabled=move || busy.get() || note_short() on:click=move |_| send(GlutenSupport::GlutenFreeOptions)>"Tiene opciones sin gluten"</button>
                        <button class="button danger" disabled=move || busy.get() || note_short() on:click=move |_| send(GlutenSupport::NoGlutenFreeOptions)>"No tiene opciones sin gluten"</button>
                    </div>
                }.into_any()
            }}
            <Show when=move || saved.get()>
                <p class="field-ok" role="status">"Gracias, tu revisión se ha guardado."</p>
            </Show>
            {move || error.get().map(|text| view! { <ErrorBox message=text /> })}
        </section>
    }
}

#[component]
pub fn ChainScreen(key: String) -> impl IntoView {
    let state = RwSignal::new(None::<Result<ChainDetail, Failure>>);
    let key = StoredValue::new(key);
    // Read outside the task: async tasks run without the reactive owner.
    let center = expect_context::<MapCenter>().0;
    let load = move || {
        spawn_local(async move {
            // Nearest branches first: the server sorts by distance to the map centre.
            let near = center
                .get_value()
                .map(|(lat, lng)| format!("?latitude={lat:.5}&longitude={lng:.5}"))
                .unwrap_or_default();
            let path = format!("/restaurants/chains/{}{near}", api::enc(&key.get_value()));
            let result = api::get(&path).await;
            state.try_set(Some(result));
        })
    };
    load();
    let title = Signal::derive(move || {
        state.with(|s| match s {
            Some(Ok(c)) => format!("Cadena {}", c.chain.name),
            _ => "Cadena".into(),
        })
    });
    view! {
        <Header title=title />
        {move || match state.get() {
            None => view! { <Loading /> }.into_any(),
            Some(Err(e)) => view! { <div class="stack"><ErrorBox message=e.message retry=Callback::new(move |_| load()) /></div> }.into_any(),
            Some(Ok(c)) => {
                let support = c.chain.gluten_support;
                let exceptions = c.branches.iter().filter(|b| b.has_local_review == Some(true)).count();
                view! {
                    <article class="stack">
                        <section class=format!("verdict {}", support_tone(support))>
                            <span class="verdict-icon" inner_html=if support.is_positive() { icons::CHECK } else { icons::HELP }></span>
                            <p class="verdict-text">{support.label()}</p>
                        </section>
                        <p class="muted">{format!("{} locales en total", c.chain.branch_count)}
                            {(exceptions > 0).then(|| format!(" · {exceptions} con revisión propia"))}</p>
                        {c.can_mark_chain.then(|| view! {
                            <p class="muted small">"Tu cuenta es de confianza: puedes marcar toda la cadena desde la ficha de cualquier local."</p>
                        })}
                        <ul class="list">
                            {c.branches.into_iter().map(|r| view! { <li><RestaurantRow item=r /></li> }).collect_view()}
                        </ul>
                    </article>
                }.into_any()
            }
        }}
    }
}
