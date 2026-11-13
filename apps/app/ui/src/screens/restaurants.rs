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
        on_view: &JsValue,
        on_select: &JsValue,
        on_error: &JsValue,
    );
    #[wasm_bindgen(js_namespace = seMap, js_name = addArea)]
    fn map_add_area(json: &str);
    #[wasm_bindgen(js_namespace = seMap, js_name = setResults)]
    fn map_set_results(json: &str);
    #[wasm_bindgen(js_namespace = seMap, js_name = useResults)]
    fn map_use_results(on: bool);
    #[wasm_bindgen(js_namespace = seMap, js_name = setFilter)]
    fn map_set_filter(value: &str);
    #[wasm_bindgen(js_namespace = seMap, js_name = place)]
    fn map_place(id: &str) -> String;
    #[wasm_bindgen(js_namespace = seMap, js_name = flyTo)]
    fn map_fly_to(lat: f64, lng: f64, zoom: f64);
    #[wasm_bindgen(js_namespace = seMap, js_name = fitPlaces)]
    fn map_fit_places(json: &str);
    #[wasm_bindgen(js_namespace = seMap, js_name = select)]
    fn map_select(id: &str);
}

/// Results a typed search frames on the map.
const FIT_NEAREST: usize = 10;
/// What the list shows when she has not typed anything.
const EVERYTHING: &str = "restaurantes";

/// Puerta del Sol, only until the real location (or the user's pan) arrives.
const FALLBACK: (f64, f64) = (40.4168, -3.7038);
/// Below this zoom the view covers too much ground to list restaurants usefully.
const MIN_AUTO_ZOOM: f64 = 11.0;
/// Quiet time after the map stops before searching again (one request per gesture).
const AUTO_DELAY_MS: u64 = 600;

/// The map's pins come from areas this much wider and taller than the screen on each
/// side (3 × 3 screens), loaded again only when the view nears the edge of what it has.
const AREA_GROW: f64 = 1.0;
/// Loads the next area once the view is within half a screen of the loaded edge.
const AREA_MARGIN: f64 = 0.5;
/// Below this zoom the map shows the clusters it already has and loads nothing.
const AREA_MIN_ZOOM: f64 = 11.5;
/// The server's limit for one area side, in degrees.
const AREA_MAX_SPAN: f64 = 0.59;

/// `[south, west, north, east]`.
type Bounds = [f64; 4];

fn grow(b: Bounds, k: f64) -> Bounds {
    let h = (b[2] - b[0]) * k;
    let w = (b[3] - b[1]) * k;
    [b[0] - h, b[1] - w, b[2] + h, b[3] + w]
}

/// Keeps an area inside the server's limit (around its centre) and the world.
fn cap(b: Bounds) -> Bounds {
    let (lat, lng) = ((b[0] + b[2]) / 2.0, (b[1] + b[3]) / 2.0);
    let h = ((b[2] - b[0]) / 2.0).min(AREA_MAX_SPAN / 2.0);
    let w = ((b[3] - b[1]) / 2.0).min(AREA_MAX_SPAN / 2.0);
    [
        (lat - h).max(-85.0),
        (lng - w).max(-180.0),
        (lat + h).min(85.0),
        (lng + w).min(180.0),
    ]
}

fn contains(outer: Bounds, inner: Bounds) -> bool {
    outer[0] <= inner[0] && outer[1] <= inner[1] && outer[2] >= inner[2] && outer[3] >= inner[3]
}

/// A pin the map loaded with its area (not necessarily in the list).
#[derive(Deserialize)]
struct AreaPlace {
    id: String,
    name: String,
    lat: f64,
    lng: f64,
    support: GlutenSupport,
}

/// What the card over the map shows for the tapped pin.
#[derive(Clone)]
struct Preview {
    id: String,
    name: String,
    support: GlutenSupport,
    detail: String,
}

impl Preview {
    fn from_restaurant(r: &Restaurant) -> Self {
        Preview {
            id: r.id.clone(),
            name: r.name.clone(),
            support: r.gluten_support,
            detail: [
                r.distance_m.map(distance),
                (!r.address.is_empty()).then(|| r.address.clone()),
            ]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>()
            .join(" · "),
        }
    }
}

/// Approximate distance in metres (equirectangular; plenty for "has the view moved?").
fn metres_between(a: (f64, f64), b: (f64, f64)) -> f64 {
    let lat = ((a.0 + b.0) / 2.0).to_radians();
    let dx = (b.1 - a.1).to_radians() * lat.cos();
    let dy = (b.0 - a.0).to_radians();
    6_371_000.0 * (dx * dx + dy * dy).sqrt()
}

