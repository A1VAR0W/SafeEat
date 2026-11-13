//! Community pieces shared by the product and restaurant screens: the list of reviews
//! (with "I have checked this too"), the product review form, star ratings and sharing.
//!
//! Wording rule: nothing here says "verified" or uses the green tone. A review confirmed
//! by other people or written by a trusted account is still somebody's report.

use leptos::prelude::*;
use leptos::task::spawn_local;
use safeeat_contracts::{CommunityReview, Items, Product, Restaurant, Role};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::api::{self, Failure};
use crate::icons;
use crate::nav::{use_nav, use_session, Route};
use crate::ui::{ErrorBox, Icon};

fn has_role(role: Role) -> impl Fn() -> bool + Copy {
    let session = use_session();
    move || {
        session
            .0
            .with(|u| u.as_ref().is_some_and(|u| u.roles.contains(&role)))
    }
}

fn review_label(status: &str) -> &'static str {
    match status {
        "GLUTEN_FREE" => "Dice que es sin gluten",
        "CONTAINS_GLUTEN" => "Dice que contiene gluten",
        "DEDICATED_GLUTEN_FREE" => "Dice que todo es sin gluten",
        "GLUTEN_FREE_OPTIONS" => "Dice que tiene opciones sin gluten",
        "NO_GLUTEN_FREE_OPTIONS" => "Dice que no tiene opciones sin gluten",
        _ => "Revisión",
    }
}

fn review_tone(status: &str) -> &'static str {
    match status {
        "CONTAINS_GLUTEN" | "NO_GLUTEN_FREE_OPTIONS" => "tone-danger",
        _ => "tone-info",
    }
}

fn confirmed_by(people: i64) -> String {
    if people == 1 {
        "Lo ha comprobado 1 persona".into()
    } else {
        format!("Lo han comprobado {people} personas")
    }
}

/// `2026-11-12T10:00:00Z` → `12/11/2026`.
pub(crate) fn short_date(iso: &str) -> String {
    let date = iso.get(..10).unwrap_or(iso);
    let mut parts = date.split('-');
    match (parts.next(), parts.next(), parts.next()) {
        (Some(y), Some(m), Some(d)) => format!("{d}/{m}/{y}"),
        _ => date.to_string(),
    }
}

/// Reviews of a product (`kind = "products"`) or restaurant (`"restaurants"`).
/// Saving a review re-renders the whole detail screen, which loads them again.
#[component]
pub fn CommunityReviews(kind: &'static str, id: String) -> impl IntoView {
    let state = RwSignal::new(None::<Result<Vec<CommunityReview>, Failure>>);
    let path = StoredValue::new(format!("/{kind}/{}/reviews", api::enc(&id)));
    let load = move || {
        spawn_local(async move {
            let Some(path) = path.try_get_value() else {
                return;
            };
            let result = api::get::<Items<CommunityReview>>(&path)
                .await
                .map(|i| i.items);
            state.try_set(Some(result));
        });
    };
    load();
    view! {
        <section class="card" aria-labelledby=format!("{kind}-reviews-title")>
            <h3 class="card-title" id=format!("{kind}-reviews-title")>
                <span class="icon" inner_html=icons::USERS></span>"Revisiones de la comunidad"
            </h3>
            {move || match state.get() {
                None => view! { <p class="muted">"Cargando revisiones…"</p> }.into_any(),
                Some(Err(e)) => view! { <ErrorBox message=e.message retry=Callback::new(move |_| load()) /> }.into_any(),
                Some(Ok(items)) if items.is_empty() => view! {
                    <p class="muted">"Nadie lo ha revisado todavía."</p>
                }.into_any(),
                Some(Ok(items)) => view! {
                    <ul class="reviews">
                        {items.into_iter().map(|r| view! {
                            <li><ReviewItem review=r path=path.get_value() reload=Callback::new(move |_| load()) /></li>
                        }).collect_view()}
                    </ul>
                }.into_any(),
            }}
            <p class="muted small">"Son aportaciones de otras personas. No certifican nada: revisa siempre la etiqueta o pregunta en el local."</p>
        </section>
    }
}

