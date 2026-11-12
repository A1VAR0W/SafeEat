//! USDA FoodData Central (optional exact-barcode fallback).

use std::time::Duration;

use safeeat_contracts::{GlutenStatus, InformationSource, NutritionInput};
use safeeat_gluten_engine::{assess_gluten, GlutenInput, Origin};
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
        }))
    }
}

/// OFF first; USDA only when OFF fails or gives no gluten answer.
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
        if assess_gluten(&first.gluten).status != GlutenStatus::Unknown {
            return Ok(Some(first.clone()));
        }
    }
    match fdc.get_by_barcode(barcode).await {
        Ok(Some(other)) => {
            if first.is_some() && assess_gluten(&other.gluten).status == GlutenStatus::Unknown {
                return Ok(first);
            }
            Ok(Some(ExternalProduct {
                image_url: first.as_ref().and_then(|f| f.image_url.clone()),
                nutrition: first
                    .as_ref()
                    .map(|f| f.nutrition.clone())
                    .unwrap_or(other.nutrition.clone()),
                ..other
            }))
        }
        Ok(None) | Err(_) => Ok(first),
    }
}
