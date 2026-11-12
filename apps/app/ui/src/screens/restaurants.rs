//! Map + search. Mounted once and kept alive (hidden) so returning to the tab is instant.

use leptos::prelude::*;
use leptos::task::spawn_local;
use safeeat_contracts::{GlutenSupport, Items, Restaurant, RestaurantSuggestion, SuggestionKind};
use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;

use crate::api::{self, Failure};
use crate::icons;
use crate::nav::{use_nav, MapCenter, Route};
use crate::ui::{Empty, ErrorBox, Icon, Loading};

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = seMap, js_name = mount)]
    fn map_mount(
        el: &web_sys::HtmlElement,
        lat: f64,
        lng: f64,
        zoom: f64,
        on_move: &JsValue,
        on_select: &JsValue,
        on_error: &JsValue,
    );
    #[wasm_bindgen(js_namespace = seMap, js_name = setPlaces)]
    fn map_set_places(json: &str);
    #[wasm_bindgen(js_namespace = seMap, js_name = flyTo)]
    fn map_fly_to(lat: f64, lng: f64, zoom: f64);
}

/// Puerta del Sol, only until the real location (or the user's pan) arrives.
const FALLBACK: (f64, f64) = (40.4168, -3.7038);

#[derive(Clone, Copy, PartialEq, Eq)]
enum Filter {
    All,
    Options,
    Dedicated,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PositionArgs {
    options: PositionOptions,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PositionOptions {
    enable_high_accuracy: bool,
    timeout: u32,
    maximum_age: u32,
}
#[derive(Deserialize)]
struct Position {
    coords: Coords,
}
#[derive(Deserialize)]
struct Coords {
    latitude: f64,
    longitude: f64,
}

async fn locate() -> Option<(f64, f64)> {
    #[derive(Serialize)]
    struct PermissionArgs {
        permissions: [&'static str; 1],
    }
    let status: Result<serde_json::Value, _> = api::invoke(
        "plugin:geolocation|request_permissions",
        PermissionArgs {
            permissions: ["location"],
        },
    )
    .await;
    match &status {
        Ok(v) if v["location"] == "denied" => return None,
        Err(e) => web_sys::console::warn_1(&format!("location permission: {}", e.message).into()),
        _ => {}
    }
    let position: Position = api::invoke(
        "plugin:geolocation|get_current_position",
        PositionArgs {
            options: PositionOptions {
                enable_high_accuracy: false,
                timeout: 10_000,
                maximum_age: 120_000,
            },
        },
    )
    .await
    .map_err(|e| web_sys::console::warn_1(&format!("location: {}", e.message).into()))
    .ok()?;
    Some((position.coords.latitude, position.coords.longitude))
}

fn places_json(items: &[Restaurant], highlight: Option<&str>) -> String {
    let features: Vec<serde_json::Value> = items
        .iter()
        .filter_map(|r| {
            let (lat, lng) = (r.latitude?, r.longitude?);
            let in_chain = highlight.is_some() && r.chain.as_ref().map(|c| c.key.as_str()) == highlight;
            Some(serde_json::json!({
                "type": "Feature",
                "geometry": { "type": "Point", "coordinates": [lng, lat] },
                "properties": { "id": r.id, "support": r.gluten_support.as_str(), "highlight": in_chain }
            }))
        })
        .collect();
    serde_json::json!({ "type": "FeatureCollection", "features": features }).to_string()
}

pub fn distance(m: f64) -> String {
    if m < 1000.0 {
        format!("{:.0} m", (m / 10.0).round() * 10.0)
    } else {
        format!("{:.1} km", m / 1000.0).replace('.', ",")
    }
}

/// Favourable restaurant information always comes from other users or OpenStreetMap
/// tags, never from a verified source, so it is shown as orientative (blue), not as a
/// green safety badge. Warnings keep their colour.
pub fn support_tone(s: GlutenSupport) -> &'static str {
    match s {
        GlutenSupport::DedicatedGlutenFree | GlutenSupport::GlutenFreeOptions => "tone-info",
        GlutenSupport::NoGlutenFreeOptions => "tone-danger",
        GlutenSupport::Unknown => "tone-unknown",
    }
}

#[component]
pub fn RestaurantsScreen() -> impl IntoView {
    let nav = use_nav();
    let host = NodeRef::<leptos::html::Div>::new();
    let center = StoredValue::new(FALLBACK);
    let moved = RwSignal::new(false);
    let filter = RwSignal::new(Filter::All);
    let query = RwSignal::new(String::from("restaurantes"));
    let results = RwSignal::new(None::<Result<Vec<Restaurant>, Failure>>);
    let loading = RwSignal::new(false);
    let located = RwSignal::new(None::<bool>);
    let map_failed = RwSignal::new(false);
    let generation = StoredValue::new(0u32);

    let shared = expect_context::<MapCenter>().0;
    let search = move || {
        let (lat, lng) = center.get_value();
        shared.set_value(Some((lat, lng)));
        let q = query.get_untracked();
        let mode = match filter.get_untracked() {
            Filter::All => "",
            Filter::Options => "&glutenMode=options",
            Filter::Dedicated => "&glutenMode=dedicated",
        };
        let ticket = generation.get_value() + 1;
        generation.set_value(ticket);
        loading.set(true);
        moved.set(false);
        spawn_local(async move {
            let path = format!(
                "/restaurants/search?q={}&latitude={lat:.5}&longitude={lng:.5}{mode}",
                api::enc(&q)
            );
            let result = api::get::<Items<Restaurant>>(&path).await.map(|r| r.items);
            if generation.get_value() != ticket {
                return;
            }
            if let Ok(items) = &result {
                map_set_places(&places_json(items, None));
            }
            results.set(Some(result));
            loading.set(false);
        });
    };

    // Mount the map once the host element exists.
    Effect::new(move |_| {
        let Some(el) = host.get() else { return };
        let on_move = Closure::<dyn FnMut(f64, f64, f64, bool)>::new(
            move |lat, lng, _zoom, by_user: bool| {
                center.set_value((lat, lng));
                if by_user {
                    moved.set(true);
                }
            },
        );
        let on_select =
            Closure::<dyn FnMut(String)>::new(move |id: String| nav.push(Route::Restaurant(id)));
        let on_error = Closure::<dyn FnMut()>::new(move || map_failed.set(true));
        map_mount(
            &el,
            FALLBACK.0,
            FALLBACK.1,
            12.0,
            on_move.as_ref(),
            on_select.as_ref(),
            on_error.as_ref(),
        );
        // The map lives for the whole app session.
        on_move.forget();
        on_select.forget();
        on_error.forget();
        spawn_local(async move {
            match locate().await {
                Some((lat, lng)) => {
                    located.set(Some(true));
                    center.set_value((lat, lng));
                    map_fly_to(lat, lng, 14.0);
                }
                None => located.set(Some(false)),
            }
            search();
        });
    });

    let relocate = move |_| {
        spawn_local(async move {
            if let Some((lat, lng)) = locate().await {
                located.set(Some(true));
                center.set_value((lat, lng));
                map_fly_to(lat, lng, 14.0);
                query.set("restaurantes".into());
                search();
            } else {
                located.set(Some(false));
            }
        });
    };

    let chip = move |value: Filter, label: &'static str| {
        view! {
            <button
                class="chip"
                class:active=move || filter.get() == value
                aria-pressed=move || (filter.get() == value).to_string()
                on:click=move |_| { filter.set(value); search(); }
            >
                {label}
            </button>
        }
    };

    view! {
        <div class="map-wrap">
            <div class="map" node_ref=host></div>
            <SuggestBox query=query center=center on_search=Callback::new(move |_| search()) />
            <div class="map-controls">
                <button class="icon-button raised" aria-label="Ir a mi ubicación" on:click=relocate>
                    <Icon svg=icons::LOCATE />
                </button>
            </div>
            <Show when=move || moved.get()>
                <button class="button raised area-button" on:click=move |_| { query.set("restaurantes".into()); search(); }>
                    "Buscar en esta zona"
                </button>
            </Show>
        </div>
        <section class="sheet">
            <div class="chips" role="group" aria-label="Filtro de gluten">
                {chip(Filter::All, "Todos")}
                {chip(Filter::Options, "Con opciones sin gluten")}
                {chip(Filter::Dedicated, "Todo sin gluten")}
            </div>
            <Show when=move || map_failed.get()>
                <ErrorBox message="No se pudo cargar el mapa. La lista sigue funcionando." />
            </Show>
            <Show when=move || located.get() == Some(false)>
                <p class="muted small">"No tenemos tu ubicación: mueve el mapa y pulsa «Buscar en esta zona»."</p>
            </Show>
            <Show when=move || loading.get()>
                <Loading label="Buscando restaurantes…" />
            </Show>
            {move || match results.get() {
                None => ().into_any(),
                Some(Err(e)) => view! { <ErrorBox message=e.message retry=Callback::new(move |_| search()) /> }.into_any(),
                Some(Ok(items)) if items.is_empty() => view! {
                    <Empty title="Nada por aquí" text="No encontramos restaurantes en esta zona con ese filtro." />
                }.into_any(),
                Some(Ok(items)) => view! {
                    <p class="muted small" role="status">{format!("{} restaurantes", items.len())}</p>
                    <ul class="list">
                        {items.into_iter().map(|r| view! { <li><RestaurantRow item=r /></li> }).collect_view()}
                    </ul>
                }.into_any(),
            }}
        </section>
    }
}

#[component]
pub fn RestaurantRow(item: Restaurant) -> impl IntoView {
    let nav = use_nav();
    let id = item.id.clone();
    view! {
        <button class="row" on:click=move |_| nav.push(Route::Restaurant(id.clone()))>
            <span class="thumb icon-thumb" inner_html=icons::STORE></span>
            <span class="row-body">
                <span class="row-title">{item.name.clone()}</span>
                <span class="muted small">
                    {[item.distance_m.map(distance), (!item.address.is_empty()).then(|| item.address.clone())]
                        .into_iter()
                        .flatten()
                        .collect::<Vec<_>>()
                        .join(" · ")}
                </span>
                <span class=format!("badge {}", support_tone(item.gluten_support))>{item.gluten_support.label()}</span>
                {item.chain.clone().map(|c| view! { <span class="muted small">"Cadena: " {c.name}</span> })}
            </span>
            <Icon svg=icons::CHEVRON class="chevron" />
        </button>
    }
}

/// Search field with suggestions: chains first ("Cadena X · N locales").
#[component]
fn SuggestBox(
    query: RwSignal<String>,
    center: StoredValue<(f64, f64)>,
    on_search: Callback<()>,
) -> impl IntoView {
    let nav = use_nav();
    let text = RwSignal::new(String::new());
    let items = RwSignal::new(Vec::<RestaurantSuggestion>::new());
    let generation = StoredValue::new(0u32);
    let timer = StoredValue::new(None::<leptos::prelude::TimeoutHandle>);

    let fetch = move |q: String| {
        let ticket = generation.get_value() + 1;
        generation.set_value(ticket);
        let (lat, lng) = center.get_value();
        spawn_local(async move {
            let path = format!(
                "/restaurants/suggest?q={}&latitude={lat:.5}&longitude={lng:.5}&includeChains=true",
                api::enc(&q)
            );
            if let Ok(found) = api::get::<Items<RestaurantSuggestion>>(&path).await {
                if generation.get_value() == ticket {
                    let mut list = found.items;
                    list.sort_by_key(|s| s.kind != SuggestionKind::Chain);
                    list.truncate(6);
                    items.set(list);
                }
            }
        });
    };
    let on_input = move |ev| {
        let value = event_target_value(&ev);
        text.set(value.clone());
        if let Some(h) = timer.get_value() {
            h.clear();
        }
        if value.trim().chars().count() < 2 {
            items.set(vec![]);
            return;
        }
        // Debounce: one request when typing pauses.
        let handle = set_timeout_with_handle(
            move || fetch(value.trim().to_string()),
            std::time::Duration::from_millis(250),
        )
        .ok();
        timer.set_value(handle);
    };
    let choose = move |s: RestaurantSuggestion| {
        items.set(vec![]);
        match (s.kind, s.chain_key.clone(), s.id.clone()) {
            (SuggestionKind::Chain, Some(key), _) => nav.push(Route::Chain(key)),
            (SuggestionKind::Restaurant, _, Some(id)) => nav.push(Route::Restaurant(id)),
            _ => {
                if let (Some(lat), Some(lng)) = (s.latitude, s.longitude) {
                    center.set_value((lat, lng));
                    map_fly_to(lat, lng, 14.0);
                    query.set("restaurantes".into());
                } else {
                    query.set(s.query.clone());
                }
                text.set(s.label.clone());
                on_search.run(());
            }
        }
    };
    view! {
        <form class="suggest" role="search" on:submit=move |ev| {
            ev.prevent_default();
            let q = text.get_untracked().trim().to_string();
            if q.chars().count() >= 2 { query.set(q); items.set(vec![]); on_search.run(()); }
        }>
            <span class="icon suggest-icon" inner_html=icons::SEARCH></span>
            <input
                class="input"
                type="search"
                enterkeyhint="search"
                aria-label="Buscar restaurante, cadena o lugar"
                placeholder="Restaurante, cadena o lugar"
                prop:value=move || text.get()
                on:input=on_input
            />
            <Show when=move || items.with(|i| !i.is_empty())>
                <ul class="suggestions" role="listbox">
                    <For each=move || items.get() key=|s| format!("{:?}{}{:?}", s.kind, s.label, s.id) let:s>
                        {
                            // The server already writes "Cadena X · N locales".
                            let label = s.label.clone();
                            let chain = s.kind == SuggestionKind::Chain;
                            view! {
                                <li role="option">
                                    <button type="button" class="suggestion" on:click=move |_| choose(s.clone())>
                                        <span class="icon" inner_html=if chain { icons::STORE } else { icons::PIN }></span>
                                        <span class:strong=chain>{label}</span>
                                    </button>
                                </li>
                            }
                        }
                    </For>
                </ul>
            </Show>
        </form>
    }
}
