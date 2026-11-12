//! Account settings: complete the profile, change e-mail or password, delete the account.
//! The calls that return a new session go through the shell (`account_change`), so the
//! tokens never reach this code.

use leptos::prelude::*;
use leptos::task::spawn_local;
use safeeat_contracts::User;
use serde::Serialize;
use serde_json::{json, Value};

use crate::api::{self, Failure};
use crate::nav::{use_nav, use_session};
use crate::ui::ErrorBox;

#[derive(Serialize)]
struct ChangeArgs {
    kind: &'static str,
    body: Value,
}

#[derive(Serialize)]
struct BodyArgs {
    body: Value,
}

/// One labelled input bound to a signal.
#[component]
fn Field(
    id: &'static str,
    label: &'static str,
    value: RwSignal<String>,
    #[prop(optional)] kind: &'static str,
    #[prop(optional)] autocomplete: &'static str,
    #[prop(optional)] hint: &'static str,
) -> impl IntoView {
    let kind = if kind.is_empty() { "text" } else { kind };
    view! {
        <div class="field">
            <label class="label" for=id>{label}</label>
            <input
                id=id
                class="input"
                type=kind
                autocomplete=autocomplete
                prop:value=move || value.get()
                on:input=move |ev| value.set(event_target_value(&ev))
            />
            {(!hint.is_empty()).then(|| view! { <p class="muted small">{hint}</p> })}
        </div>
    }
}

/// Shared state of a small form: busy flag, error and "done" message.
#[derive(Clone, Copy)]
struct FormState {
    busy: RwSignal<bool>,
    error: RwSignal<Option<String>>,
    done: RwSignal<Option<&'static str>>,
}

impl FormState {
    fn new() -> Self {
        Self {
            busy: RwSignal::new(false),
            error: RwSignal::new(None),
            done: RwSignal::new(None),
        }
    }
    fn start(self) -> bool {
        if self.busy.get_untracked() {
            return false;
        }
        self.busy.set(true);
        self.error.set(None);
        self.done.set(None);
        true
    }
    // The screen may be gone when the answer arrives: `try_` never panics.
    fn finish(self, result: Result<&'static str, Failure>) {
        self.busy.try_set(false);
        match result {
            Ok(message) => {
                self.done.try_set(Some(message));
            }
            Err(e) => {
                self.error.try_set(Some(e.message));
            }
        }
    }
}

#[component]
fn FormFeedback(form: FormState) -> impl IntoView {
    view! {
        {move || form.error.get().map(|m| view! { <ErrorBox message=m /> })}
        {move || form.done.get().map(|m| view! { <p class="field-ok" role="status">{m}</p> })}
    }
}

#[component]
pub fn AccountSettings() -> impl IntoView {
    let session = use_session();
    let incomplete = move || {
        session.0.with(|u| {
            u.as_ref()
                .is_some_and(|u| u.first_name.is_none() || u.date_of_birth.is_none())
        })
    };
    view! {
        <Show when=incomplete>
            <CompleteProfile />
        </Show>
        <details class="card disclosure">
            <summary>"Cambiar el correo"</summary>
            <ChangeEmail />
        </details>
        <details class="card disclosure">
            <summary>"Cambiar la contraseña"</summary>
            <ChangePassword />
        </details>
        <details class="card disclosure danger-zone">
            <summary>"Borrar mi cuenta"</summary>
            <DeleteAccount />
        </details>
    }
}

#[component]
fn CompleteProfile() -> impl IntoView {
    let session = use_session();
    let form = FormState::new();
    let (first, last, birth) = (
        RwSignal::new(String::new()),
        RwSignal::new(String::new()),
        RwSignal::new(String::new()),
    );
    let submit = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        if !form.start() {
            return;
        }
        let body = json!({
            "firstName": first.get_untracked().trim(),
            "lastName": last.get_untracked().trim(),
            "dateOfBirth": birth.get_untracked(),
        });
        spawn_local(async move {
            let result = api::send::<User>("POST", "/auth/profile/complete", Some(body)).await;
            form.finish(result.map(|user| {
                session.0.set(Some(user));
                "Perfil guardado."
            }));
        });
    };
    view! {
        <form class="card form-row" on:submit=submit>
            <h3 class="card-title">"Completa tu perfil"</h3>
            <Field id="profile-first" label="Nombre" value=first autocomplete="given-name" />
            <Field id="profile-last" label="Apellidos" value=last autocomplete="family-name" />
            <Field id="profile-birth" label="Fecha de nacimiento" value=birth kind="date" autocomplete="bday" />
            <FormFeedback form=form />
            <button class="button" type="submit" disabled=move || form.busy.get()>"Guardar"</button>
        </form>
    }
}

