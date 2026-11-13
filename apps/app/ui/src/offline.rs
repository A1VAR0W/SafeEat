//! Offline copy of «Guardados» (favourites, her verdicts, bookmarks), for the
//! supermarket without coverage. Kept in the app's own storage on this phone, one
//! copy per account, and deleted on logout, session expiry and account deletion.
//!
//! The copy is always shown as a copy: with the date it was saved and a warning
//! that the analysis may have changed since. It is never used while online.

use safeeat_contracts::Product;
use serde::{Deserialize, Serialize};

const PREFIX: &str = "safeeat.offline.v1.";
/// Enough for her saved lists; keeps the storage small.
const MAX_ITEMS: usize = 150;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Copy {
    /// Milliseconds since 1970, when it was saved.
    pub saved_at: f64,
    pub items: Vec<Product>,
}

/// Storage key of one list of one account. Ids are uuids or short slugs: anything
/// else is dropped so a key can never collide with another account's.
pub fn key(user_id: &str, list: &str) -> String {
    let clean = |s: &str| -> String {
        s.chars()
            .filter(|c| c.is_ascii_alphanumeric() || *c == '-')
            .collect()
    };
    format!("{PREFIX}{}.{}", clean(user_id), clean(list))
}

/// The product among the copies, by id or barcode.
pub fn find_in<'a>(copies: impl IntoIterator<Item = &'a Copy>, id: &str) -> Option<(f64, Product)> {
    copies.into_iter().find_map(|c| {
        c.items
            .iter()
            .find(|p| p.id == id || p.barcode.as_deref() == Some(id))
            .map(|p| (c.saved_at, p.clone()))
    })
}

fn storage() -> Option<web_sys::Storage> {
    web_sys::window()?.local_storage().ok().flatten()
}

fn keys() -> Vec<String> {
    let Some(s) = storage() else {
        return Vec::new();
    };
    let n = s.length().unwrap_or(0);
    (0..n)
        .filter_map(|i| s.key(i).ok().flatten())
        .filter(|k| k.starts_with(PREFIX))
        .collect()
}

pub fn save(user_id: &str, list: &str, items: &[Product]) {
    let copy = Copy {
        saved_at: js_sys::Date::now(),
        items: items.iter().take(MAX_ITEMS).cloned().collect(),
    };
    if let (Some(s), Ok(raw)) = (storage(), serde_json::to_string(&copy)) {
        // A full storage only loses the copy.
        let _ = s.set_item(&key(user_id, list), &raw);
    }
}

pub fn load(user_id: &str, list: &str) -> Option<Copy> {
    let raw = storage()?.get_item(&key(user_id, list)).ok().flatten()?;
    serde_json::from_str(&raw).ok()
}

/// A saved product of this account, when the server cannot be reached.
pub fn find(user_id: &str, id: &str) -> Option<(f64, Product)> {
    let mine = key(user_id, "");
    let copies: Vec<Copy> = keys()
        .into_iter()
        .filter(|k| k.starts_with(&mine))
        .filter_map(|k| storage()?.get_item(&k).ok().flatten())
        .filter_map(|raw| serde_json::from_str(&raw).ok())
        .collect();
    find_in(&copies, id)
}

/// Deletes every account's copy (logout, expired session, deleted account).
pub fn clear() {
    if let Some(s) = storage() {
        for k in keys() {
            let _ = s.remove_item(&k);
        }
    }
}

/// «13/11/2026, 18:05», in the phone's time.
pub fn saved_on(ms: f64) -> String {
    let date = js_sys::Date::new(&wasm_bindgen::JsValue::from_f64(ms));
    String::from(date.to_locale_string("es-ES", &wasm_bindgen::JsValue::UNDEFINED))
}

pub fn notice(ms: f64) -> String {
    format!(
        "Sin conexión. Es la copia guardada en este móvil el {}. El análisis puede haber cambiado desde entonces: comprueba la etiqueta.",
        saved_on(ms)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_are_per_account_and_cannot_collide() {
        assert_eq!(
            key("abc-1", "favorites"),
            "safeeat.offline.v1.abc-1.favorites"
        );
        // A crafted id cannot reach into another account's prefix.
        assert_eq!(key("a.b", "x"), key("ab", "x"));
        assert!(key("u1", "collection-9").starts_with(&key("u1", "")));
        assert!(!key("u10", "favorites").starts_with(&key("u1", "")));
    }
}
