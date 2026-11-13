pub mod food_data_central;
pub mod http;
pub mod open_food_facts;
pub mod openstreetmap;

use safeeat_contracts::{Certification, InformationSource, NutritionInput, Reason};
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
    /// Other texts of the same record's list ("en francés", text): the original when
    /// `ingredients` is a Spanish version, English... Analysed too; they can only add
    /// caution (a poor translation never replaces the original).
    pub alternate_ingredients: Vec<(String, String)>,
    /// Sold in an EU country, so labelled under Regulation (EU) 1169/2011.
    pub eu_labelling: bool,
    /// Entered by the manufacturer (Open Food Facts producers platform), not the community.
    pub manufacturer_data: bool,
    /// Unix time of the record's last change, when the provider gives it.
    pub last_modified: Option<i64>,
    /// Cautions found while choosing between sources (another formulation, a source that
    /// disagrees); appended to the assessment's reasons.
    pub provider_reasons: Vec<Reason>,
}

impl ExternalProduct {
    pub fn context(&self) -> safeeat_gluten_engine::Context {
        safeeat_gluten_engine::Context {
            eu_labelling: self.eu_labelling,
        }
    }
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