#[component]
fn ChangeEmail() -> impl IntoView {
    let session = use_session();
    let form = FormState::new();
    let (email, password) = (RwSignal::new(String::new()), RwSignal::new(String::new()));
    let submit = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        if !form.start() {
            return;
        }
        let body = json!({
            "email": email.get_untracked().trim(),
            "currentPassword": password.get_untracked(),
        });
        spawn_local(async move {
            let result: Result<User, Failure> = api::invoke(
                "account_change",
                ChangeArgs {
                    kind: "email",
                    body,
                },
            )
            .await;
            form.finish(result.map(|user| {
                session.0.set(Some(user));
                password.try_set(String::new());
                "Correo cambiado."
            }));
        });
    };
    view! {
        <form class="form-row" on:submit=submit>
            <Field id="new-email" label="Correo nuevo" value=email kind="email" autocomplete="email" />
            <Field id="email-password" label="Tu contraseña actual" value=password kind="password" autocomplete="current-password" />
            <FormFeedback form=form />
            <button class="button" type="submit" disabled=move || form.busy.get()>"Cambiar correo"</button>
        </form>
    }
}

#[component]
fn ChangePassword() -> impl IntoView {
    let session = use_session();
    let form = FormState::new();
    let (current, new) = (RwSignal::new(String::new()), RwSignal::new(String::new()));
    let submit = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        if new.with_untracked(|p| p.chars().count() < 12) {
            form.error.set(Some(
                "La contraseña nueva necesita al menos 12 caracteres.".into(),
            ));
            return;
        }
        if !form.start() {
            return;
        }
        let body = json!({
            "currentPassword": current.get_untracked(),
            "newPassword": new.get_untracked(),
        });
        spawn_local(async move {
            let result: Result<User, Failure> = api::invoke(
                "account_change",
                ChangeArgs {
                    kind: "password",
                    body,
                },
            )
            .await;
            form.finish(result.map(|user| {
                session.0.set(Some(user));
                current.try_set(String::new());
                new.try_set(String::new());
                "Contraseña cambiada. Las demás sesiones se han cerrado."
            }));
        });
    };
    view! {
        <form class="form-row" on:submit=submit>
            <Field id="current-password" label="Contraseña actual" value=current kind="password" autocomplete="current-password" />
            <Field id="new-password" label="Contraseña nueva" value=new kind="password" autocomplete="new-password" hint="12 caracteres como mínimo." />
            <FormFeedback form=form />
            <button class="button" type="submit" disabled=move || form.busy.get()>"Cambiar contraseña"</button>
        </form>
    }
}

#[component]
fn DeleteAccount() -> impl IntoView {
    let session = use_session();
    let nav = use_nav();
    let form = FormState::new();
    let (password, confirmation) = (RwSignal::new(String::new()), RwSignal::new(String::new()));
    let ready =
        move || confirmation.with(|c| c.trim() == "ELIMINAR") && password.with(|p| !p.is_empty());
    let submit = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        if !ready() || !form.start() {
            return;
        }
        let body = json!({
            "currentPassword": password.get_untracked(),
            "confirmation": "ELIMINAR",
        });
        spawn_local(async move {
            let result: Result<(), Failure> =
                api::invoke("delete_account", BodyArgs { body }).await;
            match result {
                Ok(()) => {
                    session.0.set(None);
                    nav.back();
                }
                Err(e) => form.finish(Err(e)),
            }
        });
    };
    view! {
        <form class="form-row" on:submit=submit>
            <p>"Se borran tu cuenta, tus veredictos, guardados y reseñas. No se puede deshacer."</p>
            <Field id="delete-password" label="Tu contraseña" value=password kind="password" autocomplete="current-password" />
            <Field id="delete-confirm" label="Escribe ELIMINAR para confirmar" value=confirmation autocomplete="off" />
            <FormFeedback form=form />
            <button class="button danger" type="submit" disabled=move || form.busy.get() || !ready()>
                "Borrar mi cuenta para siempre"
            </button>
        </form>
    }
}
