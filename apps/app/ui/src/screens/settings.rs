//! Account settings: complete the profile, change e-mail or password, and, under
//! Opciones avanzadas, delete the account (password + typed word + a last dialog; the
//! server checks the password and the word again).
//! The calls that return a new session go through the shell (`account_change`), so the
//! tokens never reach this code.

use leptos::prelude::*;
use leptos::task::spawn_local;
use safeeat_contracts::User;
use serde::Serialize;
use serde_json::{json, Value};

use crate::api::{self, Failure};
use crate::icons;
use crate::nav::{use_nav, use_session, Route};
use crate::ui::{ErrorBox, Header, Icon};

/// Word she types to confirm the deletion (the server checks the same word).
pub const DELETE_WORD: &str = "ELIMINAR";

/// Two letters for the profile circle: initials of the name, or of the e-mail.
pub fn initials(user: &User) -> String {
    let from_name: String = [user.first_name.as_deref(), user.last_name.as_deref()]
        .into_iter()
        .flatten()
        .filter_map(|w| w.trim().chars().next())
        .take(2)
        .collect();
    let text = if from_name.is_empty() {
        user.email.chars().take(1).collect()
    } else {
        from_name
    };
    text.to_uppercase()
}

/// The deletion button only works with a password and the exact word.
pub fn deletion_ready(password: &str, word: &str) -> bool {
    !password.is_empty() && word.trim() == DELETE_WORD
}

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
    let nav = use_nav();
    view! {
        <h2 class="group-label">"Cuenta"</h2>
        <div class="settings-group">
            <Show when=incomplete>
                <CompleteProfile />
            </Show>
            <details class="settings-fold">
                <summary class="settings-row">
                    <Icon svg=icons::MAIL />
                    <span class="settings-label">"Correo"</span>
                    <Icon svg=icons::CHEVRON class="chevron" />
                </summary>
                <ChangeEmail />
            </details>
            <details class="settings-fold">
                <summary class="settings-row">
                    <Icon svg=icons::LOCK />
                    <span class="settings-label">"Contraseña"</span>
                    <Icon svg=icons::CHEVRON class="chevron" />
                </summary>
                <ChangePassword />
            </details>
            <button class="settings-row" on:click=move |_| nav.push(Route::AccountAdvanced)>
                <Icon svg=icons::SLIDERS />
                <span class="settings-label">"Opciones avanzadas"</span>
                <Icon svg=icons::CHEVRON class="chevron" />
            </button>
        </div>
    }
}

