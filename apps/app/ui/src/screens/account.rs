use leptos::prelude::*;
use leptos::task::spawn_local;
use safeeat_contracts::User;
use serde::Serialize;
use serde_json::{json, Value};

use crate::api::{self, Failure};
use crate::nav::{use_nav, use_session};
use crate::ui::{ErrorBox, Header};

#[derive(Serialize)]
struct AuthArgs {
    mode: &'static str,
    body: Value,
}

#[derive(Serialize)]
struct NoArgs {}

#[component]
pub fn LoginScreen() -> impl IntoView {
    let nav = use_nav();
    let session = use_session();
    let register = RwSignal::new(false);
    let email = RwSignal::new(String::new());
    let password = RwSignal::new(String::new());
    let first = RwSignal::new(String::new());
    let last = RwSignal::new(String::new());
    let birth = RwSignal::new(String::new());
    let busy = RwSignal::new(false);
    let error = RwSignal::new(None::<String>);

    let submit = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        if busy.get_untracked() {
            return;
        }
        let creating = register.get_untracked();
        if creating && password.get_untracked().chars().count() < 12 {
            error.set(Some(
                "La contraseña necesita al menos 12 caracteres.".into(),
            ));
            return;
        }
        busy.set(true);
        error.set(None);
        let body = if creating {
            json!({
                "email": email.get_untracked().trim(),
                "password": password.get_untracked(),
                "firstName": first.get_untracked().trim(),
                "lastName": last.get_untracked().trim(),
                "dateOfBirth": birth.get_untracked(),
            })
        } else {
            json!({ "email": email.get_untracked().trim(), "password": password.get_untracked() })
        };
        spawn_local(async move {
            let result: Result<User, Failure> = api::invoke(
                "authenticate",
                AuthArgs {
                    mode: if creating { "register" } else { "login" },
                    body,
                },
            )
            .await;
            // The screen may be gone by now: `try_` never panics on a disposed signal.
            busy.try_set(false);
            match result {
                Ok(user) => {
                    session.0.set(Some(user));
                    password.try_set(String::new());
                    nav.back();
                }
                Err(e) => {
                    error.try_set(Some(e.message));
                }
            }
        });
    };

    let title = Signal::derive(move || {
        if register.get() {
            "Crear cuenta".to_string()
        } else {
            "Iniciar sesión".to_string()
        }
    });
    view! {
        <Header title=title />
        <form class="stack" on:submit=submit>
            <div class="field">
                <label class="label" for="email">"Email"</label>
                <input id="email" class="input" type="email" autocomplete="email" inputmode="email" required
                    prop:value=move || email.get() on:input=move |ev| email.set(event_target_value(&ev)) />
            </div>
            <div class="field">
                <label class="label" for="password">"Contraseña"</label>
                <input id="password" class="input" type="password" required
                    autocomplete=move || if register.get() { "new-password" } else { "current-password" }
                    prop:value=move || password.get() on:input=move |ev| password.set(event_target_value(&ev)) />
                <Show when=move || register.get()>
                    <p class="muted small">"Al menos 12 caracteres."</p>
                </Show>
            </div>
            <Show when=move || register.get()>
                <div class="field">
                    <label class="label" for="first">"Nombre"</label>
                    <input id="first" class="input" autocomplete="given-name" required
                        prop:value=move || first.get() on:input=move |ev| first.set(event_target_value(&ev)) />
                </div>
                <div class="field">
                    <label class="label" for="last">"Apellidos"</label>
                    <input id="last" class="input" autocomplete="family-name" required
                        prop:value=move || last.get() on:input=move |ev| last.set(event_target_value(&ev)) />
                </div>
                <div class="field">
                    <label class="label" for="birth">"Fecha de nacimiento"</label>
                    <input id="birth" class="input" type="date" autocomplete="bday" required
                        prop:value=move || birth.get() on:input=move |ev| birth.set(event_target_value(&ev)) />
                </div>
            </Show>
            {move || error.get().map(|m| view! { <ErrorBox message=m /> })}
            <button class="button" type="submit" disabled=move || busy.get()>
                {move || match (busy.get(), register.get()) {
                    (true, _) => "Un momento…",
                    (false, true) => "Crear cuenta",
                    (false, false) => "Entrar",
                }}
            </button>
            <button class="button ghost" type="button" on:click=move |_| { register.update(|r| *r = !*r); error.set(None); }>
                {move || if register.get() { "Ya tengo cuenta" } else { "Crear una cuenta nueva" }}
            </button>
            <p class="muted small">"Tu sesión se guarda cifrada en este móvil hasta que la cierres."</p>
        </form>
        <div class="stack update-login"><crate::update::UpdateCard /></div>
    }
}

#[component]
pub fn ProfileScreen() -> impl IntoView {
    let nav = use_nav();
    let session = use_session();
    let meta = RwSignal::new(None::<String>);
    spawn_local(async move {
        if let Ok(m) = api::get::<safeeat_contracts::Meta>("/meta").await {
            meta.try_set(Some(format!("Servidor {} · {}", m.version, m.environment)));
        }
    });
    let logout = move |_| {
        spawn_local(async move {
            let _: Result<(), Failure> = api::invoke("logout", NoArgs {}).await;
            session.0.set(None);
            nav.back();
        });
    };
    view! {
        <Header title="Tu cuenta".to_string() />
        <section class="stack">
            {move || session.0.get().map(|u| view! {
                <div class="card">
                    <p class="row-title">{u.first_name.clone().unwrap_or_default()} " " {u.last_name.clone().unwrap_or_default()}</p>
                    <p class="muted">{u.email.clone()}</p>
                    {u.is_trusted().then(|| view! { <span class="badge tone-safe">"Cuenta de confianza"</span> })}
                </div>
            })}
            <Show when=move || session.0.with(Option::is_some)>
                <crate::screens::settings::AccountSettings />
            </Show>
            <crate::update::UpdateCard />
            <button class="button ghost" on:click=logout>"Cerrar sesión"</button>
            <p class="muted small">"SafeEat " {env!("CARGO_PKG_VERSION")} {move || meta.get().map(|m| format!(" · {m}"))}</p>
        </section>
    }
}
