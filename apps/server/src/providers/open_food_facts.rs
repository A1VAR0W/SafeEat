//! Open Food Facts (port of apps/api/src/providers/open-food-facts.ts).

use std::time::Duration;

use safeeat_contracts::{Certification, InformationSource, NutritionInput};
use safeeat_gluten_engine::{GlutenInput, Origin};
use serde_json::{Map, Value};

use super::http::{provider_json, ProviderBudget, TtlCache};
use super::ExternalProduct;
use crate::error::ApiError;
use crate::util::safe_http_url;

const FIELDS: &str = "code,product_name,product_name_es,brands,ingredients_text,ingredients_text_es,ingredients_text_en,allergens_tags,traces_tags,labels_tags,image_front_small_url,image_front_thumb_url,nutriments,nutrition_grades";
const SEARCH_FIELDS: &str = "code,product_name,product_name_es,brands,image_front_thumb_url,image_front_small_url,allergens_tags,traces_tags,labels_tags";

/// OFF label tags. `en:crossed-grain-trademark` is the Espiga Barrada / AOECS symbol.
const SEAL_TAGS: &[&str] = &[
    "en:crossed-grain-trademark",
    "en:crossed-grain-symbol",
    "es:espiga-barrada",
];
const CLAIM_TAGS: &[&str] = &["en:gluten-free", "en:no-gluten", "es:sin-gluten"];

pub struct OpenFoodFacts {
    http: reqwest::Client,
    base_url: String,
    search_url: String,
    reads: ProviderBudget,
    searches: ProviderBudget,
    search_cache: TtlCache<Vec<ExternalProduct>>,
}

fn text<'a>(p: &'a Map<String, Value>, key: &str) -> Option<&'a str> {
    p.get(key).and_then(Value::as_str)
}

/// JavaScript `a || b`: the first non-empty string.
fn first<'a>(values: &[Option<&'a str>]) -> Option<&'a str> {
    values.iter().flatten().copied().find(|v| !v.is_empty())
}

fn tags(p: &Map<String, Value>, key: &str) -> Option<Vec<String>> {
    p.get(key)?.as_array().map(|a| {
        a.iter()
            .filter_map(|v| v.as_str().map(str::to_string))
            .collect()
    })
}

pub fn certification(labels: &[String]) -> Option<Certification> {
    if labels.iter().any(|t| SEAL_TAGS.contains(&t.as_str())) {
        Some(Certification::CertifiedSeal)
    } else if labels.iter().any(|t| CLAIM_TAGS.contains(&t.as_str())) {
        Some(Certification::GlutenFreeClaim)
    } else {
        None
    }
}

pub fn normalize_off(value: &Value) -> Option<ExternalProduct> {
    let p = value.as_object()?;
    let code = match p.get("code")? {
        Value::String(s) => s.clone(),
        Value::Number(n) => n.to_string(),
        _ => return None,
    };
    if !(8..=14).contains(&code.len()) || !code.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let source_url = format!("https://world.openfoodfacts.org/product/{code}");
    let ingredients = first(&[
        text(p, "ingredients_text_es"),
        text(p, "ingredients_text"),
        text(p, "ingredients_text_en"),
    ])
    .map(str::to_string);
    let allergens = tags(p, "allergens_tags");
    let traces = tags(p, "traces_tags");
    let labels = tags(p, "labels_tags");
    let nutriments = p.get("nutriments").and_then(Value::as_object);
    let number = |key: &str, max: f64| {
        nutriments
            .and_then(|n| n.get(key))
            .and_then(Value::as_f64)
            .filter(|v| v.is_finite() && *v >= 0.0 && *v <= max)
    };
    let grade = text(p, "nutrition_grades").map(str::to_uppercase);
    let non_empty = |v: &Option<Vec<String>>| v.as_ref().is_some_and(|v| !v.is_empty());
    let brand = match p.get("brands") {
        Some(Value::String(s)) => Some(s.clone()),
        Some(Value::Array(a)) => Some(
            a.iter()
                .filter_map(Value::as_str)
                .collect::<Vec<_>>()
                .join(", "),
        ),
        _ => None,
    };
    let labels_list = labels.clone().unwrap_or_default();
    Some(ExternalProduct {
        summary_only: false,
        name: first(&[text(p, "product_name_es"), text(p, "product_name")])
            .map(str::to_string)
            .unwrap_or_else(|| format!("Producto {code}")),
        brand,
        image_url: safe_http_url(first(&[
            text(p, "image_front_small_url"),
            text(p, "image_front_thumb_url"),
        ])),
        information_source: if ingredients.is_some()
            || non_empty(&allergens)
            || non_empty(&traces)
            || non_empty(&labels)
        {
            InformationSource::Api
        } else {
            InformationSource::Unknown
        },
        gluten: GlutenInput {
            ingredients: ingredients.clone(),
            allergens: allergens.map(|a| a.join(", ")),
            traces: traces.map(|t| t.join(", ")),
            declaration: labels_list
                .iter()
                .any(|t| CLAIM_TAGS.contains(&t.as_str()) || SEAL_TAGS.contains(&t.as_str()))
                .then(|| "sin gluten".to_string()),
            source: "Open Food Facts (datos comunitarios)".into(),
            source_url: Some(source_url.clone()),
            origin: Some(Origin::Provider),
            reviewed: None,
        },
        ingredients,
        nutrition: NutritionInput {
            energy_kcal: number("energy-kcal_100g", 900.0),
            sugars: number("sugars_100g", 100.0),
            saturated_fat: number("saturated-fat_100g", 100.0),
            salt: number("salt_100g", 100.0)
                .or_else(|| number("sodium_100g", 40.0).map(|s| s * 2.5)),
            fibre: number("fiber_100g", 100.0),
            protein: number("proteins_100g", 100.0),
        },
        nutri_score: grade.filter(|g| matches!(g.as_str(), "A" | "B" | "C" | "D" | "E")),
        source: "Open Food Facts · ODbL".into(),
        source_url,
        certification: certification(&labels_list),
        barcode: code,
    })
}

