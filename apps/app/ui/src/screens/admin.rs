//! Administration: accounts of trust, review moderation and the audit log.
//! Shown only to ADMIN accounts, but that is a convenience: every call is checked
//! by the server against the role in the database and the live session.

use leptos::prelude::*;
use leptos::task::spawn_local;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::api::{self, Failure};
use crate::icons;
use crate::nav::use_session;
use crate::screens::community::{review_label, short_date};
use crate::ui::{Empty, ErrorBox, Header, Icon, Loading};

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct Page<T> {
    items: Vec<T>,
    #[serde(default)]
    next_cursor: Option<String>,
}

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct AdminUser {
    id: String,
    email: String,
    #[serde(default)]
    first_name: Option<String>,
    #[serde(default)]
    last_name: Option<String>,
    #[serde(default)]
    roles: Vec<String>,
}

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct AdminReview {
    id: String,
    revision: String,
    #[serde(default)]
    resource_name: Option<String>,
    status: String,
    #[serde(default)]
    note: Option<String>,
    updated_at: String,
    #[serde(default)]
    revoked_at: Option<String>,
    #[serde(default)]
    revocation_reason: Option<String>,
    author_id: String,
    author_email: String,
    #[serde(default)]
    author_name: Option<String>,
    #[serde(default)]
    verified_at: Option<String>,
    #[serde(default)]
    verification_method: Option<String>,
    #[serde(default)]
    confirmations: i64,
    #[serde(default)]
    scope: Option<String>,
}

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct AuditEntry {
    id: String,
    action: String,
    target_id: String,
    #[serde(default)]
    details: Value,
    created_at: String,
    #[serde(default)]
    actor_email: Option<String>,
    #[serde(default)]
    target_email: Option<String>,
}

const LOG: &str = "/admin/audit-log";

pub const TRUSTED_ROLE: &str = "authorized_celiac";

/// What an audit entry means, in words.
pub fn action_label(action: &str) -> String {
    match action {
        "authorized_granted" => "Cuenta de confianza concedida".into(),
        "authorized_removed" => "Cuenta de confianza retirada".into(),
        "product_review_verified" => "Revisión de producto verificada".into(),
        "review_revoked" => "Revisión retirada".into(),
        "review_restored" => "Revisión restaurada".into(),
        other => format!("Cambio automático ({other})"),
    }
}

/// How a review counts as checked. A trusted account is not a certification.
pub fn verification_text(method: Option<&str>, confirmations: i64) -> Option<String> {
    match method {
        Some("authorized") => Some("De una cuenta de confianza (no es una certificación)".into()),
        Some(_) if confirmations > 0 => Some(format!(
            "Comprobada por {confirmations} {}",
            if confirmations == 1 {
                "persona"
            } else {
                "personas"
            }
        )),
        _ => None,
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Section {
    Users,
    Reviews,
    Log,
}

impl Section {
    const ALL: [Section; 3] = [Section::Users, Section::Reviews, Section::Log];
    fn label(self) -> &'static str {
        match self {
            Section::Users => "Usuarios",
            Section::Reviews => "Revisiones",
            Section::Log => "Registro",
        }
    }
}

#[component]
pub fn AdminScreen() -> impl IntoView {
    let session = use_session();
    let section = RwSignal::new(Section::Users);
    let is_admin = move || session.0.with(|u| u.as_ref().is_some_and(|u| u.is_admin()));
    view! {
        <Header title="Administración".to_string() />
        <Show
            when=is_admin
            fallback=|| view! {
                <section class="stack"><p class="muted">"Este apartado es solo para administradores."</p></section>
            }
        >
            <section class="stack">
                <div class="tabs" role="tablist" aria-label="Apartados de administración">
                    {Section::ALL.into_iter().map(|s| {
                        let selected = move || section.get() == s;
                        view! {
                            <button type="button" role="tab"
                                aria-selected=move || selected().to_string()
                                class:active=selected
                                on:click=move |_| section.set(s)>
                                {s.label()}
                            </button>
                        }
                    }).collect_view()}
                </div>
                {move || match section.get() {
                    Section::Users => view! { <UsersPanel /> }.into_any(),
                    Section::Reviews => view! { <ReviewsPanel /> }.into_any(),
                    Section::Log => view! { <LogPanel /> }.into_any(),
                }}
            </section>
        </Show>
    }
}

/// Loads one page into `items` (appending when `more`), keeping the next cursor.
fn load_page<T: for<'de> Deserialize<'de> + Clone + Send + Sync + 'static>(
    path: String,
    more: bool,
    items: RwSignal<Vec<T>>,
    cursor: RwSignal<Option<String>>,
    loading: RwSignal<bool>,
    error: RwSignal<Option<String>>,
) {
    loading.set(true);
    error.set(None);
    spawn_local(async move {
        let result: Result<Page<T>, Failure> = api::get(&path).await;
        loading.try_set(false);
        match result {
            Ok(page) => {
                cursor.try_set(page.next_cursor);
                items.try_update(|list| {
                    if !more {
                        list.clear();
                    }
                    list.extend(page.items);
                });
            }
            Err(e) => {
                error.try_set(Some(e.message));
            }
        }
    });
}