#[component]
fn ReviewItem(review: CommunityReview, path: String, reload: Callback<()>) -> impl IntoView {
    let session = use_session();
    let nav = use_nav();
    let admin = has_role(Role::Admin);
    let asking = RwSignal::new(false);
    let busy = RwSignal::new(false);
    let error = RwSignal::new(None::<String>);
    let by_trusted = review.verification_method.as_deref() == Some("authorized");
    let can_confirm = !review.own && !review.confirmed_by_me && !by_trusted;
    let confirm_path = StoredValue::new(format!("{path}/{}/confirm", api::enc(&review.id)));
    let revision = StoredValue::new(review.revision.clone());
    let confirm = move |_| {
        if busy.get_untracked() {
            return;
        }
        busy.set(true);
        error.set(None);
        spawn_local(async move {
            let body = json!({ "revision": revision.get_value() });
            let result = api::send::<Value>("POST", &confirm_path.get_value(), Some(body)).await;
            busy.try_set(false);
            match result {
                Ok(_) => reload.run(()),
                Err(e) => {
                    asking.try_set(false);
                    error.try_set(Some(e.message));
                    // The review may have changed meanwhile (new revision): show the current one.
                    reload.run(());
                }
            }
        });
    };
    let trust = match (&review.verified_at, by_trusted) {
        (Some(at), true) => Some(format!(
            "Revisión de una cuenta de confianza · {}",
            short_date(at)
        )),
        (Some(at), false) => Some(format!(
            "{} · {}",
            confirmed_by(review.confirmations),
            short_date(at)
        )),
        (None, _) => None,
    };
    view! {
        <article class=format!("review {}", review_tone(&review.status))>
            <p class="review-title"><strong>{review_label(&review.status)}</strong></p>
            <p class:muted=review.note.is_empty()>
                {if review.note.is_empty() { "Sin motivo añadido".to_string() } else { review.note.clone() }}
            </p>
            <p class="muted small">
                {if review.own { "Tu aportación" } else { "Aportación de la comunidad" }}
                " · " {short_date(&review.updated_at)}
                {(review.scope.as_deref() == Some("chain")).then_some(" · para toda la cadena")}
            </p>
            {review.inherited.then(|| view! {
                <p class="muted small">"Viene de otro local de la misma cadena. Pregunta cómo lo preparan en este."</p>
            })}
            {match trust {
                Some(text) => view! { <p class="small trust"><Icon svg=icons::USERS />{text}</p> }.into_any(),
                None => view! { <p class="muted small">"Nadie más lo ha comprobado todavía."</p> }.into_any(),
            }}
            {review.confirmed_by_me.then(|| view! { <p class="small field-ok">"Tú también lo has comprobado."</p> })}
            {(can_confirm).then(|| view! {
                <Show when=move || !admin()>
                    <Show
                        when=move || asking.get()
                        fallback=move || view! {
                            <button class="button ghost small" disabled=move || busy.get() on:click=move |_| {
                                if session.0.with_untracked(Option::is_none) {
                                    nav.push(Route::Login);
                                } else {
                                    asking.set(true);
                                }
                            }>"Yo también lo he comprobado"</button>
                        }
                    >
                        <div class="confirm-box" role="group" aria-label="Confirmar la revisión">
                            <p class="small">"Confírmalo solo si lo has comprobado tú en persona: la etiqueta actual o lo que te dijeron en el local."</p>
                            <div class="two">
                                <button class="button ghost small" disabled=move || busy.get() on:click=move |_| asking.set(false)>"Cancelar"</button>
                                <button class="button small" disabled=move || busy.get() on:click=confirm>
                                    {move || if busy.get() { "Guardando…" } else { "Lo he comprobado" }}
                                </button>
                            </div>
                        </div>
                    </Show>
                </Show>
            })}
            {move || error.get().map(|m| view! { <ErrorBox message=m /> })}
        </article>
    }
}