pub fn normalize_search_hit(value: &Value) -> Option<ExternalProduct> {
    let mut hit = value.as_object()?.clone();
    // Search results prefer the thumbnail.
    let thumb = hit
        .get("image_front_thumb_url")
        .and_then(Value::as_str)
        .filter(|v| !v.is_empty())
        .map(str::to_string);
    if let Some(thumb) = thumb {
        hit.insert("image_front_small_url".into(), Value::String(thumb));
    }
    let mut item = normalize_off(&Value::Object(hit))?;
    item.summary_only = true;
    item.source = "Open Food Facts · índice de búsqueda · ODbL".into();
    Some(item)
}

impl OpenFoodFacts {
    pub fn new(http: reqwest::Client, base_url: String, search_url: String) -> Self {
        Self {
            http,
            base_url: base_url.trim_end_matches('/').into(),
            search_url: search_url.trim_end_matches('/').into(),
            reads: ProviderBudget::new(14),
            searches: ProviderBudget::new(8),
            search_cache: TtlCache::new(Duration::from_secs(300), 100),
        }
    }

    pub async fn get_by_barcode(&self, barcode: &str) -> Result<Option<ExternalProduct>, ApiError> {
        self.reads.take()?;
        let url = format!("{}/api/v3/product/{barcode}?fields={FIELDS}", self.base_url);
        let Some(raw) = provider_json(self.http.get(url), Duration::from_secs(8)).await? else {
            return Ok(None);
        };
        let invalid = || {
            ApiError::new(
                503,
                "PROVIDER_INVALID",
                "La fuente ha enviado información no válida.",
            )
        };
        let envelope = raw.as_object().ok_or_else(invalid)?;
        match envelope.get("status") {
            Some(Value::Number(n)) if n.as_i64() == Some(0) => return Ok(None),
            Some(Value::String(s)) if s == "failure" => return Ok(None),
            Some(Value::Number(_)) | Some(Value::String(_)) | None => {}
            Some(_) => return Err(invalid()),
        }
        let product = envelope.get("product").unwrap_or(&Value::Null);
        normalize_off(product).map(Some).ok_or_else(|| {
            ApiError::new(
                503,
                "PROVIDER_INVALID",
                "La fuente ha enviado información incompleta.",
            )
        })
    }

    pub async fn search(&self, query: &str) -> Result<Vec<ExternalProduct>, ApiError> {
        let key = query.to_lowercase();
        if let Some(cached) = self.search_cache.get(&key) {
            return Ok(cached);
        }
        self.searches.take()?;
        let request = self
            .http
            .get(format!("{}/search", self.search_url))
            .query(&[
                ("q", query),
                ("langs", "es,en"),
                ("page_size", "20"),
                ("fields", SEARCH_FIELDS),
            ]);
        let raw = provider_json(request, Duration::from_secs(8)).await?;
        let hits = raw
            .as_ref()
            .and_then(|r| r.get("hits"))
            .and_then(Value::as_array)
            .ok_or_else(|| {
                ApiError::new(
                    503,
                    "PROVIDER_INVALID",
                    "La fuente ha enviado información no válida.",
                )
            })?;
        let items: Vec<ExternalProduct> = hits.iter().filter_map(normalize_search_hit).collect();
        Ok(self.search_cache.put(key, items))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn normalizes_product() {
        let p = normalize_off(&json!({
            "code": "8410000000001",
            "product_name": "Galletas",
            "product_name_es": "",
            "ingredients_text_es": "Harina de arroz, azúcar",
            "allergens_tags": [],
            "labels_tags": ["en:no-gluten", "en:crossed-grain-trademark"],
            "nutriments": {"sodium_100g": 0.4, "energy-kcal_100g": 450},
            "nutrition_grades": "d"
        }))
        .unwrap();
        assert_eq!(p.name, "Galletas");
        assert_eq!(p.certification, Some(Certification::CertifiedSeal));
        assert_eq!(p.gluten.declaration.as_deref(), Some("sin gluten"));
        assert_eq!(p.nutrition.salt, Some(1.0));
        assert_eq!(p.nutri_score.as_deref(), Some("D"));
        assert_eq!(p.information_source, InformationSource::Api);
    }

    #[test]
    fn rejects_bad_codes() {
        assert!(normalize_off(&json!({"code": "12"})).is_none());
        assert_eq!(
            normalize_off(&json!({"code": 12345678})).unwrap().name,
            "Producto 12345678"
        );
    }

    #[test]
    fn search_hits_join_brands() {
        let p = normalize_search_hit(&json!({"code": "12345678", "brands": ["A", "B"]})).unwrap();
        assert!(p.summary_only);
        assert_eq!(p.brand.as_deref(), Some("A, B"));
    }
}
