//! "Guardados": favourite products, her own verdicts ("Mis productos seguros"),
//! bookmarks (collections) and saved restaurants.

use leptos::prelude::*;
use leptos::task::spawn_local;
use safeeat_contracts::{Collection, Items, Page, Product, Restaurant, VerdictValue};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::api::{self, Failure};
use crate::icons;
use crate::nav::{use_nav, use_session, Route};
use crate::screens::restaurants::support_tone;
use crate::ui::{Empty, ErrorBox, Header, Icon, Loading, ProductRow};

/// What the products list shows.
#[derive(Clone, PartialEq)]
enum Shelf {
    Favorites,
    Safe,
    Unsafe,
    Collection(String),
}

#[component]
pub fn SavedScreen() -> impl IntoView {
    let nav = use_nav();
    let session = use_session();
    let restaurants = RwSignal::new(false);
    view! {
        <Header title="Guardados".to_string() />
        <section class="stack">
            {move || if session.0.with(Option::is_none) {
                view! {
                    <Empty title="Tus productos, a mano" text="Inicia sesión para guardar productos y restaurantes y verlos aquí." />
                    <button class="button" on:click=move |_| nav.push(Route::Login)>"Iniciar sesión"</button>
                }.into_any()
            } else {
                view! {
                    <div class="segmented" role="tablist" aria-label="Qué ver">
                        <button role="tab" aria-selected=move || (!restaurants.get()).to_string()
                            class:active=move || !restaurants.get() on:click=move |_| restaurants.set(false)>"Productos"</button>
                        <button role="tab" aria-selected=move || restaurants.get().to_string()
                            class:active=move || restaurants.get() on:click=move |_| restaurants.set(true)>"Restaurantes"</button>
                    </div>
                    {move || if restaurants.get() {
                        view! { <SavedRestaurants /> }.into_any()
                    } else {
                        view! { <SavedProducts /> }.into_any()
                    }}
                }.into_any()
            }}
        </section>
    }
}

#[component]
fn SavedProducts() -> impl IntoView {
    let shelf = RwSignal::new(Shelf::Favorites);
    let collections = RwSignal::new(None::<Result<Vec<Collection>, Failure>>);
    let load_collections = move || {
        spawn_local(async move {
            let result = api::get::<Items<Collection>>("/collections")
                .await
                .map(|i| i.items);
            collections.try_set(Some(result));
        });
    };
    load_collections();
    let chip = move |value: Shelf, label: String| {
        // A memo is `Copy`, so both attributes can read it.
        let selected = value.clone();
        let active = Memo::new(move |_| shelf.with(|s| *s == selected));
        view! {
            <button type="button" role="tab" class="chip" class:active=active
                aria-selected=move || active.get().to_string()
                on:click=move |_| shelf.set(value.clone())>{label}</button>
        }
    };
    view! {
        <div class="chips wrap" role="tablist" aria-label="Listas">
            {chip(Shelf::Favorites, "Favoritos".into())}
            {chip(Shelf::Safe, "Me sientan bien".into())}
            {chip(Shelf::Unsafe, "No son seguros".into())}
            {move || match collections.get() {
                Some(Ok(list)) => list.into_iter().map(|c| chip(Shelf::Collection(c.id.clone()), format!("{} · {}", c.name, c.count))).collect_view().into_any(),
                _ => ().into_any(),
            }}
        </div>
        {move || match collections.get() {
            Some(Err(e)) => view! { <ErrorBox message=e.message retry=Callback::new(move |_| load_collections()) /> }.into_any(),
            _ => ().into_any(),
        }}
        <NewCollection on_created=Callback::new(move |c: Collection| {
            shelf.set(Shelf::Collection(c.id));
            load_collections();
        }) />
        {move || {
            let current = shelf.get();
            let name = match (&current, collections.get()) {
                (Shelf::Collection(id), Some(Ok(list))) => list.into_iter().find(|c| &c.id == id).map(|c| c.name),
                _ => None,
            };
            view! {
                <ProductShelf shelf=current.clone() />
                {if let (Shelf::Collection(id), Some(name)) = (current, name) {
                    view! {
                        <DeleteCollection id=id name=name on_deleted=Callback::new(move |_| {
                            shelf.set(Shelf::Favorites);
                            load_collections();
                        }) />
                    }.into_any()
                } else {
                    ().into_any()
                }}
            }
        }}
    }
}

