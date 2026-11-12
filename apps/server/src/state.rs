use std::collections::HashMap;
use std::sync::Arc;

use jsonwebtoken::{DecodingKey, EncodingKey};
use sqlx::PgPool;
use tokio::sync::Mutex;

use crate::ai_check::AiChecker;
use crate::config::Config;
use crate::providers::ai::LabelAi;
use crate::providers::food_data_central::FoodDataCentral;
use crate::providers::open_food_facts::OpenFoodFacts;
use crate::providers::openstreetmap::OpenStreetMap;
use crate::ratelimit::RateLimiter;

pub type AppState = Arc<Inner>;

pub struct Inner {
    pub config: Config,
    pub db: PgPool,
    pub jwt_encoding: EncodingKey,
    pub jwt_decoding: DecodingKey,
    pub limiter: RateLimiter,
    pub off: OpenFoodFacts,
    pub fdc: Option<FoodDataCentral>,
    pub osm: OpenStreetMap,
    pub ai: Option<LabelAi>,
    /// On-server model for "Verificado por IA" (background queue).
    pub ai_check: Option<AiChecker>,
    /// Per-barcode locks so concurrent lookups share one provider request.
    pub barcode_locks: Mutex<HashMap<String, Arc<Mutex<()>>>>,
    pub version: &'static str,
}

impl Inner {
    pub fn new(config: Config, db: PgPool) -> anyhow::Result<AppState> {
        let http = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .user_agent(config.openfoodfacts_user_agent.clone())
            .gzip(true)
            .build()?;
        Ok(Arc::new(Inner {
            jwt_encoding: EncodingKey::from_secret(config.jwt_access_secret.as_bytes()),
            jwt_decoding: DecodingKey::from_secret(config.jwt_access_secret.as_bytes()),
            limiter: RateLimiter::default(),
            off: OpenFoodFacts::new(
                http.clone(),
                config.openfoodfacts_base_url.clone(),
                config.openfoodfacts_search_url.clone(),
            ),
            fdc: config
                .usda_fdc_api_key
                .clone()
                .map(|key| FoodDataCentral::new(http.clone(), key)),
            osm: OpenStreetMap::new(http.clone()),
            ai: config
                .anthropic_api_key
                .clone()
                .map(|key| LabelAi::new(http.clone(), key, config.anthropic_model.clone())),
            ai_check: config
                .ai_local_url
                .clone()
                .map(|url| AiChecker::new(http.clone(), url, config.ai_local_model.clone())),
            barcode_locks: Mutex::new(HashMap::new()),
            version: env!("CARGO_PKG_VERSION"),
            config,
            db,
        }))
    }
}
