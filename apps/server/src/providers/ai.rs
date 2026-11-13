//! Label photo analysis with the Claude Messages API (raw HTTP: there is no official Rust SDK).
//!
//! Safety rules (docs/SAFETY.md):
//! - The extracted text always goes through the deterministic gluten engine as well.
//! - The AI alone can never produce "sin gluten": a gluten-free result needs the engine to agree.
//! - The AI's own estimate can stop a green result but never shows a gluten or traces
//!   warning the text it read does not support (the product stays UNKNOWN, with why).

use std::time::Duration;

use base64::Engine as _;
use safeeat_contracts::{Certification, GlutenStatus, LabelAnalysis, Reason};
use safeeat_gluten_engine::{analyze, Context, GlutenInput, Origin};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::error::ApiError;

const MESSAGES_URL: &str = "https://api.anthropic.com/v1/messages";
const API_VERSION: &str = "2026-10-21";
const FALLBACK_BETA: &str = "server-side-fallback-2026-07-01";
/// Models that accept the server-side `fallbacks: "default"` parameter.
const FALLBACK_MODELS: &[&str] = &[
    "claude-sonnet-5-5",
    "claude-opus-5-5",
    "claude-opus-5",
    "claude-fable-5-2",
    "claude-fable-5-1",
];

const SYSTEM_PROMPT: &str = "Eres un asistente que lee etiquetas de alimentos para una persona celíaca. \
Recibes una foto de un envase. Transcribe exactamente lo que se lee, sin inventar nada:\n\
- ingredients: la lista de ingredientes tal cual aparece (en su idioma original). Cadena vacía si no se lee.\n\
- allergens: alérgenos destacados (negrita, mayúsculas o sección \"Alérgenos\").\n\
- traces: frases de trazas o contaminación cruzada (\"puede contener...\", \"elaborado en una fábrica que...\").\n\
- glutenFreeClaim: true solo si el envase dice textualmente \"sin gluten\", \"gluten free\" o equivalente.\n\
- certifiedSeal: true solo si ves el símbolo de la espiga barrada (espiga tachada dentro de un círculo).\n\
- glutenEstimate: tu valoración. CONTAINS_GLUTEN si hay trigo, cebada, centeno, espelta, kamut, triticale, avena no certificada, malta o derivados; \
MAY_CONTAIN si hay trazas o advertencia de contaminación; NO_GLUTEN_INGREDIENTS si la lista completa es legible y nada contiene gluten; \
UNREADABLE si la foto no permite leer la lista completa.\n\
- explanation: una o dos frases en español, claras, para la usuaria, diciendo qué has visto y por qué.\n\
Si dudas, elige la opción más prudente. Nunca supongas ingredientes que no se ven.";

/// What Claude reads from the photo.
#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LabelExtraction {
    pub product_name: String,
    pub ingredients: String,
    pub allergens: Vec<String>,
    pub traces: Vec<String>,
    pub gluten_free_claim: bool,
    pub certified_seal: bool,
    pub gluten_estimate: GlutenEstimate,
    pub explanation: String,
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum GlutenEstimate {
    ContainsGluten,
    MayContain,
    NoGlutenIngredients,
    Unreadable,
}

impl GlutenEstimate {
    fn status(self) -> GlutenStatus {
        match self {
            GlutenEstimate::ContainsGluten => GlutenStatus::ContainsGluten,
            GlutenEstimate::MayContain => GlutenStatus::MayContainTraces,
            GlutenEstimate::NoGlutenIngredients => GlutenStatus::GlutenFree,
            GlutenEstimate::Unreadable => GlutenStatus::Unknown,
        }
    }
}

fn output_schema() -> Value {
    let list = json!({"type": "array", "items": {"type": "string"}});
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["productName", "ingredients", "allergens", "traces", "glutenFreeClaim", "certifiedSeal", "glutenEstimate", "explanation"],
        "properties": {
            "productName": {"type": "string"},
            "ingredients": {"type": "string"},
            "allergens": list,
            "traces": list,
            "glutenFreeClaim": {"type": "boolean"},
            "certifiedSeal": {"type": "boolean"},
            "glutenEstimate": {"type": "string", "enum": ["CONTAINS_GLUTEN", "MAY_CONTAIN", "NO_GLUTEN_INGREDIENTS", "UNREADABLE"]},
            "explanation": {"type": "string"}
        }
    })
}

pub struct LabelAi {
    http: reqwest::Client,
    key: String,
    model: String,
}

fn ai_unavailable() -> ApiError {
    ApiError::new(
        503,
        "AI_UNAVAILABLE",
        "El análisis con IA no está disponible ahora mismo. Inténtalo más tarde.",
    )
}

impl LabelAi {
    pub fn new(http: reqwest::Client, key: String, model: String) -> Self {
        Self { http, key, model }
    }