#[component]
fn ProductShelf(shelf: Shelf) -> impl IntoView {
    let state = RwSignal::new(None::<Result<Vec<Product>, Failure>>);
    let cursor = RwSignal::new(None::<String>);
    let more_busy = RwSignal::new(false);
    let shelf = StoredValue::new(shelf);
    let load = move |next: Option<String>| {
        spawn_local(async move {
            let Some(shelf) = shelf.try_get_value() else {
                return;
            };
            let result: Result<(Vec<Product>, Option<String>), Failure> = match shelf {
                Shelf::Favorites => {
                    let path = match &next {
                        Some(c) => format!("/favorites/products?cursor={}", api::enc(c)),
                        None => "/favorites/products".into(),
                    };
                    api::get::<Page<Product>>(&path)
                        .await
                        .map(|p| (p.items, p.next_cursor))
                }
                Shelf::Safe | Shelf::Unsafe => {
                    let wanted = if shelf == Shelf::Safe {
                        VerdictValue::Safe
                    } else {
                        VerdictValue::Unsafe
                    };
                    api::get::<Items<Product>>("/products/verdicts")
                        .await
                        .map(|i| {
                            let items = i
                                .items
                                .into_iter()
                                .filter(|p| {
                                    p.my_verdict.as_ref().is_some_and(|v| v.verdict == wanted)
                                })
                                .collect();
                            (items, None)
                        })
                }
                Shelf::Collection(id) => {
                    api::get::<Items<Product>>(&format!("/collections/{}/products", api::enc(&id)))
                        .await
                        .map(|i| (i.items, None))
                }
            };
            more_busy.try_set(false);
            match result {
                Ok((items, next_cursor)) => {
                    cursor.try_set(next_cursor);
                    if next.is_some() {
                        state.try_update(|s| {
                            if let Some(Ok(list)) = s {
                                list.extend(items);
                            }
                        });
                    } else {
                        state.try_set(Some(Ok(items)));
                    }
                }
                Err(e) => {
                    state.try_set(Some(Err(e)));
                }
            }
        });
    };
    load(None);
    let (empty_title, empty_text) = match shelf.get_value() {
        Shelf::Favorites => (
            "Aún no hay favoritos",
            "Guarda productos desde su ficha con el corazón.",
        ),
        Shelf::Safe => (
            "Nada marcado todavía",
            "En la ficha de un producto, pulsa «Me sienta bien» y aparecerá aquí.",
        ),
        Shelf::Unsafe => (
            "Nada marcado todavía",
            "Los productos que marques como «No es seguro» aparecerán aquí.",
        ),
        Shelf::Collection(_) => (
            "Este marcador está vacío",
            "Añade productos desde su ficha, en «Organizar en marcadores».",
        ),
    };
    let note = matches!(shelf.get_value(), Shelf::Safe | Shelf::Unsafe).then(|| view! {
        <p class="muted small">"Es tu veredicto personal: solo lo ves tú y no cambia el análisis. Si cambian los ingredientes, la ficha te avisa."</p>
    });
    view! {
        {note}
        {move || match state.get() {
            None => view! { <Loading /> }.into_any(),
            Some(Err(e)) => view! { <ErrorBox message=e.message retry=Callback::new(move |_| load(None)) /> }.into_any(),
            Some(Ok(list)) if list.is_empty() => view! { <Empty title=empty_title text=empty_text /> }.into_any(),
            Some(Ok(list)) => view! {
                <ul class="list">{list.into_iter().map(|p| view! { <li><ProductRow product=p /></li> }).collect_view()}</ul>
            }.into_any(),
        }}
        {move || cursor.get().map(|c| view! {
            <button class="button ghost" disabled=move || more_busy.get() on:click=move |_| {
                more_busy.set(true);
                load(Some(c.clone()));
            }>{move || if more_busy.get() { "Cargando…" } else { "Cargar más" }}</button>
        })}
    }
}

#[component]
fn NewCollection(on_created: Callback<Collection>) -> impl IntoView {
    let open = RwSignal::new(false);
    let name = RwSignal::new(String::new());
    let busy = RwSignal::new(false);
    let error = RwSignal::new(None::<String>);
    let submit = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        let value = name.get_untracked().trim().to_string();
        if value.is_empty() || busy.get_untracked() {
            return;
        }
        busy.set(true);
        error.set(None);
        spawn_local(async move {
            let result =
                api::send::<Collection>("POST", "/collections", Some(json!({ "name": value })))
                    .await;
            busy.try_set(false);
            match result {
                Ok(c) => {
                    name.try_set(String::new());
                    open.try_set(false);
                    on_created.run(c);
                }
                Err(e) => {
                    error.try_set(Some(e.message));
                }
            }
        });
    };
    view! {
        <Show
            when=move || open.get()
            fallback=move || view! {
                <button class="button ghost small" on:click=move |_| open.set(true)>"+ Nuevo marcador"</button>
            }
        >
            <form class="card form-row" on:submit=submit>
                <label class="label" for="collection-name">"Nombre del marcador"</label>
                <input id="collection-name" class="input" maxlength="60" autocomplete="off"
                    placeholder="Desayunos, viajes, meriendas…"
                    prop:value=move || name.get()
                    on:input=move |ev| name.set(event_target_value(&ev)) />
                {move || error.get().map(|m| view! { <ErrorBox message=m /> })}
                <div class="two">
                    <button type="button" class="button ghost" on:click=move |_| open.set(false)>"Cancelar"</button>
                    <button type="submit" class="button" disabled=move || busy.get() || name.with(|n| n.trim().is_empty())>
                        {move || if busy.get() { "Creando…" } else { "Crear" }}
                    </button>
                </div>
            </form>
        </Show>
    }
}

