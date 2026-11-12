use leptos::prelude::*;
use leptos::task::spawn_local;
use safeeat_contracts::{Page, Product};

use crate::api::{self, Failure};
use crate::nav::{use_nav, use_session, Route};
use crate::ui::{Empty, ErrorBox, Header, Loading, ProductRow};

#[component]
pub fn SavedScreen() -> impl IntoView {
    let nav = use_nav();
    let session = use_session();
    let items = RwSignal::new(None::<Result<Vec<Product>, Failure>>);
    let load = move || {
        if session.0.with_untracked(Option::is_none) {
            return;
        }
        spawn_local(async move {
            let result = api::get::<Page<Product>>("/favorites/products").await.map(|p| p.items);
            items.try_set(Some(result));
        });
    };
    load();
    view! {
        <Header title="Guardados".to_string() />
        <section class="stack">
            {move || if session.0.with(Option::is_none) {
                view! {
                    <Empty title="Tus productos, a mano" text="Inicia sesión para guardar productos y verlos aquí." />
                    <button class="button" on:click=move |_| nav.push(Route::Login)>"Iniciar sesión"</button>
                }.into_any()
            } else {
                match items.get() {
                    None => view! { <Loading /> }.into_any(),
                    Some(Err(e)) => view! { <ErrorBox message=e.message retry=Callback::new(move |_| load()) /> }.into_any(),
                    Some(Ok(list)) if list.is_empty() => view! {
                        <Empty title="Aún no hay nada" text="Guarda productos desde su ficha con el corazón." />
                    }.into_any(),
                    Some(Ok(list)) => view! {
                        <ul class="list">{list.into_iter().map(|p| view! { <li><ProductRow product=p /></li> }).collect_view()}</ul>
                    }.into_any(),
                }
            }}
        </section>
    }
}
