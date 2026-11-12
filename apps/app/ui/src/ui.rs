//! Small shared pieces: header, icons, states, product rows.

use leptos::prelude::*;
use safeeat_contracts::Product;

use crate::icons;
use crate::nav::{use_nav, Route};
use crate::verdict;

#[component]
pub fn Icon(svg: &'static str, #[prop(optional)] class: &'static str) -> impl IntoView {
    view! { <span class=format!("icon {class}") inner_html=svg></span> }
}

/// Screen header. Shows a back button when there is somewhere to go back to.
#[component]
pub fn Header(
    #[prop(into)] title: Signal<String>,
    #[prop(optional)] children: Option<Children>,
) -> impl IntoView {
    let nav = use_nav();
    view! {
        <header class="header">
            <Show when=move || { nav.depth() > 1 }>
                <button class="icon-button" aria-label="Volver" on:click=move |_| nav.back()>
                    <Icon svg=icons::BACK />
                </button>
            </Show>
            <h1 class="title">{move || title.get()}</h1>
            {children.map(|c| c())}
        </header>
    }
}

#[component]
pub fn Loading(#[prop(optional)] label: &'static str) -> impl IntoView {
    let label = if label.is_empty() {
        "Cargando…"
    } else {
        label
    };
    view! {
        <div class="loading" role="status">
            <span class="spinner" aria-hidden="true"></span>
            <span>{label}</span>
        </div>
    }
}

#[component]
pub fn ErrorBox(
    #[prop(into)] message: String,
    #[prop(optional)] retry: Option<Callback<()>>,
) -> impl IntoView {
    view! {
        <div class="notice tone-danger" role="alert">
            <Icon svg=icons::ALERT />
            <div class="notice-body">
                <p>{message}</p>
                {retry.map(|r| view! {
                    <button class="button ghost small" on:click=move |_| r.run(())>"Reintentar"</button>
                })}
            </div>
        </div>
    }
}

#[component]
pub fn Empty(#[prop(into)] title: String, #[prop(into)] text: String) -> impl IntoView {
    view! {
        <div class="empty">
            <p class="empty-title">{title}</p>
            <p class="muted">{text}</p>
        </div>
    }
}

/// Compact gluten label used in lists.
#[component]
pub fn SummaryBadge(product: Product) -> impl IntoView {
    let s = verdict::summary(&product);
    view! {
        <span class=format!("badge {}", s.tone.class())>
            <span class="icon" inner_html=s.tone.icon()></span>
            {s.text}
        </span>
        // The qualifier is part of the answer (AI or community): written out, not an icon.
        {s.warning.map(|w| view! {
            <span class="muted small badge-note"><span class="icon" inner_html=icons::ALERT></span>{w}</span>
        })}
    }
}

#[component]
pub fn ProductRow(product: Product) -> impl IntoView {
    let nav = use_nav();
    let id = product.id.clone();
    let name = product.name.clone();
    let brand = product.brand.clone();
    let image = product.image_url.clone();
    view! {
        <button class="row" on:click=move |_| nav.push(Route::Product(id.clone()))>
            <span class="thumb">
                {image.map(|src| view! { <img src=src alt="" loading="lazy" decoding="async" /> })}
            </span>
            <span class="row-body">
                <span class="row-title">{name}</span>
                {brand.map(|b| view! { <span class="muted small">{b}</span> })}
                <SummaryBadge product=product />
            </span>
            <Icon svg=icons::CHEVRON class="chevron" />
        </button>
    }
}