/// Configuración → Cuenta → Opciones avanzadas.
#[component]
pub fn AdvancedScreen() -> impl IntoView {
    let session = use_session();
    view! {
        <Header title="Opciones avanzadas".to_string() />
        <section class="stack">
            <Show
                when=move || session.0.with(Option::is_some)
                fallback=|| view! { <p class="muted">"Inicia sesión para ver las opciones de tu cuenta."</p> }
            >
                <DeleteAccount />
            </Show>
        </section>
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
        <details class="settings-fold">
            <summary class="settings-row">
                <Icon svg=icons::USER />
                <span class="settings-label">"Completa tu perfil"</span>
                <span class="badge tone-caution">"Pendiente"</span>
                <Icon svg=icons::CHEVRON class="chevron" />
            </summary>
            <form class="form-row" on:submit=submit>
                <Field id="profile-first" label="Nombre" value=first autocomplete="given-name" />
                <Field id="profile-last" label="Apellidos" value=last autocomplete="family-name" />
                <Field id="profile-birth" label="Fecha de nacimiento" value=birth kind="date" autocomplete="bday" />
                <FormFeedback form=form />
                <button class="button" type="submit" disabled=move || form.busy.get()>"Guardar"</button>
            </form>
        </details>
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
    let open = RwSignal::new(false);
    let asking = RwSignal::new(false);
    let (password, confirmation) = (RwSignal::new(String::new()), RwSignal::new(String::new()));
    let ready = move || password.with(|p| confirmation.with(|c| deletion_ready(p, c)));
    // First step: the form only opens the last dialog.
    let submit = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        if ready() && !form.busy.get_untracked() {
            asking.set(true);
        }
    };
    let confirm = move |_| {
        if !ready() || !form.start() {
            return;
        }
        let body = json!({
            "currentPassword": password.get_untracked(),
            "confirmation": confirmation.get_untracked().trim(),
        });
        spawn_local(async move {
            let result: Result<(), Failure> =
                api::invoke("delete_account", BodyArgs { body }).await;
            asking.try_set(false);
            match result {
                Ok(()) => {
                    // Session, cached favourites and history go with the account.
                    crate::history::clear();
                    crate::offline::clear();
                    session.0.set(None);
                    nav.home();
                }
                Err(e) => form.finish(Err(e)),
            }
        });
    };
    let on_key = move |ev: leptos::ev::KeyboardEvent| {
        if ev.key() == "Escape" && !form.busy.get_untracked() {
            asking.set(false);
        }
    };
    view! {
        <section class="card">
            <h3 class="card-title danger-text"><Icon svg=icons::TRASH />"Borrar mi cuenta"</h3>
            <p class="small">
                "Se borran tu cuenta, tus veredictos, guardados, colecciones y reseñas. "
                "No se puede deshacer."
            </p>
            <Show
                when=move || open.get()
                fallback=move || view! {
                    <button class="button ghost danger-text" on:click=move |_| open.set(true)>
                        "Quiero borrar mi cuenta"
                    </button>
                }
            >
                <form class="form-row" on:submit=submit>
                    <Field id="delete-password" label="Tu contraseña actual" value=password kind="password" autocomplete="current-password" />
                    <Field id="delete-confirm" label="Escribe ELIMINAR para confirmar" value=confirmation autocomplete="off" />
                    <FormFeedback form=form />
                    <button class="button danger" type="submit" disabled=move || form.busy.get() || !ready()>
                        "Continuar"
                    </button>
                    <button class="button ghost" type="button" on:click=move |_| {
                        open.set(false);
                        password.set(String::new());
                        confirmation.set(String::new());
                        form.error.set(None);
                    }>"Cancelar"</button>
                </form>
            </Show>
        </section>
        <Show when=move || asking.get()>
            <div class="dialog-backdrop" on:keydown=on_key>
                <div class="dialog" role="alertdialog" aria-modal="true"
                    aria-labelledby="delete-title" aria-describedby="delete-text">
                    <h2 id="delete-title" class="dialog-title">"¿Borrar tu cuenta para siempre?"</h2>
                    <p id="delete-text" class="small">
                        "Perderás tus veredictos, guardados y reseñas, y se cerrará la sesión en todos tus dispositivos."
                    </p>
                    <div class="dialog-actions">
                        <button class="button ghost" autofocus disabled=move || form.busy.get()
                            on:click=move |_| asking.set(false)>"Cancelar"</button>
                        <button class="button danger" disabled=move || form.busy.get() on:click=confirm>
                            {move || if form.busy.get() { "Borrando…" } else { "Borrar definitivamente" }}
                        </button>
                    </div>
                </div>
            </div>
        </Show>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn user(first: Option<&str>, last: Option<&str>) -> User {
        serde_json::from_value(serde_json::json!({
            "id": "u1", "email": "ana@example.com", "firstName": first, "lastName": last, "roles": []
        }))
        .unwrap()
    }

    #[test]
    fn the_profile_circle_shows_initials() {
        assert_eq!(initials(&user(Some("Ana"), Some("Prueba Ejemplo"))), "AP");
        assert_eq!(initials(&user(Some("ana"), None)), "A");
        assert_eq!(initials(&user(None, None)), "A");
    }

    #[test]
    fn deleting_needs_the_password_and_the_exact_word() {
        assert!(deletion_ready("clave", "ELIMINAR"));
        assert!(deletion_ready("clave", " ELIMINAR "));
        assert!(!deletion_ready("", "ELIMINAR"));
        assert!(!deletion_ready("clave", "eliminar"));
        assert!(!deletion_ready("clave", "ELIMINA"));
    }
}
