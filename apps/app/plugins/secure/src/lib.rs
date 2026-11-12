//! Secret storage for the SafeEat session.
//!
//! - Android: AES-256-GCM key in the Android Keystore, ciphertext in private preferences.
//! - iOS: Keychain item, this device only, readable while unlocked.
//! - Desktop (development only): nothing is stored; the session lasts while the app is open.
//!
//! The plugin registers no command, so the webview (JavaScript) can never read a
//! token: only the Rust shell reaches it through [`SecureExt`].

use serde::{Deserialize, Serialize};
use tauri::plugin::{Builder, TauriPlugin};
use tauri::{Manager, Runtime};

#[cfg(target_os = "ios")]
tauri::ios_plugin_binding!(init_plugin_safeeat_secure);

#[derive(Serialize)]
struct KeyArgs<'a> {
    key: &'a str,
}

#[derive(Serialize)]
struct SetArgs<'a> {
    key: &'a str,
    value: &'a str,
}

#[derive(Deserialize)]
#[cfg_attr(not(mobile), allow(dead_code))]
struct Got {
    value: Option<String>,
}

/// Handle to the native store.
pub struct Secure<R: Runtime> {
    #[cfg(mobile)]
    handle: tauri::plugin::PluginHandle<R>,
    #[cfg(not(mobile))]
    _marker: std::marker::PhantomData<fn() -> R>,
}

impl<R: Runtime> Secure<R> {
    /// The stored value, or `None` when there is none (or it can no longer be decrypted).
    pub fn get(&self, key: &str) -> Option<String> {
        #[cfg(mobile)]
        {
            self.handle
                .run_mobile_plugin::<Got>("get", KeyArgs { key })
                .ok()
                .and_then(|got| got.value)
        }
        #[cfg(not(mobile))]
        {
            let _ = (KeyArgs { key }, std::mem::size_of::<Got>());
            None
        }
    }

    /// Stores the value. Returns whether it was written.
    pub fn set(&self, key: &str, value: &str) -> bool {
        #[cfg(mobile)]
        {
            self.handle
                .run_mobile_plugin::<serde::de::IgnoredAny>("set", SetArgs { key, value })
                .is_ok()
        }
        #[cfg(not(mobile))]
        {
            let _ = SetArgs { key, value };
            false
        }
    }

    pub fn remove(&self, key: &str) {
        #[cfg(mobile)]
        {
            let _ = self
                .handle
                .run_mobile_plugin::<serde::de::IgnoredAny>("remove", KeyArgs { key });
        }
        #[cfg(not(mobile))]
        {
            let _ = key;
        }
    }
}

pub trait SecureExt<R: Runtime> {
    fn secure(&self) -> &Secure<R>;
}

impl<R: Runtime, T: Manager<R>> SecureExt<R> for T {
    fn secure(&self) -> &Secure<R> {
        self.state::<Secure<R>>().inner()
    }
}

pub fn init<R: Runtime>() -> TauriPlugin<R> {
    Builder::new("safeeat-secure")
        .setup(|app, _api| {
            #[cfg(target_os = "android")]
            let handle =
                _api.register_android_plugin("com.a1var0w.safeeat.secure", "SecurePlugin")?;
            #[cfg(target_os = "ios")]
            let handle = _api.register_ios_plugin(init_plugin_safeeat_secure)?;
            #[cfg(mobile)]
            app.manage(Secure { handle });
            #[cfg(not(mobile))]
            app.manage(Secure::<R> {
                _marker: std::marker::PhantomData,
            });
            Ok(())
        })
        .build()
}