/// A fifth of the visible width (about 400 px) at this zoom: smaller pans keep the list.
fn auto_threshold_m(lat: f64, zoom: f64) -> f64 {
    156_543.0 * lat.to_radians().cos() / 2f64.powf(zoom) * 400.0 * 0.2
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
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
    // Quick network position first; without one (Wi-Fi off, weak signal, emulator) the
    // GPS, which takes longer but works anywhere outdoors and near windows.
    for (high, timeout) in [(false, 8_000), (true, 20_000)] {
        let position: Result<Position, _> = api::invoke(
            "plugin:geolocation|get_current_position",
            PositionArgs {
                options: PositionOptions {
                    enable_high_accuracy: high,
                    timeout,
                    maximum_age: 120_000,
                },
            },
        )
        .await;
        match position {
            Ok(p) => return Some((p.coords.latitude, p.coords.longitude)),
            Err(e) => web_sys::console::warn_1(&format!("location ({high}): {}", e.message).into()),
        }
    }
    None
}

/// The list's restaurants as map pins (the same shape as the area's).
fn places_json(items: &[Restaurant]) -> String {
    let places: Vec<serde_json::Value> = items
        .iter()
        .filter_map(|r| {
            Some(serde_json::json!({
                "id": r.id,
                "name": r.name,
                "lat": r.latitude?,
                "lng": r.longitude?,
                "support": r.gluten_support.as_str(),
            }))
        })
        .collect();
    serde_json::Value::Array(places).to_string()
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

/// Position of the panel over the map.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Sheet {
    /// Only the filter: the map is the screen.
    Peek,
    /// About half the screen: the nearest restaurants.
    Half,
    /// Up to the search field.
    Full,
}

/// Visible height of the collapsed panel (handle, status line and filter); the same as
/// `--sheet-peek` in style.css.
const PEEK_PX: f64 = 124.0;
/// Visible share of the screen in the middle position (`--sheet-half`).
const HALF_SHARE: f64 = 0.45;
/// A drag shorter than this is a tap on the handle.
const DRAG_SLOP_PX: f64 = 6.0;
/// Quiet time after a filter tap before the list asks again (taps in a row: the last).
const FILTER_DELAY_MS: u64 = 350;
/// Searches kept for going back to a filter or a place she has just seen.
const CACHE_ENTRIES: usize = 8;
const CACHE_MS: f64 = 5.0 * 60_000.0;
/// Wait after a 429 that does not say how long.
const DEFAULT_WAIT_S: u64 = 10;

/// Translate (px from fully open) of each position, for a panel `height` px tall in a
/// window `window` px tall.
fn sheet_offset(sheet: Sheet, height: f64, window: f64) -> f64 {
    let peek = (height - PEEK_PX).max(0.0);
    match sheet {
        Sheet::Peek => peek,
        Sheet::Half => (height - HALF_SHARE * window).clamp(0.0, peek),
        Sheet::Full => 0.0,
    }
}

/// Where a released drag settles: the position nearest to where it was heading.
fn snap(offset: f64, moved: f64, height: f64, window: f64) -> Sheet {
    let heading = offset + moved * 0.35;
    [Sheet::Full, Sheet::Half, Sheet::Peek]
        .into_iter()
        .min_by(|a, b| {
            let da = (sheet_offset(*a, height, window) - heading).abs();
            let db = (sheet_offset(*b, height, window) - heading).abs();
            da.total_cmp(&db)
        })
        .unwrap_or(Sheet::Peek)
}

/// One list search: what, with which filter, around where.
#[derive(Clone, Debug, PartialEq)]
struct SearchKey {
    query: String,
    filter: Filter,
    at: (f64, f64),
}

impl SearchKey {
    /// The same search near enough that its list still describes the view.
    fn matches(&self, other: &SearchKey, within_m: f64) -> bool {
        self.query == other.query
            && self.filter == other.filter
            && metres_between(self.at, other.at) <= within_m
    }
}

/// Recent list searches, so going back to a filter or a place asks nothing.
#[derive(Default)]
struct ResultCache {
    entries: Vec<(SearchKey, f64, Vec<Restaurant>)>,
}

impl ResultCache {
    fn get(&self, key: &SearchKey, now: f64, within_m: f64) -> Option<Vec<Restaurant>> {
        self.entries
            .iter()
            .rev()
            .find(|(k, at, _)| now - at <= CACHE_MS && k.matches(key, within_m))
            .map(|(_, _, items)| items.clone())
    }

    fn put(&mut self, key: SearchKey, now: f64, items: Vec<Restaurant>) {
        self.entries.retain(|(k, _, _)| k != &key);
        self.entries.push((key, now, items));
        let extra = self.entries.len().saturating_sub(CACHE_ENTRIES);
        self.entries.drain(..extra);
    }
}

/// Seconds to wait when the server says there were too many requests.
fn wait_seconds(failure: &Failure) -> Option<u64> {
    failure
        .rate_limited()
        .then(|| failure.retry_after.unwrap_or(DEFAULT_WAIT_S).clamp(1, 120))
}

/// The same rule as the map's pins (public/map.js), to show a filter at once while its
/// own list is on its way.
fn passes(filter: Filter, support: GlutenSupport) -> bool {
    match filter {
        Filter::All => true,
        Filter::Options => matches!(
            support,
            GlutenSupport::GlutenFreeOptions | GlutenSupport::DedicatedGlutenFree
        ),
        Filter::Dedicated => support == GlutenSupport::DedicatedGlutenFree,
    }
}

