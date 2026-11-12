//! System share sheet for SafeEat links.
//!
//! - Android: `Intent.ACTION_SEND` through the system chooser.
//! - iOS: `UIActivityViewController`.
//! - Desktop (development only): not available; the app shows the link instead.

use serde::Serialize;
use tauri::plugin::{Builder, TauriPlugin};
use tauri::{Manager, Runtime};

#[cfg(target_os = "ios")]
tauri::ios_plugin_binding!(init_plugin_safeeat_share);

#[derive(Serialize)]
struct ShareArgs<'a> {
    text: &'a str,
    title: &'a str,
}

pub struct Share<R: Runtime> {
    #[cfg(mobile)]
    handle: tauri::plugin::PluginHandle<R>,
    #[cfg(not(mobile))]
    _marker: std::marker::PhantomData<fn() -> R>,
}

impl<R: Runtime> Share<R> {
    /// Opens the share sheet with `text`. Returns whether it could be opened.
    pub fn text(&self, text: &str, title: &str) -> bool {
        #[cfg(mobile)]
        {
            self.handle
                .run_mobile_plugin::<serde::de::IgnoredAny>("share", ShareArgs { text, title })
                .is_ok()
        }
        #[cfg(not(mobile))]
        {
            let _ = ShareArgs { text, title };
            false
        }
    }
}

pub trait ShareExt<R: Runtime> {
    fn share(&self) -> &Share<R>;
}

impl<R: Runtime, T: Manager<R>> ShareExt<R> for T {
    fn share(&self) -> &Share<R> {
        self.state::<Share<R>>().inner()
    }
}

pub fn init<R: Runtime>() -> TauriPlugin<R> {
    Builder::new("safeeat-share")
        .setup(|app, _api| {
            #[cfg(target_os = "android")]
            let handle =
                _api.register_android_plugin("com.a1var0w.safeeat.share", "SharePlugin")?;
            #[cfg(target_os = "ios")]
            let handle = _api.register_ios_plugin(init_plugin_safeeat_share)?;
            #[cfg(mobile)]
            app.manage(Share { handle });
            #[cfg(not(mobile))]
            app.manage(Share::<R> {
                _marker: std::marker::PhantomData,
            });
            Ok(())
        })
        .build()
}
