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
}
