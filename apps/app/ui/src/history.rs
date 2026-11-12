//! Recent products and searches, kept only on this device (webview storage of the app).
//!
//! Only what is needed to find a product again is stored: id, name, brand and image.
//! Never its gluten status (it could be stale: the list always opens the current
//! analysis), never her personal verdict, and never products she created from her own
//! label (those are private to her account).

use safeeat_contracts::Product;
use serde::{de::DeserializeOwned, Deserialize, Serialize};

const PRODUCTS_KEY: &str = "safeeat.recent-products.v1";
const SEARCHES_KEY: &str = "safeeat.recent-searches.v1";
const MAX_PRODUCTS: usize = 30;
const MAX_SEARCHES: usize = 8;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RecentProduct {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub brand: Option<String>,
    #[serde(default)]
    pub image_url: Option<String>,
}

fn storage() -> Option<web_sys::Storage> {
    web_sys::window()?.local_storage().ok().flatten()
}

fn read<T: DeserializeOwned>(key: &str) -> Vec<T> {
    storage()
        .and_then(|s| s.get_item(key).ok().flatten())
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_default()
}

fn write<T: Serialize>(key: &str, items: &[T]) {
    if let (Some(s), Ok(raw)) = (storage(), serde_json::to_string(items)) {
        // Full or blocked storage only loses the history, never breaks the screen.
        let _ = s.set_item(key, &raw);
    }
}

/// Moves `item` to the front, without duplicates, keeping at most `max`.
fn push_front<T: Clone>(list: &mut Vec<T>, item: T, max: usize, same: impl Fn(&T) -> bool) {
    list.retain(|x| !same(x));
    list.insert(0, item);
    list.truncate(max);
}

pub fn remember_product(product: &Product) {
    if product.user_created {
        return;
    }
    let item = RecentProduct {
        id: product.id.clone(),
        name: product.name.clone(),
        brand: product.brand.clone(),
        image_url: product.image_url.clone(),
    };
    let mut list: Vec<RecentProduct> = read(PRODUCTS_KEY);
    push_front(&mut list, item, MAX_PRODUCTS, |x| x.id == product.id);
    write(PRODUCTS_KEY, &list);
}

pub fn recent_products() -> Vec<RecentProduct> {
    read(PRODUCTS_KEY)
}

pub fn remember_search(query: &str) {
    let clean: String = query.trim().chars().take(100).collect();
    if clean.chars().count() < 2 {
        return;
    }
    let lower = clean.to_lowercase();
    let mut list: Vec<String> = read(SEARCHES_KEY);
    push_front(&mut list, clean, MAX_SEARCHES, |x| {
        x.to_lowercase() == lower
    });
    write(SEARCHES_KEY, &list);
}

pub fn recent_searches() -> Vec<String> {
    read(SEARCHES_KEY)
}

pub fn clear() {
    if let Some(s) = storage() {
        let _ = s.remove_item(PRODUCTS_KEY);
        let _ = s.remove_item(SEARCHES_KEY);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn newest_first_without_duplicates_and_bounded() {
        let mut list = vec![1, 2, 3];
        push_front(&mut list, 2, 3, |x| *x == 2);
        assert_eq!(list, vec![2, 1, 3]);
        push_front(&mut list, 9, 3, |x| *x == 9);
        assert_eq!(list, vec![9, 2, 1]);
    }

    #[test]
    fn a_recent_product_carries_no_gluten_status() {
        let raw = serde_json::to_string(&RecentProduct {
            id: "off-1".into(),
            name: "Pan".into(),
            brand: None,
            image_url: None,
        })
        .unwrap();
        assert!(!raw.contains("status") && !raw.contains("verdict"));
    }
}