/// "What does the label say?" for products with a barcode. `saved` lives in the screen
/// because saving re-renders the detail.
#[component]
pub fn ProductReviewForm(
    product: Product,
    state: RwSignal<Option<Result<Product, Failure>>>,
    saved: RwSignal<bool>,
) -> impl IntoView {
    let session = use_session();
    let nav = use_nav();
    let authorized = has_role(Role::AuthorizedCeliac);
    let choice = RwSignal::new(None::<&'static str>);
    let note = RwSignal::new(String::new());
    let busy = RwSignal::new(false);
    let error = RwSignal::new(None::<String>);
    let note_short = move || !authorized() && note.with(|n| n.trim().chars().count() < 10);
    let id = StoredValue::new(product.id.clone());
    let submit = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        let Some(status) = choice.get_untracked() else {
            return;
        };
        if busy.get_untracked() || note_short() {
            return;
        }
        busy.set(true);
        saved.set(false);
        error.set(None);
        spawn_local(async move {
            let body = json!({ "status": status, "note": note.get_untracked().trim() });
            let path = format!("/products/{}/gluten-report", api::enc(&id.get_value()));
            let result = api::send::<Product>("POST", &path, Some(body)).await;
            busy.try_set(false);
            match result {
                Ok(p) => {
                    saved.try_set(true);
                    state.try_set(Some(Ok(p)));
                }
                Err(e) => {
                    error.try_set(Some(e.message));
                }
            }
        });
    };
    let option = move |value: &'static str, label: &'static str| {
        view! {
            <button type="button" role="radio" class="choice"
                aria-checked=move || (choice.get() == Some(value)).to_string()
                class:active=move || choice.get() == Some(value)
                on:click=move |_| choice.set(Some(value))>
                {label}
            </button>
        }
    };
    view! {
        <section class="card">
            <h3 class="card-title">"¿Qué dice la etiqueta?"</h3>
            {move || if session.0.with(Option::is_none) {
                view! {
                    <p class="muted">"Inicia sesión para contar qué indica la etiqueta de tu envase."</p>
                    <button class="button ghost" on:click=move |_| nav.push(Route::Login)>"Iniciar sesión"</button>
                }.into_any()
            } else {
                view! {
                    <form class="form-row" on:submit=submit>
                        <div class="choices" role="radiogroup" aria-label="Lo que indica la etiqueta">
                            {option("GLUTEN_FREE", "La etiqueta indica «sin gluten»")}
                            {option("CONTAINS_GLUTEN", "La etiqueta indica que contiene gluten")}
                        </div>
                        <label class="label" for="product-review-note">
                            {move || if authorized() { "Motivo (opcional)" } else { "¿Qué has comprobado?" }}
                        </label>
                        <textarea id="product-review-note" class="input textarea" rows="3" maxlength="500"
                            placeholder="Por ejemplo: la frase de la etiqueta y la fecha del envase"
                            prop:value=move || note.get()
                            on:input=move |ev| note.set(event_target_value(&ev))
                            aria-describedby="product-review-hint"></textarea>
                        <p id="product-review-hint" class="muted small">
                            {move || if note_short() { "Escribe al menos 10 caracteres y elige una opción." } else { "Otras personas verán tu revisión como aportación de usuario." }}
                        </p>
                        <button class="button" type="submit"
                            disabled=move || busy.get() || choice.get().is_none() || note_short()>
                            {move || if busy.get() { "Enviando…" } else { "Enviar revisión" }}
                        </button>
                    </form>
                }.into_any()
            }}
            <Show when=move || saved.get()>
                <p class="field-ok" role="status">"Gracias, tu revisión se ha guardado."</p>
            </Show>
            {move || error.get().map(|m| view! { <ErrorBox message=m /> })}
            <p class="muted small">"Una revisión de usuario no sustituye a la etiqueta ni descarta la contaminación cruzada."</p>
        </section>
    }
}

#[derive(Deserialize)]
struct MyRating {
    stars: Option<u8>,
}

/// Star rating of a restaurant (the experience, not its gluten-free options).
#[component]
pub fn RatingCard(
    item: Restaurant,
    state: RwSignal<Option<Result<Restaurant, Failure>>>,
) -> impl IntoView {
    let session = use_session();
    let nav = use_nav();
    let mine = RwSignal::new(None::<u8>);
    let chosen = RwSignal::new(None::<u8>);
    let busy = RwSignal::new(false);
    let error = RwSignal::new(None::<String>);
    let path = StoredValue::new(format!("/restaurants/{}/rating", api::enc(&item.id)));
    if session.0.with_untracked(Option::is_some) {
        spawn_local(async move {
            match api::get::<MyRating>(&path.get_value()).await {
                Ok(r) => {
                    mine.try_set(r.stars);
                }
                Err(e) => {
                    error.try_set(Some(e.message));
                }
            }
        });
    }
    let save = move |stars: Option<u8>| {
        if busy.get_untracked() {
            return;
        }
        busy.set(true);
        error.set(None);
        spawn_local(async move {
            let result = match stars {
                Some(s) => {
                    api::send::<Restaurant>("PUT", &path.get_value(), Some(json!({ "stars": s })))
                        .await
                }
                None => api::send::<Restaurant>("DELETE", &path.get_value(), None).await,
            };
            busy.try_set(false);
            match result {
                Ok(r) => {
                    mine.try_set(stars);
                    chosen.try_set(None);
                    state.try_set(Some(Ok(r)));
                }
                Err(e) => {
                    error.try_set(Some(e.message));
                }
            }
        });
    };
    let shown = move || chosen.get().or(mine.get()).unwrap_or(0);
    let summary = item.rating.map(|r| {
        format!(
            "{:.1} de 5 · {} {}",
            r.average,
            r.count,
            if r.count == 1 {
                "valoración"
            } else {
                "valoraciones"
            }
        )
    });
    view! {
        <section class="card">
            <h3 class="card-title">"Valoraciones"</h3>
            {match summary {
                Some(text) => view! { <p><span class="star-inline" aria-hidden="true">"★"</span>" " {text}</p> }.into_any(),
                None => view! { <p class="muted">"Nadie lo ha valorado todavía."</p> }.into_any(),
            }}
            <p class="muted small">"Las estrellas puntúan la experiencia. No dicen nada de sus opciones sin gluten."</p>
            {move || if session.0.with(Option::is_none) {
                view! { <button class="button ghost" on:click=move |_| nav.push(Route::Login)>"Inicia sesión para valorar"</button> }.into_any()
            } else {
                view! {
                    <p class="small">{move || if mine.get().is_some() { "Tu valoración" } else { "¿Qué te ha parecido?" }}</p>
                    <div class="stars" role="radiogroup" aria-label="Tu valoración">
                        {(1u8..=5).map(|value| view! {
                            <button type="button" role="radio" class="star"
                                class:on=move || value <= shown()
                                aria-checked=move || (shown() == value).to_string()
                                aria-label=format!("{value} {}", if value == 1 { "estrella" } else { "estrellas" })
                                disabled=move || busy.get()
                                on:click=move |_| chosen.set(Some(value))>"★"</button>
                        }).collect_view()}
                    </div>
                    <div class="two">
                        <button class="button" disabled=move || busy.get() || chosen.get().is_none()
                            on:click=move |_| save(chosen.get_untracked())>
                            {move || if busy.get() { "Guardando…" } else { "Guardar" }}
                        </button>
                        <Show when=move || mine.get().is_some()>
                            <button class="button ghost" disabled=move || busy.get() on:click=move |_| save(None)>"Quitar"</button>
                        </Show>
                    </div>
                }.into_any()
            }}
            {move || error.get().map(|m| view! { <ErrorBox message=m /> })}
        </section>
    }
}

