//! In-app updates for the SafeEat APK (Android only).
//!
//! The Kotlin side (`UpdaterPlugin.kt`, a port of the 1.x updater) downloads from the
//! fixed SafeEat host only and checks size, SHA-256, package, version code and the
//! signing certificate before it opens the Android installer. The plugin registers no
//! command: only the Rust shell reaches it (`update_*` commands in src-tauri), with the
//! data of the manifest it fetched itself.
//!
//! iPhone and desktop: not available (SideStore updates the iPhone app).

use serde::{Deserialize, Serialize};
use tauri::plugin::{Builder, TauriPlugin};
use tauri::{Manager, Runtime};

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Installed {
    pub version: String,
    pub version_code: i64,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Progress {
    /// idle | downloading | ready | failed
    pub state: String,
    pub received: i64,
    pub total: i64,
    pub message: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Install {
    pub opened: bool,
    pub needs_permission: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadArgs<'a> {
    pub url: &'a str,
    pub sha256: &'a str,
    pub size: i64,
    pub version_code: i64,
}

pub struct Updater<R: Runtime> {
    #[cfg(target_os = "android")]
    handle: tauri::plugin::PluginHandle<R>,
    #[cfg(not(target_os = "android"))]
    _marker: std::marker::PhantomData<fn() -> R>,
}

impl<R: Runtime> Updater<R> {
    /// Whether this build can update itself (Android).
    pub fn available(&self) -> bool {
        cfg!(target_os = "android")
    }

    /// The installed version; `None` where the updater does not exist.
    pub fn installed(&self) -> Option<Installed> {
        #[cfg(target_os = "android")]
        {
            self.handle.run_mobile_plugin("installed", ()).ok()
        }
        #[cfg(not(target_os = "android"))]
        {
            None
        }
    }

    /// Starts the download in the background; follow it with [`Updater::progress`].
    pub fn download(&self, args: DownloadArgs<'_>) -> Result<(), String> {
        #[cfg(target_os = "android")]
        {
            self.handle
                .run_mobile_plugin::<serde::de::IgnoredAny>("download", args)
                .map(|_| ())
                .map_err(|e| e.to_string())
        }
        #[cfg(not(target_os = "android"))]
        {
            let _ = args;
            Err("No disponible en este dispositivo.".into())
        }
    }

    pub fn progress(&self) -> Option<Progress> {
        #[cfg(target_os = "android")]
        {
            self.handle.run_mobile_plugin("progress", ()).ok()
        }
        #[cfg(not(target_os = "android"))]
        {
            None
        }
    }

    pub fn cancel(&self) {
        #[cfg(target_os = "android")]
        {
            let _ = self
                .handle
                .run_mobile_plugin::<serde::de::IgnoredAny>("cancel", ());
        }
    }

    /// Opens the Android installer for the verified download.
    pub fn install(&self) -> Result<Install, String> {
        #[cfg(target_os = "android")]
        {
            self.handle
                .run_mobile_plugin("install", ())
                .map_err(|e| e.to_string())
        }
        #[cfg(not(target_os = "android"))]
        {
            Err("No disponible en este dispositivo.".into())
        }
    }

    /// Android's «Instalar aplicaciones desconocidas» page for SafeEat.
    pub fn open_settings(&self) {
        #[cfg(target_os = "android")]
        {
            let _ = self
                .handle
                .run_mobile_plugin::<serde::de::IgnoredAny>("openSettings", ());
        }
    }
}

pub trait UpdaterExt<R: Runtime> {
    fn updater(&self) -> &Updater<R>;
}

impl<R: Runtime, T: Manager<R>> UpdaterExt<R> for T {
    fn updater(&self) -> &Updater<R> {
        self.state::<Updater<R>>().inner()
    }
}

pub fn init<R: Runtime>() -> TauriPlugin<R> {
    Builder::new("safeeat-updater")
        .setup(|app, _api| {
            #[cfg(target_os = "android")]
            {
                let handle =
                    _api.register_android_plugin("com.a1var0w.safeeat.updater", "UpdaterPlugin")?;
                app.manage(Updater { handle });
            }
            #[cfg(not(target_os = "android"))]
            app.manage(Updater::<R> {
                _marker: std::marker::PhantomData,
            });
            Ok(())
        })
        .build()
}