fn now_ms() -> f64 {
    js_sys::Date::now()
}

fn window_height() -> f64 {
    window()
        .inner_height()
        .ok()
        .and_then(|v| v.as_f64())
        .unwrap_or(800.0)
}

#[component]
pub fn RestaurantsScreen() -> impl IntoView {
    let nav = use_nav();
    let host = NodeRef::<leptos::html::Div>::new();
    let sheet_el = NodeRef::<leptos::html::Section>::new();
    let grab_el = NodeRef::<leptos::html::Div>::new();
    let center = StoredValue::new(FALLBACK);
    let zoom = StoredValue::new(12.0f64);
    let filter = RwSignal::new(Filter::All);
    let query = RwSignal::new(String::from(EVERYTHING));
    // The search field's text lives here so «Quitar búsqueda» can empty it.
    let text = RwSignal::new(String::new());
    let selected = RwSignal::new(None::<Preview>);
    // The last list received, with the filter it was asked with (kept while the next
    // one loads: the panel never empties).
    let results = RwSignal::new(None::<(Filter, Vec<Restaurant>)>);
    let failure = RwSignal::new(None::<Failure>);
    let loading = RwSignal::new(false);
    let located = RwSignal::new(None::<bool>);
    let map_failed = RwSignal::new(false);
    let too_far = RwSignal::new(false);
    let auto_timer = StoredValue::new(None::<leptos::prelude::TimeoutHandle>);
    let filter_timer = StoredValue::new(None::<leptos::prelude::TimeoutHandle>);
    // Her position, for the distance on the card of a pin outside the list.
    let me = StoredValue::new(None::<(f64, f64)>);
    // Areas already on the map, the one on its way and whether the catalog covers it
    // (outside it the pins are the list's results).
    let loaded = StoredValue::new(Vec::<Bounds>::new());
    let in_flight = StoredValue::new(None::<Bounds>);
    let covered = RwSignal::new(None::<bool>);
    // Share of the last area with catalog data: below 95 % part of the map has no data,
    // which is not the same as having no restaurants.
    let coverage = RwSignal::new(None::<f64>);
    let area_loading = RwSignal::new(false);
    let area_wait_until = StoredValue::new(0.0f64);
    let last_view = StoredValue::new(None::<(Bounds, f64)>);
    // `load_area` cannot name itself: its retry after a 429 goes through this copy.
    let area_slot = StoredValue::new(None::<Callback<(Bounds, f64)>>);

    // The panel.
    let sheet = RwSignal::new(Sheet::Peek);
    let drag = RwSignal::new(None::<f64>);
    let grab = StoredValue::new(None::<(f64, f64, f64)>);

    // The list's requests: one at a time, the latest intention wins, recent answers are
    // reused and a 429 waits for the time the server gives.
    let intent = StoredValue::new(None::<bool>);
    let busy = StoredValue::new(false);
    let wait_until = StoredValue::new(0.0f64);
    let retry_in = RwSignal::new(None::<u64>);
    let ticker = StoredValue::new(None::<leptos::prelude::IntervalHandle>);
    let shown = StoredValue::new(None::<SearchKey>);
    let cache = StoredValue::new(ResultCache::default());
    // A search skipped because nobody looks at the list (collapsed panel, pins from the
    // catalog): made when the panel opens.
    let skipped = StoredValue::new(false);
    let pump = StoredValue::new(None::<Callback<()>>);
    let kick = move || {
        if let Some(p) = pump.get_value() {
            p.run(());
        }
    };

    let load_area = move |view: Bounds, z: f64| {
        last_view.set_value(Some((view, z)));
        if z < AREA_MIN_ZOOM || now_ms() < area_wait_until.get_value() {
            return;
        }
        let need = cap(grow(view, AREA_MARGIN));
        let have = loaded.with_value(|l| l.iter().any(|b| contains(*b, need)))
            || in_flight.get_value().is_some_and(|b| contains(b, need));
        if have {
            return;
        }
        let want = cap(grow(view, AREA_GROW));
        in_flight.set_value(Some(want));
        area_loading.set(true);
        spawn_local(async move {
            let path = format!(
                "/restaurants/area?south={:.4}&west={:.4}&north={:.4}&east={:.4}",
                want[0], want[1], want[2], want[3]
            );
            let result = api::get::<serde_json::Value>(&path).await;
            // Only the latest area decides coverage: an older answer arriving late
            // must not describe the view she has moved to.
            let latest = in_flight.get_value() == Some(want);
            if latest {
                in_flight.set_value(None);
                area_loading.set(false);
            }
            match result {
                Ok(found) => {
                    map_add_area(&found["items"].to_string());
                    // A crowded area comes back cut (its farthest places missing):
                    // count only the part around the view as loaded.
                    let done = if found["truncated"].as_bool() == Some(true) {
                        need
                    } else {
                        want
                    };
                    loaded.update_value(|l| {
                        l.push(done);
                        let extra = l.len().saturating_sub(24);
                        l.drain(..extra);
                    });
                    if latest {
                        covered.set(found["covered"].as_bool());
                        coverage.set(found["coverage"].as_f64());
                    }
                }
                // Too many requests: wait what the server says, then load the view
                // she is looking at by then (once).
                Err(f) => {
                    if let Some(seconds) = wait_seconds(&f) {
                        area_wait_until.set_value(now_ms() + seconds as f64 * 1000.0);
                        set_timeout(
                            move || {
                                if let (Some(again), Some(view)) =
                                    (area_slot.get_value(), last_view.get_value())
                                {
                                    again.run(view);
                                }
                            },
                            std::time::Duration::from_secs(seconds),
                        );
                    }
                }
            }
        });
    };
    area_slot.set_value(Some(Callback::new(move |(view, z)| load_area(view, z))));

    // A typed search shows its results; browsing shows the whole area (or the list's
    // places where the catalog has no data).
    Effect::new(move |_| {
        let typed = query.with(|q| q != EVERYTHING);
        map_use_results(typed || covered.get() == Some(false));
    });

    let shared = expect_context::<MapCenter>().0;
    let list_needed = move || {
        sheet.get_untracked() != Sheet::Peek
            || query.with_untracked(|q| q != EVERYTHING)
            || covered.get_untracked() == Some(false)
    };

    let show = move |key: SearchKey, items: Vec<Restaurant>, fit: bool| {
        map_set_results(&places_json(&items));
        // A typed search: frame the nearest few (a chain can have 50 venues across the
        // region); the card only stays if its place is a result.
        if fit {
            map_fit_places(&places_json(&items[..items.len().min(FIT_NEAREST)]));
            let keep = selected.with_untracked(|s| {
                s.as_ref()
                    .is_some_and(|s| items.iter().any(|r| r.id == s.id))
            });
            if !keep {
                selected.set(None);
                map_select("");
            }
        }
        failure.set(None);
        results.set(Some((key.filter, items)));
        shown.set_value(Some(key));
    };

    let start_wait = move |seconds: u64| {
        wait_until.set_value(now_ms() + seconds as f64 * 1000.0);
        retry_in.set(Some(seconds));
        if let Some(h) = ticker.get_value() {
            h.clear();
        }
        let handle = set_interval_with_handle(
            move || {
                let left = ((wait_until.get_value() - now_ms()) / 1000.0).ceil();
                if left <= 0.0 {
                    if let Some(h) = ticker.get_value() {
                        h.clear();
                    }
                    ticker.set_value(None);
                    retry_in.set(None);
                    kick();
                } else {
                    retry_in.set(Some(left as u64));
                }
            },
            std::time::Duration::from_secs(1),
        )
        .ok();
        ticker.set_value(handle);
    };

    pump.set_value(Some(Callback::new(move |_| {
        if busy.get_value() || now_ms() < wait_until.get_value() {
            return; // the answer on its way, or the end of the wait, runs it
        }
        let Some(fit) = intent.get_value() else {
            return;
        };
        intent.set_value(None);
        let (lat, lng) = center.get_value();
        shared.set_value(Some((lat, lng)));
        let key = SearchKey {
            query: query.get_untracked(),
            filter: filter.get_untracked(),
            at: (lat, lng),
        };
        let within = auto_threshold_m(lat, zoom.get_value());
        let current = results.with_untracked(Option::is_some)
            && shown.with_value(|s| s.as_ref().is_some_and(|s| s.matches(&key, within)));
        if current && !fit {
            loading.set(false);
            return;
        }
        if let Some(items) = cache.with_value(|c| c.get(&key, now_ms(), within)) {
            show(key, items, fit);
            loading.set(false);
            return;
        }
        let mode = match key.filter {
            Filter::All => "",
            Filter::Options => "&glutenMode=options",
            Filter::Dedicated => "&glutenMode=dedicated",
        };
        let path = format!(
            "/restaurants/search?q={}&latitude={lat:.5}&longitude={lng:.5}{mode}",
            api::enc(&key.query)
        );
        busy.set_value(true);
        loading.set(true);
        spawn_local(async move {
            let result = api::get::<Items<Restaurant>>(&path).await.map(|r| r.items);
            busy.set_value(false);
            match result {
                Ok(items) => {
                    cache.update_value(|c| c.put(key.clone(), now_ms(), items.clone()));
                    // A newer intention (another filter, a move, a typed search) makes
                    // this answer old: only the latest is shown.
                    if intent.get_value().is_none() {
                        show(key, items, fit);
                    }
                }
                Err(f) => match wait_seconds(&f) {
                    Some(seconds) => {
                        // Keep the intention: it runs by itself when the wait ends.
                        if intent.get_value().is_none() {
                            intent.set_value(Some(fit));
                        }
                        start_wait(seconds);
                    }
                    None if intent.get_value().is_none() => failure.set(Some(f)),
                    None => {}
                },
            }
            if intent.get_value().is_some() && retry_in.get_untracked().is_none() {
                kick();
            } else {
                loading.set(false);
            }
        });
    })));

    // Ask for the list (or note that it is owed when nobody is looking at it).
    let request = move |fit: bool| {
        if !fit && !list_needed() {
            skipped.set_value(true);
            return;
        }
        skipped.set_value(false);
        intent.set_value(Some(fit || intent.get_value() == Some(true)));
        loading.set(true);
        kick();
    };
    let search = move || request(false);
    let typed_search = move || request(query.get_untracked() != EVERYTHING);

    // Opening the panel (or leaving the catalog) shows the list it owes.
    Effect::new(move |_| {
        let open = sheet.get() != Sheet::Peek;
        let uncovered = covered.get() == Some(false);
        if (open || uncovered) && skipped.get_value() {
            search();
        }
    });

    // Mount the map once the host element exists.
    Effect::new(move |_| {
        let Some(el) = host.get() else { return };
        let on_move = Closure::<dyn FnMut(f64, f64, f64, bool)>::new(
            move |lat, lng, z: f64, by_user: bool| {
                center.set_value((lat, lng));
                zoom.set_value(z);
                if !by_user {
                    return;
                }
                if let Some(h) = auto_timer.get_value() {
                    h.clear();
                }
                let far = z < MIN_AUTO_ZOOM;
                too_far.set(far);
                let moved_enough = shown.with_value(|s| {
                    s.as_ref()
                        .is_none_or(|s| metres_between(s.at, (lat, lng)) > auto_threshold_m(lat, z))
                });
                if far || !moved_enough {
                    return;
                }
                // Like map apps: the list follows the map once she stops moving it.
                let handle = set_timeout_with_handle(
                    search,
                    std::time::Duration::from_millis(AUTO_DELAY_MS),
                )
                .ok();
                auto_timer.set_value(handle);
            },
        );
        let on_view = Closure::<dyn FnMut(f64, f64, f64, f64, f64)>::new(
            move |south, west, north, east, z: f64| load_area([south, west, north, east], z),
        );
        let on_select = Closure::<dyn FnMut(String)>::new(move |id: String| {
            let found = results.with_untracked(|r| {
                r.as_ref()
                    .and_then(|(_, items)| items.iter().find(|x| x.id == id))
                    .map(Preview::from_restaurant)
            });
            // A pin of the area outside the list: what the map knows is enough.
            let found = found.or_else(|| {
                let place: AreaPlace = serde_json::from_str(&map_place(&id)).ok()?;
                Some(Preview {
                    detail: me
                        .get_value()
                        .map(|here| distance(metres_between(here, (place.lat, place.lng))))
                        .unwrap_or_default(),
                    id: place.id,
                    name: place.name,
                    support: place.support,
                })
            });
            match found {
                Some(p) => {
                    map_select(&p.id);
                    selected.set(Some(p));
                    sheet.set(Sheet::Peek);
                }
                None => nav.push(Route::Restaurant(id)),
            }
        });
        let on_error = Closure::<dyn FnMut()>::new(move || map_failed.set(true));
        map_mount(
            &el,
            FALLBACK.0,
            FALLBACK.1,
            12.0,
            on_move.as_ref(),
            on_view.as_ref(),
            on_select.as_ref(),
            on_error.as_ref(),
        );
        // The map lives for the whole app session.
        on_move.forget();
        on_view.forget();
        on_select.forget();
        on_error.forget();
        spawn_local(async move {
            match locate().await {
                Some((lat, lng)) => {
                    located.set(Some(true));
                    me.set_value(Some((lat, lng)));
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
                me.set_value(Some((lat, lng)));
                center.set_value((lat, lng));
                map_fly_to(lat, lng, 14.0);
                query.set(EVERYTHING.into());
                text.set(String::new());
                search();
            } else {
                located.set(Some(false));
            }
        });
    };

    let segment = move |value: Filter, label: &'static str| {
        view! {
            <button
                type="button"
                role="radio"
                class:active=move || filter.get() == value
                aria-checked=move || (filter.get() == value).to_string()
                on:click=move |_| {
                    if filter.get_untracked() == value {
                        return; // the same filter again asks nothing
                    }
                    filter.set(value);
                    // The pins change at once; the list follows once the taps stop.
                    map_set_filter(match value {
                        Filter::All => "all",
                        Filter::Options => "options",
                        Filter::Dedicated => "dedicated",
                    });
                    if let Some(h) = filter_timer.get_value() {
                        h.clear();
                    }
                    loading.set(list_needed());
                    let handle = set_timeout_with_handle(
                        typed_search,
                        std::time::Duration::from_millis(FILTER_DELAY_MS),
                    )
                    .ok();
                    filter_timer.set_value(handle);
                }
            >
                {label}
            </button>
        }
    };
    let clear_query = move |_| {
        query.set(EVERYTHING.into());
        text.set(String::new());
        search();
    };
    let close_preview = move |_| {
        selected.set(None);
        map_select("");
    };

    // Dragging the handle moves the panel with the finger; letting go settles it.
    let on_down = move |ev: web_sys::PointerEvent| {
        let Some(el) = sheet_el.get_untracked() else {
            return;
        };
        let height = el.offset_height() as f64;
        let offset = drag
            .get_untracked()
            .unwrap_or_else(|| sheet_offset(sheet.get_untracked(), height, window_height()));
        grab.set_value(Some((ev.client_y() as f64, offset, height)));
        if let Some(g) = grab_el.get_untracked() {
            let _ = g.set_pointer_capture(ev.pointer_id());
        }
    };
    let on_drag = move |ev: web_sys::PointerEvent| {
        let Some((y0, offset, height)) = grab.get_value() else {
            return;
        };
        let moved = ev.client_y() as f64 - y0;
        if drag.get_untracked().is_some() || moved.abs() > DRAG_SLOP_PX {
            let max = sheet_offset(Sheet::Peek, height, window_height());
            drag.set(Some((offset + moved).clamp(0.0, max)));
        }
    };
    let on_up = move |ev: web_sys::PointerEvent| {
        let Some((y0, _, height)) = grab.get_value() else {
            return;
        };
        grab.set_value(None);
        match drag.get_untracked() {
            Some(at) => {
                let moved = ev.client_y() as f64 - y0;
                drag.set(None);
                sheet.set(snap(at, moved, height, window_height()));
            }
            // A tap on the handle: open to the middle, or close.
            None => sheet.update(|s| {
                *s = if *s == Sheet::Peek {
                    Sheet::Half
                } else {
                    Sheet::Peek
                }
            }),
        }
    };
    let on_cancel = move |_: web_sys::PointerEvent| {
        grab.set_value(None);
        drag.set(None);
    };

    // What the list shows: its own answer, or the previous one narrowed to the new
    // filter while that answer arrives.
    let visible = move || {
        results.with(|r| {
            r.as_ref().map(|(asked, items)| {
                let now = filter.get();
                if *asked == now {
                    items.clone()
                } else {
                    items
                        .iter()
                        .filter(|x| passes(now, x.gluten_support))
                        .cloned()
                        .collect::<Vec<_>>()
                }
            })
        })
    };
    let status = move || {
        if let Some(s) = retry_in.get() {
            return format!("Muchas búsquedas seguidas: reintentamos en {s} s");
        }
        if loading.get() {
            return "Actualizando…".to_string();
        }
        if failure.with(Option::is_some) {
            return "No se ha podido cargar la lista".to_string();
        }
        match visible().map(|v| v.len()) {
            Some(n) if sheet.get() != Sheet::Peek || !skipped.get_value() => format!(
                "{n} {} cerca",
                if n == 1 {
                    "restaurante"
                } else {
                    "restaurantes"
                }
            ),
            _ => "Desliza hacia arriba para ver la lista".to_string(),
        }
    };

    view! {
        <div
            class="map-wrap"
            class:sheet-peek=move || sheet.get() == Sheet::Peek && drag.get().is_none()
            class:sheet-half=move || sheet.get() == Sheet::Half && drag.get().is_none()
        >
            <div class="map" node_ref=host></div>
            <SuggestBox query=query text=text center=center on_search=Callback::new(move |_| typed_search()) />
            <div class="map-controls" class:lifted=move || selected.with(Option::is_some)>
                <button class="icon-button raised" aria-label="Ir a mi ubicación" on:click=relocate>
                    <Icon svg=icons::LOCATE />
                </button>
            </div>
            <Show when=move || too_far.get() && selected.with(Option::is_none)>
                <p class="map-hint raised" role="status">"Acerca el mapa para ver restaurantes"</p>
            </Show>
            <Show when=move || {
                !too_far.get()
                    && selected.with(Option::is_none)
                    && coverage.get().is_some_and(|c| c < 0.95)
            }>
                <p class="map-hint raised" role="status">
                    {move || if coverage.get().unwrap_or(0.0) < 0.05 {
                        "Sin datos de restaurantes en esta zona"
                    } else {
                        "Parte de esta zona no tiene datos"
                    }}
                </p>
            </Show>
            {move || selected.get().map(|r| {
                let id = r.id.clone();
                view! {
                    <div class="map-preview raised" role="dialog" aria-label=r.name.clone()>
                        <button class="map-preview-body" on:click=move |_| nav.push(Route::Restaurant(id.clone()))>
                            <span class="row-title">{r.name.clone()}</span>
                            {(!r.detail.is_empty()).then(|| view! { <span class="muted small">{r.detail.clone()}</span> })}
                            <span class="map-preview-foot">
                                <span class=format!("badge {}", support_tone(r.support))>{r.support.label()}</span>
                                <span class="map-preview-open">"Ver ficha" <Icon svg=icons::CHEVRON /></span>
                            </span>
                        </button>
                        <button class="icon-button" aria-label="Cerrar" on:click=close_preview>
                            <Icon svg=icons::CLOSE />
                        </button>
                    </div>
                }
            })}
        </div>
        <section
            class="sheet"
            node_ref=sheet_el
            class:peek=move || sheet.get() == Sheet::Peek
            class:half=move || sheet.get() == Sheet::Half
            class:full=move || sheet.get() == Sheet::Full
            class:dragging=move || drag.get().is_some()
            style:transform=move || drag.get().map(|y| format!("translateY({y}px)"))
            aria-label="Restaurantes cercanos"
            on:keydown=move |ev: web_sys::KeyboardEvent| {
                if ev.key() == "Escape" {
                    sheet.set(Sheet::Peek);
                }
            }
        >
            <div class="sheet-progress" class:on=move || area_loading.get() || loading.get() aria-hidden="true"></div>
            <div
                class="sheet-grab"
                node_ref=grab_el
                on:pointerdown=on_down
                on:pointermove=on_drag
                on:pointerup=on_up
                on:pointercancel=on_cancel
            >
                <button
                    type="button"
                    class="sheet-handle"
                    aria-expanded=move || (sheet.get() != Sheet::Peek).to_string()
                    aria-label=move || if sheet.get() == Sheet::Peek { "Mostrar la lista" } else { "Ocultar la lista" }
                    on:keydown=move |ev: web_sys::KeyboardEvent| {
                        match ev.key().as_str() {
                            "ArrowUp" => sheet.update(|s| *s = if *s == Sheet::Peek { Sheet::Half } else { Sheet::Full }),
                            "ArrowDown" => sheet.update(|s| *s = if *s == Sheet::Full { Sheet::Half } else { Sheet::Peek }),
                            "Enter" | " " => {
                                ev.prevent_default();
                                sheet.update(|s| *s = if *s == Sheet::Peek { Sheet::Half } else { Sheet::Peek });
                            }
                            _ => {}
                        }
                    }
                >
                    <span class="sheet-bar" aria-hidden="true"></span>
                </button>
                <div class="sheet-status">
                    <p class="small" role="status" aria-live="polite">
                        <Show when=move || loading.get() && retry_in.get().is_none()>
                            <span class="spinner tiny" aria-hidden="true"></span>
                        </Show>
                        {status}
                    </p>
                    <Show when=move || query.with(|q| q != EVERYTHING)>
                        <button class="chip active small" on:click=clear_query on:pointerdown=|ev| ev.stop_propagation() aria-label="Quitar búsqueda">
                            {move || format!("«{}»", query.get())}
                            <Icon svg=icons::CLOSE />
                        </button>
                    </Show>
                </div>
            </div>
            <div
                class="segmented three"
                role="radiogroup"
                aria-label="Filtro de gluten"
                aria-busy=move || loading.get().to_string()
            >
                {segment(Filter::All, "Todos")}
                {segment(Filter::Options, "Con opciones")}
                {segment(Filter::Dedicated, "100 % sin gluten")}
            </div>
            <div class="sheet-body">
                <details class="legend-box">
                    <summary class="small">"Qué significan los colores"</summary>
                    <ul class="legend" aria-label="Colores del mapa">
                        <li><span class="dot dedicated"></span>"100 % sin gluten"</li>
                        <li><span class="dot options"></span>"Opciones"</li>
                        <li><span class="dot none"></span>"Sin opciones"</li>
                        <li><span class="dot unknown"></span>"Sin datos"</li>
                    </ul>
                </details>
                <Show when=move || map_failed.get()>
                    <ErrorBox message="No se pudo cargar el mapa. La lista sigue funcionando." />
                </Show>
                <Show when=move || located.get() == Some(false)>
                    <p class="muted small">"No tenemos tu ubicación: mueve el mapa y la lista se actualizará sola."</p>
                </Show>
                {move || failure.get().map(|f| view! {
                    <ErrorBox message=f.message retry=Callback::new(move |_| { failure.set(None); request(true) }) />
                })}
                // First load: spinner. Later updates keep the current list (no jumps).
                <Show when=move || loading.get() && results.with(Option::is_none)>
                    <Loading label="Buscando restaurantes…" />
                </Show>
                {move || match visible() {
                    None => ().into_any(),
                    Some(items) if items.is_empty() => view! {
                        <Empty title="Nada por aquí" text="No hay restaurantes con ese filtro en lo que tenemos de esta zona. Mueve el mapa o prueba «Todos»." />
                    }.into_any(),
                    Some(items) => view! {
                        <ul class="list">
                            {items.into_iter().map(|r| view! { <li><RestaurantRow item=r /></li> }).collect_view()}
                        </ul>
                    }.into_any(),
                }}
            </div>
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
fn hide_keyboard() {
    use wasm_bindgen::JsCast;
    if let Some(el) = document()
        .active_element()
        .and_then(|e| e.dyn_into::<web_sys::HtmlElement>().ok())
    {
        let _ = el.blur();
    }
}

#[component]
fn SuggestBox(
    query: RwSignal<String>,
    text: RwSignal<String>,
    center: StoredValue<(f64, f64)>,
    on_search: Callback<()>,
) -> impl IntoView {
    let nav = use_nav();
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
        hide_keyboard();
        match (s.kind, s.chain_key.clone(), s.id.clone()) {
            (SuggestionKind::Chain, Some(key), _) => nav.push(Route::Chain(key)),
            (SuggestionKind::Restaurant, _, Some(id)) => nav.push(Route::Restaurant(id)),
            _ => {
                if let (Some(lat), Some(lng)) = (s.latitude, s.longitude) {
                    center.set_value((lat, lng));
                    map_fly_to(lat, lng, 14.0);
                    query.set(EVERYTHING.into());
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
            if q.chars().count() >= 2 { query.set(q); items.set(vec![]); hide_keyboard(); on_search.run(()); }
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

#[cfg(test)]
mod tests {
    use super::*;

    fn restaurant(id: &str, support: &str) -> Restaurant {
        serde_json::from_value(serde_json::json!({
            "id": id, "name": id, "address": "", "mapsUrl": "https://m", "glutenSupport": support,
            "evidence": [], "source": "s", "attributions": [], "fetchedAt": "t"
        }))
        .unwrap()
    }

    fn key(filter: Filter, at: (f64, f64)) -> SearchKey {
        SearchKey {
            query: EVERYTHING.into(),
            filter,
            at,
        }
    }

    #[test]
    fn the_panel_settles_where_the_finger_was_heading() {
        let (h, w) = (700.0, 800.0);
        let peek = sheet_offset(Sheet::Peek, h, w);
        let half = sheet_offset(Sheet::Half, h, w);
        assert_eq!(peek, 700.0 - PEEK_PX);
        assert!(half > 0.0 && half < peek);
        assert_eq!(sheet_offset(Sheet::Full, h, w), 0.0);
        // Small moves stay; a fling goes on to the next position.
        assert_eq!(snap(peek - 20.0, -20.0, h, w), Sheet::Peek);
        assert_eq!(snap(half + 10.0, 10.0, h, w), Sheet::Half);
        assert_eq!(snap(half - 120.0, -200.0, h, w), Sheet::Full);
        assert_eq!(snap(half + 120.0, 200.0, h, w), Sheet::Peek);
        // A short screen never puts the middle position below the collapsed one.
        assert!(sheet_offset(Sheet::Half, 200.0, 300.0) <= sheet_offset(Sheet::Peek, 200.0, 300.0));
    }

    #[test]
    fn recent_searches_are_reused_and_bounded() {
        let mut cache = ResultCache::default();
        let here = (40.4168, -3.7038);
        cache.put(
            key(Filter::All, here),
            0.0,
            vec![restaurant("a", "UNKNOWN")],
        );
        // Same search a few metres away: reused. Other filter or far away: not.
        assert!(cache
            .get(&key(Filter::All, (40.4169, -3.7038)), 1000.0, 200.0)
            .is_some());
        assert!(cache
            .get(&key(Filter::Options, here), 1000.0, 200.0)
            .is_none());
        assert!(cache
            .get(&key(Filter::All, (40.43, -3.7038)), 1000.0, 200.0)
            .is_none());
        // Old answers are not reused.
        assert!(cache
            .get(&key(Filter::All, here), CACHE_MS + 1.0, 200.0)
            .is_none());
        for i in 0..20 {
            cache.put(key(Filter::All, (i as f64, 0.0)), 0.0, vec![]);
        }
        assert_eq!(cache.entries.len(), CACHE_ENTRIES);
    }

    #[test]
    fn a_429_waits_what_the_server_says() {
        let mut f = Failure {
            status: 429,
            code: "RATE_LIMIT".into(),
            message: "m".into(),
            retry_after: Some(42),
        };
        assert_eq!(wait_seconds(&f), Some(42));
        f.retry_after = None;
        assert_eq!(wait_seconds(&f), Some(DEFAULT_WAIT_S));
        f.retry_after = Some(100_000);
        assert_eq!(wait_seconds(&f), Some(120));
        f.status = 500;
        assert_eq!(wait_seconds(&f), None);
    }

    #[test]
    fn the_provisional_list_uses_the_map_rule() {
        use GlutenSupport::*;
        assert!(passes(Filter::All, Unknown));
        assert!(passes(Filter::Options, GlutenFreeOptions));
        assert!(passes(Filter::Options, DedicatedGlutenFree));
        assert!(!passes(Filter::Options, Unknown));
        assert!(!passes(Filter::Options, NoGlutenFreeOptions));
        assert!(passes(Filter::Dedicated, DedicatedGlutenFree));
        assert!(!passes(Filter::Dedicated, GlutenFreeOptions));
        let _ = restaurant("x", "DEDICATED_GLUTEN_FREE");
    }
}