    pub fn request_body(&self, jpeg: &[u8]) -> Value {
        let data = base64::engine::general_purpose::STANDARD.encode(jpeg);
        let mut body = json!({
            "model": self.model,
            "max_tokens": 8000,
            "system": SYSTEM_PROMPT,
            "output_config": {
                "effort": "medium",
                "format": {"type": "json_schema", "schema": output_schema()}
            },
            "messages": [{
                "role": "user",
                "content": [
                    {"type": "image", "source": {"type": "base64", "media_type": "image/jpeg", "data": data}},
                    {"type": "text", "text": "Lee esta etiqueta y devuelve los datos pedidos."}
                ]
            }]
        });
        if self.uses_fallback() {
            body["fallbacks"] = json!("default");
        }
        body
    }

    fn uses_fallback(&self) -> bool {
        FALLBACK_MODELS.contains(&self.model.as_str())
    }

    pub async fn extract(&self, jpeg: &[u8]) -> Result<LabelExtraction, ApiError> {
        let mut request = self
            .http
            .post(MESSAGES_URL)
            .header("x-api-key", &self.key)
            .header("anthropic-version", API_VERSION)
            .timeout(Duration::from_secs(90))
            .json(&self.request_body(jpeg));
        if self.uses_fallback() {
            request = request.header("anthropic-beta", FALLBACK_BETA);
        }
        let response = request.send().await.map_err(|error| {
            tracing::warn!(%error, "label AI request failed");
            ai_unavailable()
        })?;
        let status = response.status();
        let raw: Value = response.json().await.map_err(|_| ai_unavailable())?;
        if !status.is_success() {
            let kind = raw
                .pointer("/error/type")
                .and_then(Value::as_str)
                .unwrap_or("unknown");
            tracing::warn!(%status, kind, "label AI returned an error");
            return Err(if status.as_u16() == 429 || status.as_u16() == 529 {
                ApiError::new(
                    503,
                    "AI_BUSY",
                    "El análisis con IA está saturado. Espera un momento y vuelve a intentarlo.",
                )
            } else {
                ai_unavailable()
            });
        }
        parse_response(&raw)
    }
}

/// Reads the structured-output text block, after checking `stop_reason`.
pub fn parse_response(raw: &Value) -> Result<LabelExtraction, ApiError> {
    match raw.get("stop_reason").and_then(Value::as_str) {
        Some("refusal") => {
            return Err(ApiError::new(
                422,
                "AI_REFUSED",
                "La IA no ha podido analizar esta foto. Prueba con otra imagen de la etiqueta.",
            ))
        }
        Some("max_tokens") => return Err(ai_unavailable()),
        _ => {}
    }
    let text = raw
        .get("content")
        .and_then(Value::as_array)
        .and_then(|blocks| {
            blocks
                .iter()
                .filter(|b| b.get("type").and_then(Value::as_str) == Some("text"))
                .filter_map(|b| b.get("text").and_then(Value::as_str))
                .next_back()
        })
        .ok_or_else(ai_unavailable)?;
    serde_json::from_str(text).map_err(|error| {
        tracing::warn!(%error, "label AI returned JSON that does not match the schema");
        ai_unavailable()
    })
}

fn non_empty(v: &str) -> Option<String> {
    let t = v.trim();
    (!t.is_empty()).then(|| t.to_string())
}