#[derive(Serialize)]
struct ShareArgs {
    kind: &'static str,
    id: String,
}

#[derive(Deserialize)]
struct Shared {
    url: String,
    shared: bool,
}

/// Opens the system share sheet with a public link. Where there is no sheet (desktop
/// development builds), shows the link so it can be copied.
#[component]
pub fn ShareButton(kind: &'static str, id: String) -> impl IntoView {
    let busy = RwSignal::new(false);
    let link = RwSignal::new(None::<String>);
    let error = RwSignal::new(None::<String>);
    let id = StoredValue::new(id);
    let share = move |_| {
        if busy.get_untracked() {
            return;
        }
        busy.set(true);
        error.set(None);
        spawn_local(async move {
            let result: Result<Shared, Failure> = api::invoke(
                "share_link",
                ShareArgs {
                    kind,
                    id: id.get_value(),
                },
            )
            .await;
            busy.try_set(false);
            match result {
                Ok(s) if s.shared => {}
                Ok(s) => {
                    link.try_set(Some(s.url));
                }
                Err(e) => {
                    error.try_set(Some(e.message));
                }
            }
        });
    };
    view! {
        <div class="share-row">
            <button class="button ghost" disabled=move || busy.get() on:click=share>
                <Icon svg=icons::SHARE />
                {move || if busy.get() { "Preparando enlace…" } else { "Compartir" }}
            </button>
            {move || link.get().map(|url| view! {
                <div class="share-fallback" role="status">
                    <label class="label" for=format!("share-link-{kind}")>"Enlace para compartir"</label>
                    <input id=format!("share-link-{kind}") class="input" readonly value=url />
                </div>
            })}
            {move || error.get().map(|m| view! { <ErrorBox message=m /> })}
        </div>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn community_reviews_are_never_green() {
        for status in [
            "GLUTEN_FREE",
            "CONTAINS_GLUTEN",
            "DEDICATED_GLUTEN_FREE",
            "GLUTEN_FREE_OPTIONS",
            "NO_GLUTEN_FREE_OPTIONS",
            "SOMETHING_NEW",
        ] {
            assert_ne!(review_tone(status), "tone-safe", "{status}");
            assert!(!review_label(status).to_lowercase().contains("verific"));
        }
        assert_eq!(review_tone("CONTAINS_GLUTEN"), "tone-danger");
    }

    #[test]
    fn dates_are_shown_day_first() {
        assert_eq!(short_date("2026-11-12T10:00:00.000Z"), "12/11/2026");
        assert_eq!(short_date("raro"), "raro");
        assert_eq!(confirmed_by(1), "Lo ha comprobado 1 persona");
        assert_eq!(confirmed_by(3), "Lo han comprobado 3 personas");
    }
}
