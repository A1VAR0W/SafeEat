//! App shell: three tabs, a stack of screens per visit, and the Android back button
//! (each push adds a history entry; `popstate` pops it).

use leptos::ev;
use leptos::prelude::*;
use safeeat_contracts::User;

use crate::icons;
use crate::screens;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Scan,
    Restaurants,
    Saved,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Route {
    Home,
    /// Product by barcode (scan) or by id (`off-…`, lists).
    Product(String),
    Restaurant(String),
    Chain(String),
    Login,
    Profile,
}

#[derive(Clone, Copy)]
pub struct Nav {
    pub tab: RwSignal<Tab>,
    stack: RwSignal<Vec<Route>>,
}

impl Nav {
    pub fn current(&self) -> Route {
        self.stack
            .with(|s| s.last().cloned())
            .unwrap_or(Route::Home)
    }
    pub fn depth(&self) -> usize {
        self.stack.with(Vec::len)
    }
    pub fn push(&self, route: Route) {
        self.stack.update(|s| s.push(route));
        push_history();
        scroll_top();
    }
    pub fn back(&self) {
        let _ = window().history().map(|h| h.back());
    }
    pub fn switch(&self, tab: Tab) {
        if self.tab.get_untracked() == tab {
            // Tapping the active tab returns to its first screen.
            if self.depth() > 1 {
                self.stack.set(vec![Route::Home]);
            }
            return;
        }
        self.tab.set(tab);
        self.stack.set(vec![Route::Home]);
        push_history();
        scroll_top();
    }
    fn pop(&self) {
        if self.depth() > 1 {
            self.stack.update(|s| {
                s.pop();
            });
        } else if self.tab.get_untracked() != Tab::Scan {
            self.tab.set(Tab::Scan);
        }
    }
}

fn push_history() {
    if let Ok(history) = window().history() {
        let _ = history.push_state_with_url(&wasm_bindgen::JsValue::NULL, "", None);
    }
}

fn scroll_top() {
    window().scroll_to_with_x_and_y(0.0, 0.0);
}

/// Signed-in user, if any. The tokens themselves stay in the Tauri shell.
#[derive(Clone, Copy)]
pub struct Session(pub RwSignal<Option<User>>);

/// Last map centre (real location or where she panned), for "nearest branches".
#[derive(Clone, Copy)]
pub struct MapCenter(pub StoredValue<Option<(f64, f64)>>);

pub fn use_nav() -> Nav {
    expect_context::<Nav>()
}

pub fn use_session() -> Session {
    expect_context::<Session>()
}

#[component]
pub fn App() -> impl IntoView {
    let nav = Nav {
        tab: RwSignal::new(Tab::Scan),
        stack: RwSignal::new(vec![Route::Home]),
    };
    provide_context(nav);
    provide_context(Session(RwSignal::new(None)));
    provide_context(MapCenter(StoredValue::new(None)));
    let handle = window_event_listener(ev::popstate, move |_| nav.pop());
    on_cleanup(move || handle.remove());

    let screen = move || {
        let route = nav.current();
        match (nav.tab.get(), route) {
            (_, Route::Product(id)) => view! { <screens::product::ProductScreen id=id /> }.into_any(),
            (_, Route::Restaurant(id)) => {
                view! { <screens::restaurant::RestaurantScreen id=id /> }.into_any()
            }
            (_, Route::Chain(key)) => view! { <screens::restaurant::ChainScreen key=key /> }.into_any(),
            (_, Route::Login) => view! { <screens::account::LoginScreen /> }.into_any(),
            (_, Route::Profile) => view! { <screens::account::ProfileScreen /> }.into_any(),
            (Tab::Scan, Route::Home) => view! { <screens::scan::ScanScreen /> }.into_any(),
            (Tab::Saved, Route::Home) => view! { <screens::saved::SavedScreen /> }.into_any(),
            // The map lives outside this switch so it is never torn down.
            (Tab::Restaurants, Route::Home) => ().into_any(),
        }
    };
    let map_visible = move || nav.tab.get() == Tab::Restaurants && nav.depth() == 1;
    let map_started = RwSignal::new(false);
    Effect::new(move |_| {
        if map_visible() {
            map_started.set(true);
        }
    });

    view! {
        <div class="app">
            <main class="screen" class:screen-map=map_visible>
                {screen}
                <Show when=move || map_started.get()>
                    <div class="map-host" class:hidden=move || !map_visible()>
                        <screens::restaurants::RestaurantsScreen />
                    </div>
                </Show>
            </main>
            <TabBar />
        </div>
    }
}

#[component]
fn TabBar() -> impl IntoView {
    let nav = use_nav();
    let item = move |tab: Tab, label: &'static str, icon: &'static str| {
        let active = move || nav.tab.get() == tab;
        view! {
            <button
                class="tab"
                class:active=active
                aria-current=move || if active() { Some("page") } else { None }
                on:click=move |_| nav.switch(tab)
            >
                <span class="icon" inner_html=icon></span>
                <span class="tab-label">{label}</span>
            </button>
        }
    };
    view! {
        <nav class="tabbar" aria-label="Secciones">
            {item(Tab::Scan, "Escanear", icons::SCAN)}
            {item(Tab::Restaurants, "Restaurantes", icons::PIN)}
            {item(Tab::Saved, "Guardados", icons::BOOKMARK)}
        </nav>
    }
}
