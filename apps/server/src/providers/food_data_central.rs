//! USDA FoodData Central (optional exact-barcode fallback).

use std::time::Duration;

use safeeat_contracts::{GlutenStatus, InformationSource, NutritionInput, Reason};
use safeeat_gluten_engine::{analyze, GlutenInput, Origin};
use serde_json::{json, Value};

use super::http::{provider_json, ProviderBudget};
use super::open_food_facts::OpenFoodFacts;
use super::{ExternalProduct, IngredientLanguage};
use crate::error::ApiError;

pub struct FoodDataCentral {
    http: reqwest::Client,
    key: String,
    budget: ProviderBudget,
}

fn digits(value: &str) -> &str {
    let trimmed = value.trim_start_matches('0');
    if trimmed.is_empty() {
        "0"
    } else {
        trimmed
    }
}

impl FoodDataCentral {
    pub fn new(http: reqwest::Client, key: String) -> Self {
        Self {
            http,
            key,
            budget: ProviderBudget::new(12),
        }
    }

    pub async fn get_by_barcode(&self, barcode: &str) -> Result<Option<ExternalProduct>, ApiError> {
        self.budget.take()?;
        let request = self
            .http
            .post("https://api.nal.usda.gov/fdc/v1/foods/search")
            .query(&[("api_key", &self.key)])
            .json(&json!({"query": barcode, "dataType": ["Branded"], "pageSize": 20}));
        let raw = provider_json(request, Duration::from_secs(8))
            .await?
            .unwrap_or(Value::Null);
        let foods = raw
            .get("foods")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let Some(food) = foods.iter().find(|f| {
            f.get("gtinUpc")
                .and_then(Value::as_str)
                .is_some_and(|g| digits(g) == digits(barcode))
        }) else {
            return Ok(None);
        };
        let id = food
            .get("fdcId")
            .and_then(Value::as_i64)
            .ok_or_else(ApiError::provider_unavailable)?;
        let description = food
            .get("description")
            .and_then(Value::as_str)
            .ok_or_else(ApiError::provider_unavailable)?;
        let str_field = |k: &str| {
            food.get(k)
                .and_then(Value::as_str)
                .filter(|v| !v.is_empty())
                .map(str::to_string)
        };
        let ingredients = str_field("ingredients");
        let source_url = format!("https://fdc.nal.usda.gov/food-details/{id}/nutrients");
        Ok(Some(ExternalProduct {
            summary_only: false,
            barcode: barcode.into(),
            name: description.into(),
            brand: str_field("brandName").or_else(|| str_field("brandOwner")),
            image_url: None,
            information_source: if ingredients.is_some() {
                InformationSource::Api
            } else {
                InformationSource::Unknown
            },
            gluten: GlutenInput {
                ingredients: ingredients.clone(),
                source: "USDA FoodData Central".into(),
                source_url: Some(source_url.clone()),
                origin: Some(Origin::Provider),
                ..Default::default()
            },
            ingredients,
            nutrition: NutritionInput::default(),
            nutri_score: None,
            source: "USDA FoodData Central · CC0".into(),
            source_url,
            certification: None,
            language: IngredientLanguage {
                code: Some("en".into()),
                ..Default::default()
            },
            alternate_ingredients: Vec::new(),
            // US labelling: barley and rye need not be declared as allergens.
            eu_labelling: false,
            manufacturer_data: false,
            last_modified: None,
            provider_reasons: Vec::new(),
        }))
    }
}

fn status(product: &ExternalProduct) -> GlutenStatus {
    analyze(&product.gluten, product.context())
        .assessment
        .status
}

fn words(text: Option<&str>, min: usize) -> Vec<String> {
    safeeat_gluten_engine::normalize(text.unwrap_or(""))
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|w| w.len() >= min)
        .map(str::to_string)
        .collect()
}