#[component]
fn DeleteCollection(id: String, name: String, on_deleted: Callback<()>) -> impl IntoView {
    let asking = RwSignal::new(false);
    let busy = RwSignal::new(false);
    let error = RwSignal::new(None::<String>);
    let id = StoredValue::new(id);
    let delete = move |_| {
        if busy.get_untracked() {
            return;
        }
        busy.set(true);
        spawn_local(async move {
            let path = format!("/collections/{}", api::enc(&id.get_value()));
            let result = api::send::<Value>("DELETE", &path, None).await;
            busy.try_set(false);
            match result {
                Ok(_) => on_deleted.run(()),
                Err(e) => {
                    error.try_set(Some(e.message));
                }
            }
        });
    };
    view! {
        <Show
            when=move || asking.get()
            fallback=move || view! {
                <button class="button ghost small" on:click=move |_| asking.set(true)>"Eliminar este marcador"</button>
            }
        >
            <div class="confirm-box" role="group" aria-label="Eliminar marcador">
                <p class="small">"¿Eliminar «" {name.clone()} "»? Los productos seguirán en sus otras listas."</p>
                <div class="two">
                    <button class="button ghost small" on:click=move |_| asking.set(false)>"Cancelar"</button>
                    <button class="button danger small" disabled=move || busy.get() on:click=delete>"Eliminar"</button>
                </div>
            </div>
        </Show>
        {move || error.get().map(|m| view! { <ErrorBox message=m /> })}
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SavedId {
    id: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RestaurantPage {
    items: Vec<SavedId>,
    #[serde(default)]
    next_cursor: Option<String>,
}

#[component]
fn SavedRestaurants() -> impl IntoView {
    let state = RwSignal::new(None::<Result<Vec<String>, Failure>>);
    let cursor = RwSignal::new(None::<String>);
    let load = move |next: Option<String>| {
        spawn_local(async move {
            let path = match &next {
                Some(c) => format!("/favorites/restaurants?cursor={}", api::enc(c)),
                None => "/favorites/restaurants".into(),
            };
            match api::get::<RestaurantPage>(&path).await {
                Ok(page) => {
                    cursor.try_set(page.next_cursor);
                    let ids: Vec<String> = page.items.into_iter().map(|i| i.id).collect();
                    if next.is_some() {
                        state.try_update(|s| {
                            if let Some(Ok(list)) = s {
                                list.extend(ids);
                            }
                        });
                    } else {
                        state.try_set(Some(Ok(ids)));
                    }
                }
                Err(e) => {
                    state.try_set(Some(Err(e)));
                }
            }
        });
    };
    load(None);
    view! {
        {move || match state.get() {
            None => view! { <Loading /> }.into_any(),
            Some(Err(e)) => view! { <ErrorBox message=e.message retry=Callback::new(move |_| load(None)) /> }.into_any(),
            Some(Ok(ids)) if ids.is_empty() => view! {
                <Empty title="Aún no hay restaurantes" text="Guárdalos desde su ficha con el corazón." />
            }.into_any(),
            Some(Ok(ids)) => view! {
                <ul class="list">{ids.into_iter().map(|id| view! { <li><SavedRestaurant id=id /></li> }).collect_view()}</ul>
            }.into_any(),
        }}
        {move || cursor.get().map(|c| view! {
            <button class="button ghost" on:click=move |_| load(Some(c.clone()))>"Cargar más"</button>
        })}
    }
}

/// The list only has ids: each row loads its own details.
#[component]
fn SavedRestaurant(id: String) -> impl IntoView {
    let nav = use_nav();
    let details = RwSignal::new(None::<Restaurant>);
    let path = format!("/restaurants/{}", api::enc(&id));
    spawn_local(async move {
        if let Ok(r) = api::get::<Restaurant>(&path).await {
            details.try_set(Some(r));
        }
    });
    let target = id.clone();
    view! {
        <button class="row" on:click=move |_| nav.push(Route::Restaurant(target.clone()))>
            <span class="thumb icon-thumb" inner_html=icons::STORE></span>
            <span class="row-body">
                {move || match details.get() {
                    Some(r) => view! {
                        <span class="row-title">{r.name.clone()}</span>
                        {(!r.address.is_empty()).then(|| view! { <span class="muted small">{r.address.clone()}</span> })}
                        <span class=format!("badge {}", support_tone(r.gluten_support))>{r.gluten_support.label()}</span>
                    }.into_any(),
                    None => view! { <span class="row-title">"Restaurante guardado"</span> }.into_any(),
                }}
            </span>
            <Icon svg=icons::CHEVRON class="chevron" />
        </button>
    }
}