/// Combine the AI reading with the deterministic engine.
///
/// The text the model read (ingredients, allergens, traces, a "sin gluten" mention) is
/// analysed by the rules: that is the evidence. The model's own estimate is a
/// hypothesis: it can stop a green result, but on its own it never shows a gluten or
/// traces warning (that would be an AI guess dressed as a declared warning); the
/// product stays UNKNOWN and says why.
pub fn combine(extraction: LabelExtraction) -> LabelAnalysis {
    let declaration = extraction.gluten_free_claim || extraction.certified_seal;
    let engine = analyze(
        &GlutenInput {
            ingredients: non_empty(&extraction.ingredients),
            allergens: (!extraction.allergens.is_empty()).then(|| extraction.allergens.join(", ")),
            traces: (!extraction.traces.is_empty()).then(|| extraction.traces.join(", ")),
            declaration: declaration.then(|| "sin gluten".to_string()),
            source: "Foto de la etiqueta (lectura con IA)".into(),
            origin: Some(Origin::Ocr),
            ..Default::default()
        },
        Context::default(),
    );
    let ai_status = extraction.gluten_estimate.status();
    let reading = Reason {
        code: "AI_LABEL_READING".into(),
        message: format!(
            "Lectura de la etiqueta con IA: {}",
            extraction.explanation.trim()
        ),
    };
    let engine_status = engine.assessment.status;
    let mut assessment = match (engine_status, ai_status) {
        // The model doubts a "sin gluten" the rules accepted: not green.
        (GlutenStatus::GlutenFree, other) if other != GlutenStatus::GlutenFree => engine
            .without_claim(
                "AI_ESTIMATE_DISAGREES",
                "La lectura con IA no confirma que sea apto aunque la etiqueta diga «sin gluten». Compruébalo en el envase.",
            )
            .assessment,
        // A warning only the model sees: not shown as a declared warning.
        (engine_status, GlutenStatus::ContainsGluten | GlutenStatus::MayContainTraces)
            if ai_status.risk() > engine_status.risk() =>
        {
            let mut a = engine.assessment;
            if a.status == GlutenStatus::GlutenFree {
                a.status = GlutenStatus::Unknown;
            }
            a.reasons.push(Reason {
                code: "AI_ESTIMATE_UNCONFIRMED".into(),
                message: "La lectura con IA sospecha gluten o trazas, pero el texto leído no lo confirma. No se ha podido determinar la presencia de gluten: revisa la etiqueta.".into(),
            });
            a
        }
        _ => engine.assessment,
    };
    if assessment.status != GlutenStatus::GlutenFree {
        assessment.confidence = assessment.confidence.min(0.6);
    }
    assessment.reasons.push(reading);
    let certification = if extraction.certified_seal {
        Some(Certification::CertifiedSeal)
    } else if extraction.gluten_free_claim {
        Some(Certification::GlutenFreeClaim)
    } else {
        None
    };
    LabelAnalysis {
        assessment,
        ingredients: extraction.ingredients.trim().to_string(),
        allergens: extraction.allergens,
        traces: extraction.traces,
        explanation: extraction.explanation.trim().to_string(),
        product_name: non_empty(&extraction.product_name),
        certification,
        ai_status,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn extraction(ingredients: &str, estimate: GlutenEstimate, claim: bool) -> LabelExtraction {
        LabelExtraction {
            product_name: "Galletas".into(),
            ingredients: ingredients.into(),
            allergens: vec![],
            traces: vec![],
            gluten_free_claim: claim,
            certified_seal: false,
            gluten_estimate: estimate,
            explanation: "x".into(),
        }
    }

    #[test]
    fn ai_alone_never_says_gluten_free() {
        let r = combine(extraction(
            "harina de arroz, azúcar",
            GlutenEstimate::NoGlutenIngredients,
            false,
        ));
        assert_ne!(r.assessment.status, GlutenStatus::GlutenFree);
    }

    #[test]
    fn engine_wins_when_ai_misses_gluten() {
        let r = combine(extraction(
            "harina de trigo, azúcar",
            GlutenEstimate::NoGlutenIngredients,
            false,
        ));
        assert_eq!(r.assessment.status, GlutenStatus::ContainsGluten);
    }

    #[test]
    fn ai_estimate_stops_green_but_never_invents_a_warning() {
        // The model guesses traces the label text does not show: not amber.
        for claim in [false, true] {
            let r = combine(extraction(
                "harina de arroz",
                GlutenEstimate::MayContain,
                claim,
            ));
            assert_eq!(r.assessment.status, GlutenStatus::Unknown);
            assert!(r
                .assessment
                .reasons
                .iter()
                .any(|x| x.code == "AI_ESTIMATE_UNCONFIRMED"));
        }
        let r = combine(extraction(
            "arroz, sal",
            GlutenEstimate::ContainsGluten,
            false,
        ));
        assert_eq!(r.assessment.status, GlutenStatus::Unknown);
        // Traces the model actually read on the label are a declared warning.
        let mut read = extraction("arroz", GlutenEstimate::MayContain, false);
        read.traces = vec!["gluten".into()];
        assert_eq!(
            combine(read).assessment.status,
            GlutenStatus::MayContainTraces
        );
        // An unreviewed photo reading is never green, even when both agree.
        let r = combine(extraction(
            "harina de arroz, azúcar",
            GlutenEstimate::NoGlutenIngredients,
            true,
        ));
        assert_eq!(r.assessment.status, GlutenStatus::Unknown);
    }

    #[test]
    fn parses_structured_output_and_refusal() {
        let ok = json!({"stop_reason": "end_turn", "content": [
            {"type": "thinking", "thinking": ""},
            {"type": "text", "text": "{\"productName\":\"\",\"ingredients\":\"arroz\",\"allergens\":[],\"traces\":[],\"glutenFreeClaim\":false,\"certifiedSeal\":true,\"glutenEstimate\":\"UNREADABLE\",\"explanation\":\"e\"}"}
        ]});
        let e = parse_response(&ok).unwrap();
        assert!(e.certified_seal);
        assert_eq!(e.gluten_estimate, GlutenEstimate::Unreadable);
        let refused = json!({"stop_reason": "refusal", "content": []});
        assert_eq!(parse_response(&refused).unwrap_err().code, "AI_REFUSED");
    }

    #[test]
    fn fallback_only_for_supported_models() {
        let ai = LabelAi::new(
            reqwest::Client::new(),
            "k".into(),
            "claude-sonnet-5-5".into(),
        );
        assert_eq!(ai.request_body(b"x")["fallbacks"], json!("default"));
        let other = LabelAi::new(
            reqwest::Client::new(),
            "k".into(),
            "claude-haiku-5-5".into(),
        );
        assert!(other.request_body(b"x").get("fallbacks").is_none());
    }
}
