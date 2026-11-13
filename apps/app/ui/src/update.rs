//! New versions of the Android app, from inside the app.
//!
//! - On start (after a short pause) the shell checks the update manifest; a newer
//!   version opens a sheet: «Actualizar» / «Ahora no».
//! - «Buscar actualizaciones» in Tu cuenta and on the sign-in screen checks by hand.
//! - Download with progress → Android installer. The first time Android asks to allow
//!   installs from SafeEat: the sheet explains it and goes on when she comes back.
//!
//! iPhone and desktop: the shell answers `supported: false` and nothing is shown
//! (SideStore updates the iPhone app).

use leptos::prelude::*;
use leptos::task::spawn_local;
use serde::{Deserialize, Serialize};

use crate::api;
use crate::icons;
use crate::ui::Icon;

#[derive(Serialize)]
struct NoArgs {}

#[derive(Clone, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct Check {
    #[serde(default)]
    supported: bool,
    #[serde(default)]
    installed: String,
    #[serde(default)]
    available: bool,
    #[serde(default)]
    version: String,
    #[serde(default)]
    notes: String,
    #[serde(default)]
    size: f64,
}

#[derive(Clone, Deserialize)]
struct Progress {
    state: String,
    received: f64,
    total: f64,
    message: Option<String>,
}

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Installed {
    opened: bool,
    needs_permission: bool,
}

#[derive(Clone, PartialEq)]
enum Phase {
    Offer,
    Downloading(f64),
    NeedsPermission,
    /// The Android installer was opened; «Instalar» opens it again.
    Installing,
    Failed(String),
}

/// Shared by the sheet and the «Buscar actualizaciones» cards.
#[derive(Clone, Copy)]
pub struct Updates {
    check: RwSignal<Option<Check>>,
    phase: RwSignal<Phase>,
    open: RwSignal<bool>,
    checking: RwSignal<bool>,
    /// Result of a manual check, shown next to its button.
    note: RwSignal<Option<String>>,
}

impl Updates {
    fn run_check(self, manual: bool) {
        if self.checking.get_untracked() {
            return;
        }
        self.checking.set(true);
        if manual {
            self.note.set(None);
        }
        spawn_local(async move {
            let result = api::invoke::<Check>("update_check", NoArgs {}).await;
            self.checking.set(false);
            match result {
                Ok(c) => {
                    let available = c.available;
                    let installed = c.installed.clone();
                    self.check.set(Some(c));
                    if available {
                        // A download already under way keeps its progress.
                        if !self.open.get_untracked() {
                            self.phase.set(Phase::Offer);
                        }
                        self.open.set(true);
                    } else if manual {
                        self.note
                            .set(Some(format!("Tienes la última versión ({installed}).")));
                    }
                }
                Err(e) if manual => self.note.set(Some(e.message)),
                Err(_) => {}
            }
        });
    }

    fn download(self) {
        self.phase.set(Phase::Downloading(0.0));
        spawn_local(async move {
            if let Err(e) = api::invoke::<()>("update_download", NoArgs {}).await {
                self.phase.set(Phase::Failed(e.message));
                return;
            }
            self.watch();
        });
    }

    /// Follows the download until it is verified, fails or is cancelled.
    fn watch(self) {
        set_timeout(
            move || {
                spawn_local(async move {
                    let p = api::invoke::<Option<Progress>>("update_progress", NoArgs {})
                        .await
                        .ok()
                        .flatten();
                    let Some(p) = p else {
                        self.phase
                            .set(Phase::Failed("No se ha podido seguir la descarga.".into()));
                        return;
                    };
                    match p.state.as_str() {
                        "ready" => self.install(),
                        "failed" => self.phase.set(Phase::Failed(
                            p.message
                                .unwrap_or_else(|| "No se ha podido descargar.".into()),
                        )),
                        "idle" => self.phase.set(Phase::Offer),
                        _ => {
                            let share = if p.total > 0.0 {
                                (p.received / p.total).clamp(0.0, 1.0)
                            } else {
                                0.0
                            };
                            if matches!(self.phase.get_untracked(), Phase::Downloading(_)) {
                                self.phase.set(Phase::Downloading(share));
                                self.watch();
                            }
                        }
                    }
                });
            },
            std::time::Duration::from_millis(400),
        );
    }

    fn install(self) {
        spawn_local(async move {
            match api::invoke::<Installed>("update_install", NoArgs {}).await {
                Ok(r) if r.needs_permission => self.phase.set(Phase::NeedsPermission),
                Ok(r) if r.opened => self.phase.set(Phase::Installing),
                Ok(_) => self
                    .phase
                    .set(Phase::Failed("No se ha podido abrir el instalador.".into())),
                Err(e) => self.phase.set(Phase::Failed(e.message)),
            }
        });
    }

    fn cancel(self) {
        spawn_local(async move {
            let _ = api::invoke::<()>("update_cancel", NoArgs {}).await;
        });
        self.phase.set(Phase::Offer);
    }
}

pub fn use_updates() -> Updates {
    expect_context::<Updates>()
}

