use leptos::prelude::*;
use leptos::task::spawn_local;
use safeeat_contracts::{ChainDetail, GlutenSupport, Restaurant, SupportSource};
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

/// Spanish name of an OpenStreetMap venue kind.
pub fn category_label(category: &str) -> Option<&'static str> {
    Some(match category {
        "restaurant" => "Restaurante",
        "cafe" => "Cafetería",
        "fast_food" => "Comida rápida",
        "bar" => "Bar",
        "pub" => "Pub",
        "food_court" => "Zona de restauración",
        "biergarten" => "Cervecería",
        "bakery" => "Panadería",
        "pastry" => "Pastelería",
        "convenience" => "Tienda",
        _ => return None,
    })
}

/// Spanish name of the most common OpenStreetMap cuisines; others as tagged.
pub fn cuisine_label(cuisine: &str) -> String {
    match cuisine {
        "pizza" => "Pizzería".into(),
        "italian" => "Italiana".into(),
        "spanish" => "Española".into(),
        "regional" => "Regional".into(),
        "burger" => "Hamburguesas".into(),
        "sushi" | "japanese" => "Japonesa".into(),
        "chinese" => "China".into(),
        "mexican" => "Mexicana".into(),
        "indian" => "India".into(),
        "kebab" => "Kebab".into(),
        "tapas" => "Tapas".into(),
        "mediterranean" => "Mediterránea".into(),
        "asian" => "Asiática".into(),
        "american" => "Americana".into(),
        "coffee_shop" => "Café".into(),
        "sandwich" => "Bocadillos".into(),
        "chicken" => "Pollo".into(),
        "seafood" => "Marisco".into(),
        "vegetarian" => "Vegetariana".into(),
        "vegan" => "Vegana".into(),
        "ice_cream" => "Heladería".into(),
        other => {
            let text = other.replace('_', " ");
            let mut chars = text.chars();
            chars
                .next()
                .map(|c| c.to_uppercase().chain(chars).collect())
                .unwrap_or_default()
        }
    }
}

/// "Restaurante · Pizzería": the kind of place, as far as the map knows it.
pub fn place_kind(r: &Restaurant) -> String {
    [
        r.category
            .as_deref()
            .and_then(category_label)
            .map(String::from),
        r.cuisine.as_deref().map(cuisine_label),
    ]
    .into_iter()
    .flatten()
    .collect::<Vec<_>>()
    .join(" · ")
}

/// What other users said, in one line ("2 dicen que tiene opciones · 1 que no").
pub fn reports_line(r: &Restaurant) -> Option<String> {
    let c = r.gluten_reports?;
    let mut parts = Vec::new();
    if c.dedicated > 0 {
        parts.push(format!("{} que todo es sin gluten", c.dedicated));
    }
    if c.options > 0 {
        parts.push(format!("{} que tiene opciones", c.options));
    }
    if c.no_options > 0 {
        parts.push(format!("{} que no tiene opciones", c.no_options));
    }
    let total = c.dedicated + c.options + c.no_options;
    (total > 0).then(|| {
        format!(
            "{total} {}: {}",
            if total == 1 {
                "revisión de usuarios"
            } else {
                "revisiones de usuarios"
            },
            parts.join(" · ")
        )
    })
}

/// Website without the scheme, for the button's caption.
fn host_of(url: &str) -> String {
    url.split("://")
        .nth(1)
        .unwrap_or(url)
        .split('/')
        .next()
        .unwrap_or_default()
        .trim_start_matches("www.")
        .to_string()
}

