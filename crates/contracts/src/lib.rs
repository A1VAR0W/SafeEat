//! Shared JSON contract between the SafeEat server and app.
//!
//! The JSON shape intentionally matches the historical TypeScript/Zod contract
//! (`packages/contracts`) so that app 1.0.x keeps working against the Rust API:
//! optional values are omitted instead of serialized as `null`, while nullable
//! values are always present.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Hash)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum GlutenStatus {
    GlutenFree,
    MayContainTraces,
    ContainsGluten,
    Unknown,
}

impl GlutenStatus {
    /// Higher means riskier. Used to keep the most conservative of two assessments.
    pub fn risk(self) -> u8 {
        match self {
            GlutenStatus::ContainsGluten => 3,
            GlutenStatus::MayContainTraces => 2,
            GlutenStatus::GlutenFree => 1,
            GlutenStatus::Unknown => 0,
        }
    }
    pub fn label(self) -> (&'static str, &'static str) {
        match self {
            GlutenStatus::GlutenFree => ("✓", "SIN GLUTEN DECLARADO"),
            GlutenStatus::MayContainTraces => ("!", "PUEDE CONTENER TRAZAS"),
            GlutenStatus::ContainsGluten => ("×", "CONTIENE GLUTEN"),
            GlutenStatus::Unknown => ("⚠", "NO VERIFICADO"),
        }
    }
    pub fn as_str(self) -> &'static str {
        match self {
            GlutenStatus::GlutenFree => "GLUTEN_FREE",
            GlutenStatus::MayContainTraces => "MAY_CONTAIN_TRACES",
            GlutenStatus::ContainsGluten => "CONTAINS_GLUTEN",
            GlutenStatus::Unknown => "UNKNOWN",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EvidenceKind {
    Claim,
    Ingredient,
    Allergen,
    Trace,
    Uncertainty,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Evidence {
    pub source: String,
    pub text: String,
    pub kind: EvidenceKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Reason {
    pub code: String,
    pub message: String,
}

/// Certification evidence that is shown separately from the ingredient analysis.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Certification {
    /// Crossed grain symbol (Espiga Barrada / AOECS) on the package.
    CertifiedSeal,
    /// A "gluten free" label claim without a certification symbol.
    GlutenFreeClaim,
}

impl Certification {
    pub fn label(self) -> &'static str {
        match self {
            Certification::CertifiedSeal => "Sello Espiga Barrada",
            Certification::GlutenFreeClaim => "Declarado «sin gluten» en el envase",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Assessment {
    pub status: GlutenStatus,
    pub confidence: f64,
    pub reasons: Vec<Reason>,
    pub evidence: Vec<Evidence>,
    pub engine_version: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub certification: Option<Certification>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NutritionInput {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub energy_kcal: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub saturated_fat: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sugars: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub salt: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fibre: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub protein: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BreakdownItem {
    pub label: String,
    pub points: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NutritionScore {
    pub score: Option<i64>,
    pub breakdown: Vec<BreakdownItem>,
    pub explanation: String,
    pub version: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum InformationSource {
    Api,
    Community,
    #[default]
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ProductReportCounts {
    pub gluten_free: i64,
    pub contains_gluten: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum VerdictValue {
    Safe,
    Unsafe,
}

/// A personal verdict ("safe for me / not safe for me"). Shown above everything
/// else for its owner, but it never changes the automatic assessment.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PersonalVerdict {
    pub verdict: VerdictValue,
    pub note: String,
    pub updated_at: String,
    /// True when the product's ingredients changed after the verdict was saved.
    pub ingredients_changed: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Product {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub barcode: Option<String>,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub brand: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ingredients_image_url: Option<String>,
    #[serde(default)]
    pub information_source: InformationSource,
    #[serde(default)]
    pub community_contribution: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gluten_reports: Option<ProductReportCounts>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub verified_gluten_reports: Option<ProductReportCounts>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ingredients: Option<String>,
    pub assessment: Assessment,
    #[serde(default)]
    pub nutrition: NutritionInput,
    pub quality: NutritionScore,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub nutri_score: Option<String>,
    pub source: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_url: Option<String>,
    pub fetched_at: String,
    #[serde(default)]
    pub stale: bool,
    #[serde(default)]
    pub user_created: bool,
    /// Only present for the authenticated owner of the verdict.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub my_verdict: Option<PersonalVerdict>,
    /// Source-language ingredients when `ingredients` is a Spanish translation.
    /// The gluten analysis always runs on this original text.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ingredients_original: Option<String>,
    /// ISO 639-1 language of `ingredients_original` ("fr", "en"...).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ingredients_language: Option<String>,
    /// Second opinion of the on-server model. Never part of `assessment` (1.0.x
    /// validates its enums), and never shown when the engine found any evidence.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ai_check: Option<AiCheck>,
    /// Label text typed or photographed by a user. Shown next to the provider's
    /// ingredients, never instead of them.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub community_ingredients: Option<String>,
    /// The provider's own analysis, when a community label raised the risk shown in
    /// `assessment`. Lets the app show both without losing the original.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_assessment: Option<Assessment>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AiCheckStatus {
    /// Every ingredient passed the deterministic gates and the model. Orientative only:
    /// shown as "Análisis orientativo por IA", never as a certification or in green.
    Verified,
    /// The model thinks some ingredients may contain gluten (listed in `flagged`).
    Suspected,
    /// Queued; the server CPU is slow, results arrive within a minute or two.
    Pending,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiCheck {
    pub status: AiCheckStatus,
    pub message: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub flagged: Vec<String>,
    pub model: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub checked_at: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Hash)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum GlutenSupport {
    DedicatedGlutenFree,
    GlutenFreeOptions,
    NoGlutenFreeOptions,
    Unknown,
}

impl GlutenSupport {
    pub fn as_str(self) -> &'static str {
        match self {
            GlutenSupport::DedicatedGlutenFree => "DEDICATED_GLUTEN_FREE",
            GlutenSupport::GlutenFreeOptions => "GLUTEN_FREE_OPTIONS",
            GlutenSupport::NoGlutenFreeOptions => "NO_GLUTEN_FREE_OPTIONS",
            GlutenSupport::Unknown => "UNKNOWN",
        }
    }
    pub fn parse(value: &str) -> Option<Self> {
        Some(match value {
            "DEDICATED_GLUTEN_FREE" => GlutenSupport::DedicatedGlutenFree,
            "GLUTEN_FREE_OPTIONS" => GlutenSupport::GlutenFreeOptions,
            "NO_GLUTEN_FREE_OPTIONS" => GlutenSupport::NoGlutenFreeOptions,
            "UNKNOWN" => GlutenSupport::Unknown,
            _ => return None,
        })
    }
    pub fn label(self) -> &'static str {
        match self {
            GlutenSupport::DedicatedGlutenFree => "100 % sin gluten",
            GlutenSupport::GlutenFreeOptions => "Opciones sin gluten",
            GlutenSupport::NoGlutenFreeOptions => "Sin opciones sin gluten",
            GlutenSupport::Unknown => "Sin datos de gluten",
        }
    }
    pub fn is_positive(self) -> bool {
        matches!(
            self,
            GlutenSupport::DedicatedGlutenFree | GlutenSupport::GlutenFreeOptions
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChainRef {
    pub key: String,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReviewGroup {
    pub key: String,
    pub name: String,
    pub count: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RatingSummary {
    pub average: f64,
    pub count: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct RestaurantReportCounts {
    pub dedicated: i64,
    pub options: i64,
    pub no_options: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Restaurant {
    pub id: String,
    pub name: String,
    pub address: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub latitude: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub longitude: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub website: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub phone: Option<String>,
    pub maps_url: String,
    pub gluten_support: GlutenSupport,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub community_created: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chain: Option<ChainRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub review_group: Option<ReviewGroup>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rating: Option<RatingSummary>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gluten_reports: Option<RestaurantReportCounts>,
    pub evidence: Vec<Evidence>,
    pub source: String,
    #[serde(default)]
    pub attributions: Vec<String>,
    pub fetched_at: String,
    /// Distance in metres from the search point, when the search had a location.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub distance_m: Option<f64>,
    /// Whether this branch has its own (local) review that overrides the chain verdict.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub has_local_review: Option<bool>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SuggestionKind {
    Restaurant,
    Place,
    Chain,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RestaurantSuggestion {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    pub label: String,
    pub kind: SuggestionKind,
    pub query: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub latitude: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub longitude: Option<f64>,
    /// Present for chain suggestions.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chain_key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branch_count: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChainSummary {
    pub key: String,
    pub name: String,
    pub branch_count: i64,
    /// Verdict that applies to every branch without its own review.
    pub gluten_support: GlutenSupport,
    #[serde(default)]
    pub reports: RestaurantReportCounts,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChainDetail {
    pub chain: ChainSummary,
    pub branches: Vec<Restaurant>,
    pub total: i64,
    /// Whether the current account may mark the whole chain.
    pub can_mark_chain: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommunityReview {
    pub id: String,
    pub revision: String,
    pub own: bool,
    pub confirmed_by_me: bool,
    pub note: String,
    pub status: String,
    pub updated_at: String,
    pub verified_at: Option<String>,
    #[serde(default)]
    pub verification_method: Option<String>,
    pub confirmations: i64,
    pub inherited: bool,
    /// "local" or "chain" (restaurants only).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Role {
    #[serde(rename = "authorized_celiac")]
    AuthorizedCeliac,
    #[serde(rename = "ADMIN")]
    Admin,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct User {
    pub id: String,
    pub email: String,
    #[serde(default)]
    pub roles: Vec<Role>,
    #[serde(default)]
    pub first_name: Option<String>,
    #[serde(default)]
    pub last_name: Option<String>,
    #[serde(default)]
    pub date_of_birth: Option<String>,
    #[serde(default)]
    pub product_updates_consent: bool,
    #[serde(default)]
    pub product_updates_consent_at: Option<String>,
    #[serde(default)]
    pub product_updates_consent_version: Option<String>,
}

impl User {
    pub fn is_admin(&self) -> bool {
        self.roles.contains(&Role::Admin)
    }
    /// Trusted accounts may mark whole chains and their reviews count as verified.
    pub fn is_trusted(&self) -> bool {
        self.roles
            .iter()
            .any(|r| matches!(r, Role::Admin | Role::AuthorizedCeliac))
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Session {
    pub access_token: String,
    pub refresh_token: String,
    pub user: User,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "item", rename_all = "lowercase")]
pub enum PublicShare {
    Product(Box<Product>),
    Restaurant(Box<Restaurant>),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ApiErrorBody {
    pub error: ApiErrorDetail,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ApiErrorDetail {
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Items<T> {
    pub items: Vec<T>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Page<T> {
    pub items: Vec<T>,
    pub next_cursor: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Collection {
    pub id: String,
    pub name: String,
    pub count: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Meta {
    pub status: String,
    pub environment: String,
    pub version: String,
}

/// Result of the AI label analysis.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LabelAnalysis {
    /// Final, conservative assessment (deterministic engine ∧ AI).
    pub assessment: Assessment,
    pub ingredients: String,
    pub allergens: Vec<String>,
    pub traces: Vec<String>,
    pub explanation: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub product_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub certification: Option<Certification>,
    pub ai_status: GlutenStatus,
}

pub const RESTAURANT_DISCLAIMER: &str =
    "Confirma siempre con el restaurante sus protocolos y posibles contaminaciones cruzadas.";
pub const PRODUCT_DISCLAIMER: &str = "Comprueba la etiqueta actual. La información puede estar incompleta o haber cambiado. La confianza refleja la evidencia disponible, no una probabilidad de seguridad.";

pub const BARCODE_PATTERN: &str = r"^(?:\d{8}|\d{12,14})$";

/// True for EAN-8, UPC-A (12), EAN-13 and GTIN-14 digit strings.
pub fn is_barcode(value: &str) -> bool {
    value.bytes().all(|b| b.is_ascii_digit()) && matches!(value.len(), 8 | 12 | 13 | 14)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShareKind {
    Product,
    Restaurant,
}

/// Parse `https://<public>/p/<token>`, `https://<public>/r/<token>` or `safeeat://p/<token>`.
pub fn parse_share_link(value: &str, public_app_url: &str) -> Option<(ShareKind, String)> {
    let path = if let Some(rest) = value.strip_prefix("safeeat://") {
        format!("/{}", rest.split(['?', '#']).next().unwrap_or(""))
    } else {
        let base = public_app_url.trim_end_matches('/');
        let rest = value.strip_prefix(base)?;
        if !rest.starts_with('/') {
            return None;
        }
        rest.split(['?', '#']).next().unwrap_or("").to_string()
    };
    let mut parts = path.trim_start_matches('/').split('/');
    let kind = match parts.next()? {
        "p" => ShareKind::Product,
        "r" => ShareKind::Restaurant,
        _ => return None,
    };
    let token = parts.next()?;
    if parts.next().is_some()
        || token.len() != 48
        || !token
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return None;
    }
    Some((kind, token.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn share_links() {
        let token = "a".repeat(48);
        assert_eq!(
            parse_share_link(
                &format!("https://x.test/pre/p/{token}"),
                "https://x.test/pre/"
            ),
            Some((ShareKind::Product, token.clone()))
        );
        assert_eq!(
            parse_share_link(&format!("safeeat://r/{token}"), "https://x.test"),
            Some((ShareKind::Restaurant, token.clone()))
        );
        assert_eq!(
            parse_share_link(&format!("https://evil.test/p/{token}"), "https://x.test"),
            None
        );
        assert_eq!(
            parse_share_link("https://x.test/p/zz", "https://x.test"),
            None
        );
    }

    #[test]
    fn optional_fields_are_omitted() {
        let r = RestaurantReportCounts::default();
        assert_eq!(
            serde_json::to_string(&r).unwrap(),
            r#"{"dedicated":0,"options":0,"noOptions":0}"#
        );
        let share = PublicShare::Restaurant(Box::new(Restaurant {
            id: "x".into(),
            name: "n".into(),
            address: "".into(),
            latitude: None,
            longitude: None,
            website: None,
            phone: None,
            maps_url: "https://m".into(),
            gluten_support: GlutenSupport::Unknown,
            community_created: None,
            chain: None,
            review_group: None,
            rating: None,
            gluten_reports: None,
            evidence: vec![],
            source: "s".into(),
            attributions: vec![],
            fetched_at: "t".into(),
            distance_m: None,
            has_local_review: None,
        }));
        let json = serde_json::to_value(&share).unwrap();
        assert_eq!(json["type"], "restaurant");
        assert!(json["item"].get("latitude").is_none());
    }
}