fn with_after(base: &str, cursor: Option<String>) -> String {
    match cursor {
        Some(c) => {
            let sep = if base.contains('?') { '&' } else { '?' };
            format!("{base}{sep}after={}", api::enc(&c))
        }
        None => base.to_string(),
    }
}

#[component]
fn UsersPanel() -> impl IntoView {
    let q = RwSignal::new(String::new());
    let items = RwSignal::new(Vec::<AdminUser>::new());
    let cursor = RwSignal::new(None::<String>);
    let loading = RwSignal::new(false);
    let error = RwSignal::new(None::<String>);
    let base = move || format!("/admin/users?q={}", api::enc(q.get_untracked().trim()));
    let search = move || load_page(base(), false, items, cursor, loading, error);
    let more = move || {
        load_page(
            with_after(&base(), cursor.get_untracked()),
            true,
            items,
            cursor,
            loading,
            error,
        )
    };
    search();
    view! {
        <form class="card form-row" role="search" on:submit=move |ev| { ev.prevent_default(); search(); }>
            <label class="label" for="admin-q">"Buscar por nombre o correo"</label>
            <div class="input-row">
                <input id="admin-q" class="input" type="search" autocomplete="off"
                    prop:value=move || q.get()
                    on:input=move |ev| q.set(event_target_value(&ev)) />
                <button class="button" type="submit">"Buscar"</button>
            </div>
        </form>
        <p class="muted small">
            "Las reseñas de una cuenta de confianza se marcan como comprobadas, pero no son una certificación. "
            "El rol de administrador no se puede dar desde la app."
        </p>
        {move || error.get().map(|m| view! { <ErrorBox message=m /> })}
        <ul class="list admin-list">
            {move || items.get().into_iter().map(|u| view! { <li><UserRow user=u items=items /></li> }).collect_view()}
        </ul>
        <Show when=move || loading.get()><Loading /></Show>
        <Show when=move || !loading.get() && items.with(Vec::is_empty) && error.with(Option::is_none)>
            <Empty title="Sin usuarios" text="Prueba con otro nombre o correo." />
        </Show>
        <Show when=move || cursor.with(Option::is_some) && !loading.get()>
            <button class="button ghost small" on:click=move |_| more()>"Cargar más"</button>
        </Show>
    }
}