#[component]
fn PlaceHeader(item: Restaurant) -> impl IntoView {
    // A photo that does not load leaves the icon, never a broken image.
    let broken = RwSignal::new(false);
    let photo = item.photo.clone();
    let kind = place_kind(&item);
    let address = item.address.trim().to_string();
    let rating = item.rating.clone();
    let distance = item.distance_m.map(crate::screens::restaurants::distance);
    let meta = [(!kind.is_empty()).then_some(kind), distance]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join(" · ");
    let has_photo = photo.is_some();
    view! {
        <section class="place card" class:with-photo=move || has_photo && !broken.get()>
            {photo.map(|p| view! {
                <Show when=move || !broken.get()>
                    <figure class="place-photo">
                        <img src=p.url.clone() alt="" loading="lazy" decoding="async" on:error=move |_| broken.set(true) />
                        <figcaption>
                            <a href=p.page_url.clone() target="_blank" rel="noopener">{format!("Foto: {}", p.credit)}</a>
                        </figcaption>
                    </figure>
                </Show>
            })}
            <div class="place-head">
                <Show when=move || !has_photo || broken.get()>
                    <span class="place-icon" inner_html=icons::UTENSILS aria-hidden="true"></span>
                </Show>
                <div class="place-text">
                    <h2 class="place-name">{item.name.clone()}</h2>
                    {(!meta.is_empty()).then(|| view! { <p class="muted small">{meta.clone()}</p> })}
                    {(!address.is_empty()).then(|| view! { <p class="small place-address">{address.clone()}</p> })}
                    {rating.map(|r| view! {
                        <p class="small">{format!("{:.1} ★ · {} {}", r.average, r.count, if r.count == 1 { "valoración" } else { "valoraciones" })}</p>
                    })}
                </div>
                <FavoriteButton kind="restaurants" id=item.id.clone() />
            </div>
        </section>
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
    let website = item.website.clone();
    let phone = item.phone.clone();
    let trusted = item.support_source == Some(SupportSource::TrustedCommunity);
    let reports = reports_line(&item);
    view! {
        <article class="stack">
            <PlaceHeader item=item.clone() />
            <section class=format!("verdict compact {}", support_tone(support))>
                <span class="verdict-icon" inner_html=if support.is_positive() { icons::CHECK } else if support == GlutenSupport::NoGlutenFreeOptions { icons::X_CIRCLE } else { icons::HELP }></span>
                <div>
                    <p class="verdict-text">{support.label()}</p>
                    {item.support_source.map(|s| view! { <p class="verdict-source">{s.explanation()}</p> })}
                    {trusted.then(|| view! {
                        <p class="verdict-note"><Icon svg=icons::SHIELD />"Incluye la revisión de una cuenta de confianza"</p>
                    })}
                    {reports.map(|line| view! { <p class="verdict-note">{line}</p> })}
                </div>
            </section>
            <nav class="actions" aria-label="Acciones">
                <a class="action" href=item.maps_url.clone() target="_blank" rel="noopener">
                    <Icon svg=icons::NAVIGATION />"Cómo llegar"
                </a>
                {phone.map(|p| view! {
                    <a class="action" href=format!("tel:{}", p.replace(' ', ""))>
                        <Icon svg=icons::PHONE />"Llamar"
                    </a>
                })}
                {website.map(|w| {
                    let host = host_of(&w);
                    view! {
                        <a class="action" href=w target="_blank" rel="noopener" aria-label=format!("Web: {host}")>
                            <Icon svg=icons::GLOBE />"Web"
                        </a>
                    }
                })}
            </nav>
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
        <details class="card fold" prop:open=move || saved.get() || error.with(Option::is_some)>
            <summary class="card-title">"¿Has comido aquí? Cuenta qué comprobaste"</summary>
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
        </details>
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

#[cfg(test)]
mod tests {
    use super::*;

    fn place(extra: serde_json::Value) -> Restaurant {
        let mut base = serde_json::json!({
            "id": "osm_n_1", "name": "Beverly-Beer", "address": "", "mapsUrl": "https://m",
            "glutenSupport": "UNKNOWN", "evidence": [], "source": "s", "attributions": [], "fetchedAt": "t"
        });
        base.as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        serde_json::from_value(base).unwrap()
    }

    #[test]
    fn the_kind_line_uses_what_the_map_knows() {
        assert_eq!(place_kind(&place(serde_json::json!({}))), "");
        assert_eq!(
            place_kind(&place(serde_json::json!({"category": "bar"}))),
            "Bar"
        );
        assert_eq!(
            place_kind(&place(
                serde_json::json!({"category": "restaurant", "cuisine": "pizza"})
            )),
            "Restaurante · Pizzería"
        );
        assert_eq!(
            place_kind(&place(
                serde_json::json!({"category": "nightclub", "cuisine": "fusion_food"})
            )),
            "Fusion food"
        );
    }

    #[test]
    fn user_reports_are_summed_up_in_one_line() {
        assert_eq!(reports_line(&place(serde_json::json!({}))), None);
        let none = place(
            serde_json::json!({"glutenReports": {"dedicated": 0, "options": 0, "noOptions": 0}}),
        );
        assert_eq!(reports_line(&none), None);
        let mixed = place(
            serde_json::json!({"glutenReports": {"dedicated": 0, "options": 2, "noOptions": 1}}),
        );
        assert_eq!(
            reports_line(&mixed).unwrap(),
            "3 revisiones de usuarios: 2 que tiene opciones · 1 que no tiene opciones"
        );
    }

    #[test]
    fn the_web_button_names_the_site() {
        assert_eq!(host_of("https://www.example.com/carta"), "example.com");
        assert_eq!(host_of("http://bar.es"), "bar.es");
    }
}
