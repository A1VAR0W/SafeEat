pub mod ai;
pub mod food_data_central;
pub mod http;
pub mod open_food_facts;
pub mod openstreetmap;

use safeeat_contracts::{Certification, InformationSource, NutritionInput};
use safeeat_gluten_engine::GlutenInput;

/// A product as returned by an external provider, before assessment.
#[derive(Debug, Clone)]
pub struct ExternalProduct {
    /// Search-index summaries are previews, never current safety evidence.
    pub summary_only: bool,
    pub barcode: String,
    pub name: String,
    pub brand: Option<String>,
    pub image_url: Option<String>,
    pub information_source: InformationSource,
    pub ingredients: Option<String>,
    pub gluten: GlutenInput,
    pub nutrition: NutritionInput,
    pub nutri_score: Option<String>,
    pub source: String,
    pub source_url: String,
    pub certification: Option<Certification>,
    /// Language and structure of the ingredient list, for the Spanish view and the AI gate.
    pub language: IngredientLanguage,
}

/// One parsed ingredient from Open Food Facts (taxonomy id + original text).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ParsedIngredient {
    pub id: String,
    pub text: String,
    pub percent: Option<f64>,
    pub children: Vec<ParsedIngredient>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct IngredientLanguage {
    /// Language of `ExternalProduct::ingredients` ("es" when OFF had a Spanish text).
    pub code: Option<String>,
    /// OFF's parse of that text; empty for other providers.
    pub parsed: Vec<ParsedIngredient>,
    /// Ingredients OFF could not match to its taxonomy (None when unknown).
    pub unknown: Option<u64>,
    /// OFF trace tags ("en:nuts"), shown translated after the list.
    pub trace_tags: Vec<String>,
}