#[component]
fn UserRow(user: AdminUser, items: RwSignal<Vec<AdminUser>>) -> impl IntoView {
    let trusted = user.roles.iter().any(|r| r == TRUSTED_ROLE);
    let admin = user.roles.iter().any(|r| r == "ADMIN");
    let name = [user.first_name.clone(), user.last_name.clone()]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join(" ");
    let asking = RwSignal::new(false);
    let busy = RwSignal::new(false);
    let error = RwSignal::new(None::<String>);
    let id = StoredValue::new(user.id.clone());
    let email = user.email.clone();
    let change = move |_| {
        busy.set(true);
        error.set(None);
        spawn_local(async move {
            let path = format!("/admin/users/{}/authorized", api::enc(&id.get_value()));
            let method = if trusted { "DELETE" } else { "PUT" };
            let result = api::send::<Value>(method, &path, None).await;
            busy.try_set(false);
            asking.try_set(false);
            match result {
                Ok(v) => {
                    let now = v["authorized"].as_bool().unwrap_or(!trusted);
                    items.try_update(|list| {
                        if let Some(u) = list.iter_mut().find(|u| u.id == id.get_value()) {
                            u.roles.retain(|r| r != TRUSTED_ROLE);
                            if now {
                                u.roles.push(TRUSTED_ROLE.into());
                            }
                        }
                    });
                }
                Err(e) => {
                    error.try_set(Some(e.message));
                }
            }
        });
    };
    view! {
        <article class="admin-item">
            <p class="row-title">{if name.is_empty() { email.clone() } else { name.clone() }}</p>
            {(!name.is_empty()).then(|| view! { <p class="muted small">{email.clone()}</p> })}
            <div class="profile-badges">
                {trusted.then(|| view! { <span class="badge tone-safe"><Icon svg=icons::SHIELD />"Cuenta de confianza"</span> })}
                {admin.then(|| view! { <span class="badge tone-info">"Administración"</span> })}
            </div>
            <Show
                when=move || asking.get()
                fallback=move || view! {
                    <button class="button ghost small" on:click=move |_| asking.set(true)>
                        {if trusted { "Quitar confianza" } else { "Dar confianza" }}
                    </button>
                }
            >
                <div class="confirm-box" role="group" aria-label="Confirmar el cambio de rol">
                    <p class="small">
                        {if trusted {
                            "Sus reseñas dejarán de contar como comprobadas. Queda registrado."
                        } else {
                            "Sus reseñas contarán como comprobadas (no como certificación) y podrá opinar sobre cadenas enteras. Queda registrado."
                        }}
                    </p>
                    <div class="two">
                        <button class="button ghost small" disabled=move || busy.get() on:click=move |_| asking.set(false)>"Cancelar"</button>
                        <button class="button small" disabled=move || busy.get() on:click=change>
                            {move || if busy.get() { "Guardando…" } else if trusted { "Quitar" } else { "Dar confianza" }}
                        </button>
                    </div>
                </div>
            </Show>
            {move || error.get().map(|m| view! { <ErrorBox message=m /> })}
        </article>
    }
}