/// Same barcode is not enough to mix two records: brand or name must agree too when
/// both sources give them.
pub fn same_product(a: &ExternalProduct, b: &ExternalProduct) -> bool {
    let overlap = |x: Vec<String>, y: Vec<String>| x.iter().any(|w| y.contains(w));
    let (brand_a, brand_b) = (words(a.brand.as_deref(), 3), words(b.brand.as_deref(), 3));
    if !brand_a.is_empty() && !brand_b.is_empty() {
        return overlap(brand_a, brand_b);
    }
    let (name_a, name_b) = (words(Some(&a.name), 4), words(Some(&b.name), 4));
    name_a.is_empty() || name_b.is_empty() || overlap(name_a, name_b)
}

/// OFF first; USDA only when OFF fails or gives no gluten answer.
///
/// The records are never mixed: the result is one source's ingredient list. USDA data
/// follows US labelling (barley and rye need not be declared) and may be another
/// formulation, so it can add a warning but never a "sin gluten" over OFF's own list.
pub async fn enriched_barcode(
    off: &OpenFoodFacts,
    fdc: Option<&FoodDataCentral>,
    barcode: &str,
) -> Result<Option<ExternalProduct>, ApiError> {
    let Some(fdc) = fdc else {
        return off.get_by_barcode(barcode).await;
    };
    let first = match off.get_by_barcode(barcode).await {
        Ok(first) => first,
        Err(error) => return fdc.get_by_barcode(barcode).await.map_err(|_| error),
    };
    if let Some(first) = &first {
        if status(first) != GlutenStatus::Unknown {
            return Ok(Some(first.clone()));
        }
    }
    let other = match fdc.get_by_barcode(barcode).await {
        Ok(Some(other)) => other,
        Ok(None) | Err(_) => return Ok(first),
    };
    let Some(mut first) = first else {
        return Ok(Some(other));
    };
    if !same_product(&first, &other) {
        tracing::info!(
            barcode,
            "USDA record does not match OFF brand or name; ignored"
        );
        return Ok(Some(first));
    }
    match status(&other) {
        GlutenStatus::ContainsGluten | GlutenStatus::MayContainTraces => {
            let mut warned = ExternalProduct {
                image_url: first.image_url.clone(),
                nutrition: first.nutrition.clone(),
                ..other
            };
            warned.provider_reasons.push(Reason {
                code: "SOURCES_DISAGREE".into(),
                message: "USDA FoodData Central indica gluten o trazas y Open Food Facts no. Mostramos el aviso por precaución; puede ser otra formulación. Revisa la etiqueta.".into(),
            });
            Ok(Some(warned))
        }
        GlutenStatus::GlutenFree if first.ingredients.is_some() => {
            first.provider_reasons.push(Reason {
                code: "OTHER_SOURCE_CLAIM".into(),
                message: "USDA FoodData Central lo declara sin gluten, pero puede ser otra formulación (de otro país) y no se tiene en cuenta.".into(),
            });
            Ok(Some(first))
        }
        GlutenStatus::GlutenFree => Ok(Some(ExternalProduct {
            image_url: first.image_url.clone(),
            nutrition: first.nutrition.clone(),
            ..other
        })),
        GlutenStatus::Unknown => Ok(Some(first)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn product(brand: Option<&str>, name: &str) -> ExternalProduct {
        ExternalProduct {
            summary_only: false,
            barcode: "0".into(),
            name: name.into(),
            brand: brand.map(str::to_string),
            image_url: None,
            information_source: InformationSource::Api,
            ingredients: None,
            gluten: GlutenInput::default(),
            nutrition: NutritionInput::default(),
            nutri_score: None,
            source: String::new(),
            source_url: String::new(),
            certification: None,
            language: IngredientLanguage::default(),
            alternate_ingredients: vec![],
            eu_labelling: false,
            manufacturer_data: false,
            last_modified: None,
            provider_reasons: vec![],
        }
    }

    #[test]
    fn records_must_match_by_brand_or_name() {
        assert!(same_product(
            &product(Some("Kimbo"), "Patatas fritas"),
            &product(Some("KIMBO SNACKS"), "Potato chips")
        ));
        assert!(!same_product(
            &product(Some("Kimbo"), "Patatas fritas"),
            &product(Some("Acme"), "Patatas fritas")
        ));
        assert!(same_product(
            &product(None, "Galletas María"),
            &product(Some("X"), "MARIA COOKIES galletas")
        ));
        assert!(same_product(&product(None, "Producto"), &product(None, "")));
    }
}