/// Call once in the app root, before its view (the cards and the sheet share it).
pub fn provide_updates() {
    provide_context(Updates {
        check: RwSignal::new(None),
        phase: RwSignal::new(Phase::Offer),
        open: RwSignal::new(false),
        checking: RwSignal::new(false),
        note: RwSignal::new(None),
    });
}

/// Mount once, at the root: checks on start and shows the sheet.
#[component]
pub fn UpdateSheet() -> impl IntoView {
    let u = use_updates();
    // Not while the app is starting: the scanner and the session come first.
    set_timeout(
        move || u.run_check(false),
        std::time::Duration::from_secs(2),
    );
    // Back from Android's settings (permission) or from the installer.
    let handle = window_event_listener(leptos::ev::visibilitychange, move |_| {
        let visible = document().visibility_state() == web_sys::VisibilityState::Visible;
        if visible && u.open.get_untracked() && u.phase.get_untracked() == Phase::NeedsPermission {
            u.install();
        }
    });
    on_cleanup(move || handle.remove());

    let version = move || {
        u.check
            .with(|c| c.as_ref().map(|c| c.version.clone()).unwrap_or_default())
    };
    let body = move || {
        match u.phase.get() {
        Phase::Offer => {
            let (notes, size) = u.check.with(|c| {
                c.as_ref()
                    .map(|c| (c.notes.clone(), c.size))
                    .unwrap_or_default()
            });
            view! {
                {(!notes.is_empty()).then(|| view! { <p class="update-notes">{notes}</p> })}
                <p class="muted small">
                    {format!("Descarga de {:.0} MB. Tus datos y tu sesión se conservan.", (size / 1_048_576.0).max(1.0))}
                </p>
                <button class="button" on:click=move |_| u.download()>"Actualizar"</button>
                <button class="button ghost" on:click=move |_| u.open.set(false)>"Ahora no"</button>
            }
            .into_any()
        }
        Phase::Downloading(share) => view! {
            <p>"Descargando…"</p>
            <div class="update-bar" role="progressbar" aria-valuemin="0" aria-valuemax="100"
                aria-valuenow=format!("{:.0}", share * 100.0)>
                <span style=format!("width: {:.1}%", share * 100.0)></span>
            </div>
            <p class="muted small">{format!("{:.0} %", share * 100.0)}</p>
            <button class="button ghost" on:click=move |_| u.cancel()>"Cancelar"</button>
        }
        .into_any(),
        Phase::NeedsPermission => view! {
            <p><strong>"Falta un permiso (solo la primera vez)"</strong></p>
            <ol class="update-steps">
                <li>"Pulsa «Abrir ajustes»."</li>
                <li>"Activa «Permitir de esta fuente»."</li>
                <li>"Vuelve a SafeEat: la instalación sigue sola."</li>
            </ol>
            <button class="button" on:click=move |_| {
                spawn_local(async move { let _ = api::invoke::<()>("update_settings", NoArgs {}).await; });
            }>"Abrir ajustes"</button>
            <button class="button ghost" on:click=move |_| u.install()>"Ya lo he permitido"</button>
        }
        .into_any(),
        Phase::Installing => view! {
            <p>"Pulsa «Instalar» en la ventana de Android. SafeEat se cerrará y se abrirá la versión nueva."</p>
            <button class="button" on:click=move |_| u.install()>"Instalar"</button>
            <button class="button ghost" on:click=move |_| u.open.set(false)>"Más tarde"</button>
        }
        .into_any(),
        Phase::Failed(message) => view! {
            <div class="notice tone-caution">
                <Icon svg=icons::ALERT />
                <div class="notice-body"><p>{message}</p></div>
            </div>
            <button class="button" on:click=move |_| u.download()>"Reintentar"</button>
            <button class="button ghost" on:click=move |_| u.open.set(false)>"Ahora no"</button>
        }
        .into_any(),
    }
    };
    view! {
        <Show when=move || u.open.get()>
            <div class="update-backdrop">
                <section class="update-sheet" role="dialog" aria-modal="true" aria-labelledby="update-title">
                    <h2 id="update-title" class="update-title">
                        <span class="icon" inner_html=icons::SPARKLES></span>
                        {move || format!("SafeEat {} disponible", version())}
                    </h2>
                    {body}
                </section>
            </div>
        </Show>
    }
}

/// «Buscar actualizaciones» with the installed version (Android only).
#[component]
pub fn UpdateCard() -> impl IntoView {
    let u = use_updates();
    let supported = move || u.check.with(|c| c.as_ref().is_none_or(|c| c.supported));
    view! {
        <Show when=supported>
            <div class="card update-card">
                <div class="update-card-row">
                    <div>
                        <p class="row-title">"Versión de la app"</p>
                        <p class="muted small">"SafeEat " {env!("CARGO_PKG_VERSION")}</p>
                    </div>
                    <button class="button ghost small" disabled=move || u.checking.get()
                        on:click=move |_| u.run_check(true)>
                        <Icon svg=icons::DOWNLOAD />
                        {move || if u.checking.get() { "Buscando…" } else { "Buscar actualizaciones" }}
                    </button>
                </div>
                {move || u.note.get().map(|n| view! { <p class="muted small" role="status">{n}</p> })}
            </div>
        </Show>
    }
}