#[component]
fn ReviewsPanel() -> impl IntoView {
    let restaurants = RwSignal::new(false);
    let revoked = RwSignal::new(false);
    let q = RwSignal::new(String::new());
    let items = RwSignal::new(Vec::<AdminReview>::new());
    let cursor = RwSignal::new(None::<String>);
    let loading = RwSignal::new(false);
    let error = RwSignal::new(None::<String>);
    let base = move || {
        format!(
            "/admin/reviews?kind={}&state={}&q={}",
            if restaurants.get_untracked() {
                "restaurants"
            } else {
                "products"
            },
            if revoked.get_untracked() {
                "revoked"
            } else {
                "active"
            },
            api::enc(q.get_untracked().trim())
        )
    };
    let search = move || load_page(base(), false, items, cursor, loading, error);
    let more = move || {
        load_page(
            with_after(&base(), cursor.get_untracked()),
            true,
            items,
            cursor,
            loading,
            error,
        )
    };
    search();
    let chip = move |label: &'static str, signal: RwSignal<bool>, value: bool| {
        view! {
            <button type="button" class="chip" class:active=move || signal.get() == value
                aria-pressed=move || (signal.get() == value).to_string()
                on:click=move |_| { signal.set(value); search(); }>
                {label}
            </button>
        }
    };
    let reload = Callback::new(move |_| search());
    view! {
        <div class="chips" role="group" aria-label="Tipo">
            {chip("Productos", restaurants, false)}
            {chip("Restaurantes", restaurants, true)}
        </div>
        <div class="chips" role="group" aria-label="Estado">
            {chip("Activas", revoked, false)}
            {chip("Retiradas", revoked, true)}
        </div>
        <form class="card form-row" role="search" on:submit=move |ev| { ev.prevent_default(); search(); }>
            <label class="label" for="admin-rq">"Buscar en las revisiones"</label>
            <div class="input-row">
                <input id="admin-rq" class="input" type="search" autocomplete="off"
                    prop:value=move || q.get()
                    on:input=move |ev| q.set(event_target_value(&ev)) />
                <button class="button" type="submit">"Buscar"</button>
            </div>
        </form>
        {move || error.get().map(|m| view! { <ErrorBox message=m /> })}
        <ul class="list admin-list">
            {move || items.get().into_iter().map(|r| view! {
                <li><ReviewRow review=r restaurant=restaurants.get_untracked() reload=reload /></li>
            }).collect_view()}
        </ul>
        <Show when=move || loading.get()><Loading /></Show>
        <Show when=move || !loading.get() && items.with(Vec::is_empty) && error.with(Option::is_none)>
            <Empty title="Sin revisiones" text="No hay revisiones con estos filtros." />
        </Show>
        <Show when=move || cursor.with(Option::is_some) && !loading.get()>
            <button class="button ghost small" on:click=move |_| more()>"Cargar más"</button>
        </Show>
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Asking {
    Nothing,
    Verify,
    Moderate,
}

#[component]
fn ReviewRow(review: AdminReview, restaurant: bool, reload: Callback<()>) -> impl IntoView {
    let session = use_session();
    let mine = session
        .0
        .with_untracked(|u| u.as_ref().is_some_and(|u| u.id == review.author_id));
    let is_revoked = review.revoked_at.is_some();
    let asking = RwSignal::new(Asking::Nothing);
    let reason = RwSignal::new(String::new());
    let busy = RwSignal::new(false);
    let error = RwSignal::new(None::<String>);
    let id = StoredValue::new(review.id.clone());
    let revision = StoredValue::new(review.revision.clone());
    let kind = if restaurant {
        "restaurants"
    } else {
        "products"
    };
    let can_verify = !restaurant && !mine && !is_revoked && review.verified_at.is_none();
    let run = move |path: String, body: Value| {
        busy.set(true);
        error.set(None);
        spawn_local(async move {
            let result = api::send::<Value>("POST", &path, Some(body)).await;
            busy.try_set(false);
            match result {
                Ok(_) => {
                    asking.try_set(Asking::Nothing);
                    reload.run(());
                }
                Err(e) => {
                    error.try_set(Some(e.message));
                }
            }
        });
    };
    let verify = move |_| {
        run(
            format!(
                "/admin/reviews/products/{}/verify",
                api::enc(&id.get_value())
            ),
            json!({ "revision": revision.get_value() }),
        )
    };
    let moderate = move |_| {
        let text = reason.get_untracked().trim().to_string();
        if text.chars().count() < 5 {
            error.set(Some("Escribe el motivo (al menos 5 caracteres).".into()));
            return;
        }
        run(
            format!(
                "/admin/reviews/{kind}/{}/moderate",
                api::enc(&id.get_value())
            ),
            json!({
                "revision": revision.get_value(),
                "action": if is_revoked { "restore" } else { "revoke" },
                "reason": text,
            }),
        )
    };
    let author = review
        .author_name
        .clone()
        .filter(|n| !n.trim().is_empty())
        .unwrap_or_else(|| review.author_email.clone());
    view! {
        <article class="admin-item">
            <p class="row-title">{review.resource_name.clone().unwrap_or_else(|| "Sin nombre".into())}</p>
            <p><strong>{review_label(&review.status)}</strong>
                {(review.scope.as_deref() == Some("chain")).then_some(" · para toda la cadena")}</p>
            {review.note.clone().filter(|n| !n.is_empty()).map(|n| view! { <p>{n}</p> })}
            <p class="muted small">{format!("{author} · {} · {}", review.author_email, short_date(&review.updated_at))}</p>
            {verification_text(review.verification_method.as_deref(), review.confirmations)
                .map(|t| view! { <p class="small trust"><Icon svg=icons::USERS />{t}</p> })}
            {review.revocation_reason.clone().filter(|_| is_revoked).map(|r| view! { <p class="small danger-text">{format!("Retirada: {r}")}</p> })}
            {move || match asking.get() {
                Asking::Nothing => view! {
                    <div class="two">
                        {can_verify.then(|| view! {
                            <button class="button ghost small" on:click=move |_| asking.set(Asking::Verify)>"Verificar"</button>
                        })}
                        <button class="button ghost small" on:click=move |_| asking.set(Asking::Moderate)>
                            {if is_revoked { "Restaurar" } else { "Retirar" }}
                        </button>
                    </div>
                }.into_any(),
                Asking::Verify => view! {
                    <div class="confirm-box" role="group" aria-label="Verificar la revisión">
                        <p class="small">"Verifícala solo si has comprobado tú la etiqueta actual. Queda registrado."</p>
                        <div class="two">
                            <button class="button ghost small" disabled=move || busy.get() on:click=move |_| asking.set(Asking::Nothing)>"Cancelar"</button>
                            <button class="button small" disabled=move || busy.get() on:click=verify>"Verificar"</button>
                        </div>
                    </div>
                }.into_any(),
                Asking::Moderate => view! {
                    <div class="confirm-box" role="group" aria-label="Moderar la revisión">
                        <label class="label" for=format!("reason-{}", id.get_value())>"Motivo (queda registrado)"</label>
                        <textarea id=format!("reason-{}", id.get_value()) class="input ocr-text admin-reason" maxlength="500"
                            prop:value=move || reason.get()
                            on:input=move |ev| reason.set(event_target_value(&ev))></textarea>
                        <div class="two">
                            <button class="button ghost small" disabled=move || busy.get() on:click=move |_| asking.set(Asking::Nothing)>"Cancelar"</button>
                            <button class="button small" disabled=move || busy.get() on:click=moderate>
                                {if is_revoked { "Restaurar" } else { "Retirar" }}
                            </button>
                        </div>
                    </div>
                }.into_any(),
            }}
            {move || error.get().map(|m| view! { <ErrorBox message=m /> })}
        </article>
    }
}

#[component]
fn LogPanel() -> impl IntoView {
    let items = RwSignal::new(Vec::<AuditEntry>::new());
    let cursor = RwSignal::new(None::<String>);
    let loading = RwSignal::new(false);
    let error = RwSignal::new(None::<String>);
    load_page(LOG.to_string(), false, items, cursor, loading, error);
    let more = move || {
        load_page(
            with_after(LOG, cursor.get_untracked()),
            true,
            items,
            cursor,
            loading,
            error,
        )
    };
    view! {
        {move || error.get().map(|m| view! { <ErrorBox message=m /> })}
        <ul class="list admin-list">
            {move || items.get().into_iter().map(|e| {
                let reason = e.details.get("reason").and_then(Value::as_str).map(str::to_string);
                let target = e.target_email.clone().unwrap_or_else(|| e.target_id.clone());
                view! {
                    <li>
                        <article class="admin-item" data-id=e.id.clone()>
                            <p class="row-title">{action_label(&e.action)}</p>
                            <p class="muted small">{format!("{} · {}", short_date(&e.created_at), e.actor_email.clone().unwrap_or_else(|| "Sistema".into()))}</p>
                            <p class="small">{format!("Sobre: {target}")}</p>
                            {reason.map(|r| view! { <p class="small">{format!("Motivo: {r}")}</p> })}
                        </article>
                    </li>
                }
            }).collect_view()}
        </ul>
        <Show when=move || loading.get()><Loading /></Show>
        <Show when=move || !loading.get() && items.with(Vec::is_empty) && error.with(Option::is_none)>
            <Empty title="Sin registros" text="Aún no hay acciones de administración." />
        </Show>
        <Show when=move || cursor.with(Option::is_some) && !loading.get()>
            <button class="button ghost small" on:click=move |_| more()>"Cargar más"</button>
        </Show>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn audit_actions_read_as_words() {
        assert_eq!(
            action_label("authorized_granted"),
            "Cuenta de confianza concedida"
        );
        assert_eq!(action_label("review_revoked"), "Revisión retirada");
        assert!(action_label("chain_review_rescoped").contains("chain_review_rescoped"));
    }

    #[test]
    fn a_trusted_review_is_never_called_a_certification() {
        let text = verification_text(Some("authorized"), 0).unwrap();
        assert!(text.contains("no es una certificación"));
        assert!(!text.to_lowercase().starts_with("certificad"));
        assert_eq!(
            verification_text(Some("manual"), 2).unwrap(),
            "Comprobada por 2 personas"
        );
        assert_eq!(verification_text(None, 0), None);
    }
}
