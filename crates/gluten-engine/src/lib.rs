//! Deterministic gluten evidence engine.
//!
//! Explicit evidence only: a missing hazard is never evidence of absence.
//!
//! Two layers:
//! - `assess_gluten` (core, version `CORE_VERSION`): a faithful port of
//!   `packages/gluten-engine/src/index.ts`; the golden cases in `tests/golden.json`
//!   were produced by the TypeScript engine and must keep producing identical output.
//! - `analyze` (version `ENGINE_VERSION`): the core plus the versioned rule catalog
//!   (`catalog`), which explains every ingredient, applies the legal exemptions and
//!   tells the evidence cases apart. This is what the server serves.

mod analysis;
pub mod catalog;
mod items;
mod nutrition;

pub use analysis::{analyze, Analysis, ClassifiedItem, Context, ENGINE_VERSION};
pub use catalog::RULES_VERSION;
pub use nutrition::score_nutrition;

use fancy_regex::Regex as FancyRegex;
use once_cell::sync::Lazy;
use regex::Regex;
use safeeat_contracts::{Assessment, Evidence, EvidenceKind, GlutenStatus, Reason};
use serde::{Deserialize, Serialize};
use unicode_normalization::UnicodeNormalization;

/// Version of the core engine (`assess_gluten`), locked by the golden cases.
pub const CORE_VERSION: &str = "1.1.0";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Origin {
    #[default]
    Provider,
    Manual,
    Ocr,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GlutenInput {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ingredients: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub allergens: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub traces: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub declaration: Option<String>,
    pub source: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub origin: Option<Origin>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reviewed: Option<bool>,
}

/// JavaScript `\s` (ECMAScript WhiteSpace + LineTerminator).
fn js_space(c: char) -> bool {
    matches!(
        c,
        '\t' | '\n' | '\u{0b}' | '\u{0c}' | '\r' | ' ' | '\u{a0}' | '\u{1680}' | '\u{2000}'
            ..='\u{200a}'
                | '\u{2028}'
                | '\u{2029}'
                | '\u{202f}'
                | '\u{205f}'
                | '\u{3000}'
                | '\u{feff}'
    )
}

/// Same output as the TypeScript `normalize` (NFKD, no marks, lowercase, ASCII words).
pub fn normalize(text: &str) -> String {
    let lowered: String = text
        .nfkd()
        .filter(|c| !('\u{300}'..='\u{36f}').contains(c))
        .collect::<String>()
        .to_lowercase();
    let mut mapped = String::with_capacity(lowered.len());
    for c in lowered.chars() {
        let keep = c.is_ascii_lowercase() || c.is_ascii_digit() || ".,;:!?()".contains(c);
        if matches!(c, '‐' | '‑' | '–' | '—' | '_' | '-') {
            mapped.push(' ');
        } else if keep || js_space(c) {
            mapped.push(c);
        } else {
            mapped.push(' ');
        }
    }
    let mut out = String::with_capacity(mapped.len());
    let mut space = false;
    for c in mapped.chars() {
        if js_space(c) {
            space = true;
        } else {
            if space && !out.is_empty() {
                out.push(' ');
            }
            space = false;
            out.push(c);
        }
    }
    out
}

fn re(pattern: &str) -> Regex {
    Regex::new(pattern).expect("valid regex")
}

// Word lists shared by the patterns below; the same strings as the TypeScript engine.
// Cereals with gluten in Spanish, English, French, Italian, German, Dutch, Portuguese and Catalan.
const CEREALS: &str = "trigo|wheat|cebada|barley|centeno|rye|espelta|spelt|escanda|kamut|khorasan|triticale|einkorn|emmer|farro|freekeh|durum|seitan|bulgur|cuscus|couscous|ble|froment|orge|seigle|epeautre|frumento|grano duro|grano tenero|orzo|segale|weizen[a-z0-9]*|gerste[a-z0-9]*|roggen[a-z0-9]*|dinkel[a-z0-9]*|tarwe[a-z0-9]*|gerst|rogge|cevada|centeio|blat|ordi|segol";
const OATS: &str =
    "avena|oats?|avoine|hafer[a-z0-9]*|aveia|civada|havermout|havervlokken|havermeel";
const OTHER_HAZARDS: &str = "gluten|glutine|malta|malt|malte|malz[a-z0-9]*";
// A negation only cancels the hazard it governs: it must come right before it, separated
// at most by filler words or other hazards ("no contiene harina de trigo ni cebada").
// "Harina de trigo, no contiene leche" and "cerveza sin alcohol de cebada" stay positive.
const NEGATIONS: &str = "no|not|sin|s[1l]n|ni|nor|never|nunca|without|free from|libre de|exent[oa]s? de|ausencia de|sans|ohne|senza|sem|sense|kein[a-z0-9]*";
const FILLERS: &str = "de|del|la|el|los|las|lo|of|the|any|added|anadid[oa]s?|harinas?|flour|con|contienen?|contener|contain|contains|containing|es|is|un|una|a|an|trazas|traces|y|e|o|u|and|or|derivados|productos|ingredientes|ingredients|que|contengan|with|tiene|lleva";

static HAZARDS: Lazy<Regex> = Lazy::new(|| re(&format!(r"\b({CEREALS}|{OTHER_HAZARDS}|{OATS})\b")));
static FREE_CLAIM: Lazy<Regex> = Lazy::new(|| {
    re(
        r"\b(sin gluten|gluten free|libre de gluten|no contiene gluten|does not contain gluten|sans gluten|glutenfrei|senza glutine|sem gluten|sense gluten|glutenvrij)\b",
    )
});
static NEGATION_BEFORE: Lazy<Regex> = Lazy::new(|| {
    re(&format!(
        r"\b(?:{NEGATIONS})(?:\s+(?:{FILLERS}|{CEREALS}|{OTHER_HAZARDS}|{OATS}))*\s*:?\s*$"
    ))
});
/// Hedging about a hazard anywhere in the same list item.
static HEDGE: Lazy<Regex> = Lazy::new(|| {
    re(
        r"\b(no garantiza|no garantizado|not guaranteed|no certificad[ao]|no apto|no es apto|no se puede|no podemos|cannot|uncertain|desconocid[ao]|desconoce|desconocemos|posible|posiblemente|podria|could|might|consult[ae]|consultar|pregunt[ae]|ask|opcion|option|excepto|except|sin confirmar|no confirmado|unconfirmed|no se ha comprobado|no se ha verificado|not tested|not verified)\b",
    )
});
static ITEM_SPLIT: Lazy<Regex> = Lazy::new(|| re(r"[,()]"));
static UNCERTAINTY: Lazy<Regex> = Lazy::new(|| {
    re(
        r"\b(no es|not|no garantiza|no garantizado|not guaranteed|no certificado|no certificad[ao]|no apto|no es apto|no se puede|no podemos|cannot|uncertain|desconocid[ao]|desconoce|desconocemos|posible|posiblemente|podria|could|might|consult[ae]|consultar|pregunt[ae]|ask|opcion|option|excepto|except|sin confirmar|no confirmado|unconfirmed)\b|[?]",
    )
});
static BUCKWHEAT: Lazy<Regex> = Lazy::new(|| {
    re(
        r"\b(trigo sarraceno|buckwheat|ble noir|sarrasin|grano saraceno|trigo mourisco|blat negre|alforfon)\b",
    )
});
static NEGATED_FREE: Lazy<Regex> =
    Lazy::new(|| re(r"\b(no contiene gluten|does not contain gluten)\b"));
static RECIPE: Lazy<Regex> = Lazy::new(|| {
    re(
        r"\b(segun receta|receta|recipe|antes|formerly|previously|no declara|no pone|no indica|no consta|sin comprobar|sin evaluar|sin certificar|no se ha comprobado|no se ha verificado|not tested|not verified|not certified|cereales|cereals)\b",
    )
});
static UNVERIFIED: Lazy<Regex> =
    Lazy::new(|| re(r"\b(no verificado|no evaluado|no apt[oa]s?|not suitable|sin verificar)\b"));
static NEGATION_WORDS: Lazy<Regex> =
    Lazy::new(|| re(r"\b(no|not|never|nunca|fals[oa]|fake|without)\b"));
static NEGATED_CLAIM: Lazy<Regex> = Lazy::new(|| {
    re(
        r"\b(no|not|without)\s+(?:es\s+|is\s+|a\s+|un\s+)?(?:sin gluten|gluten free|libre de gluten)\b",
    )
});
static PRECAUTION: Lazy<FancyRegex> = Lazy::new(|| {
    FancyRegex::new(r"\b(?:puede[n]? contener|puede tener|trazas de|may contain|may contain traces of|traces of|processed in|made in|elaborado en|fabricado en|contaminacion cruzada|cross contamination|peut contenir|traces de|traces eventuelles de|spuren von|puo contenere|tracce di|pode conter|vestigios de|pot contenir)\b.*?(?=\b(?:ingredientes|ingredients|alergenos|allergens|contiene:|contains:)\b|$)").expect("valid regex")
});
static SCOPE_UNCLEAR: Lazy<Regex> = Lazy::new(|| re(r"\b(cereales|cereals|alergenos|allergens)\b"));
static SCOPE_DANGLING: Lazy<Regex> =
    Lazy::new(|| re(r"\b(contener|contain|trazas de|traces of)\s*$"));
static ONLY_OATS: Lazy<Regex> = Lazy::new(|| re(&format!("^({OATS})$")));
static CLAUSE_SPLIT: Lazy<Regex> = Lazy::new(|| re(r"[.;\n]+"));

/// JavaScript `String.prototype.trim` for the ASCII-normalized clauses.
fn js_trim(value: &str) -> &str {
    value.trim_matches(js_space)
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Field {
    Ingredients,
    Allergens,
    Traces,
    Declaration,
}

/// Assess gluten evidence. See module documentation.
pub fn assess_gluten(input: &GlutenInput) -> Assessment {
    let mut evidence: Vec<Evidence> = Vec::new();
    let mut add = |text: &str, kind: EvidenceKind| {
        evidence.push(Evidence {
            source: input.source.clone(),
            text: text.to_string(),
            kind,
            url: input.source_url.clone().filter(|u| !u.is_empty()),
        })
    };
    let (mut contains, mut traces, mut oats, mut claim, mut ambiguous) =
        (false, false, false, false, false);
    let fields = [
        (Field::Ingredients, &input.ingredients),
        (Field::Allergens, &input.allergens),
        (Field::Traces, &input.traces),
        (Field::Declaration, &input.declaration),
    ];
    for (field, raw) in fields {
        let Some(raw) = raw else { continue };
        if js_trim(raw).is_empty() {
            continue;
        }
        let normalized = normalize(raw);
        for clause in CLAUSE_SPLIT.split(&normalized).filter(|c| !c.is_empty()) {
            let mut rest = BUCKWHEAT.replace_all(clause, " ").into_owned();
            let claim_context = NEGATED_FREE.replace_all(clause, "sin gluten").into_owned();
            if RECIPE.is_match(&claim_context) {
                ambiguous = true;
                add(js_trim(clause), EvidenceKind::Uncertainty);
            }
            if UNCERTAINTY.is_match(&claim_context) || UNVERIFIED.is_match(&claim_context) {
                ambiguous = true;
                add(js_trim(clause), EvidenceKind::Uncertainty);
            }
            if FREE_CLAIM.is_match(clause) {
                let unclear = UNCERTAINTY.is_match(&claim_context)
                    || NEGATION_WORDS.is_match(&claim_context)
                    || NEGATED_CLAIM.is_match(clause);
                if unclear {
                    ambiguous = true;
                    add(js_trim(clause), EvidenceKind::Uncertainty);
                } else {
                    claim = true;
                    add(js_trim(clause), EvidenceKind::Claim);
                }
                rest = FREE_CLAIM.replace_all(&rest, " ").into_owned();
            }
            // A precaution applies only after its marker, and ends at an explicit
            // ingredients/allergens section.
            let mut replaced = String::with_capacity(rest.len());
            let mut last = 0;
            for found in PRECAUTION.find_iter(&rest) {
                let Ok(found) = found else { break };
                let scope = found.as_str();
                if HAZARDS.is_match(scope) {
                    traces = true;
                    add(js_trim(scope), EvidenceKind::Trace);
                } else if SCOPE_UNCLEAR.is_match(scope) || SCOPE_DANGLING.is_match(scope) {
                    ambiguous = true;
                    add(js_trim(scope), EvidenceKind::Uncertainty);
                }
                replaced.push_str(&rest[last..found.start()]);
                replaced.push(' ');
                last = found.end();
            }
            replaced.push_str(&rest[last..]);
            let rest = replaced;
            let any_hit = HAZARDS.is_match(&rest);
            if field == Field::Traces && any_hit {
                traces = true;
                add(js_trim(clause), EvidenceKind::Trace);
                continue;
            }
            if any_hit {
                // Judge each list item on its own, so a negation or a doubt about one
                // ingredient never hides a cereal named in another.
                let mut positive = false;
                for item in ITEM_SPLIT.split(&rest) {
                    let doubtful = HEDGE.is_match(item);
                    for hit in HAZARDS.find_iter(item) {
                        // A negated cereal is not positive evidence, and never
                        // establishes gluten-free.
                        if doubtful || NEGATION_BEFORE.is_match(&item[..hit.start()]) {
                            ambiguous = true;
                        } else if ONLY_OATS.is_match(hit.as_str()) {
                            oats = true;
                        } else {
                            positive = true;
                        }
                    }
                }
                if positive {
                    contains = true;
                    add(
                        js_trim(clause),
                        if field == Field::Allergens {
                            EvidenceKind::Allergen
                        } else {
                            EvidenceKind::Ingredient
                        },
                    );
                } else {
                    add(js_trim(clause), EvidenceKind::Uncertainty);
                }
            }
        }
    }
    let origin = input.origin;
    let reviewed = input.reviewed.unwrap_or(false);
    let mut reasons = Vec::new();
    let reason = |code: &str, message: &str| Reason {
        code: code.into(),
        message: message.into(),
    };
    let status;
    let mut confidence: f64;
    if contains {
        status = GlutenStatus::ContainsGluten;
        confidence = 0.95;
        reasons.push(reason(
            "CEREAL_DETECTED",
            "Se han detectado gluten o cereales con gluten en los ingredientes o alérgenos.",
        ));
    } else if traces {
        status = GlutenStatus::MayContainTraces;
        confidence = 0.9;
        reasons.push(reason(
            "PRECAUTIONARY_LABEL",
            "La información disponible advierte de posibles trazas o contaminación cruzada.",
        ));
    } else if claim && !ambiguous && (origin != Some(Origin::Ocr) || reviewed) {
        status = GlutenStatus::GlutenFree;
        confidence = if origin == Some(Origin::Provider) {
            0.75
        } else {
            0.65
        };
        reasons.push(reason(
            "EXPLICIT_CLAIM",
            "El producto está declarado explícitamente como sin gluten en la fuente indicada.",
        ));
    } else {
        status = GlutenStatus::Unknown;
        confidence = 0.0;
        reasons.push(reason(
            "INSUFFICIENT_EVIDENCE",
            "No disponemos de información suficiente para confirmar que sea seguro.",
        ));
    }
    if oats && status != GlutenStatus::GlutenFree {
        reasons.push(reason(
            "OATS_UNCERTAIN",
            "La avena requiere una declaración fiable de ausencia de gluten.",
        ));
    }
    if claim && (contains || traces || ambiguous) {
        reasons.push(reason(
            "CONTRADICTORY_EVIDENCE",
            "Hay información contradictoria; prevalece la precaución. Comprueba la etiqueta y consulta al fabricante.",
        ));
    }
    if origin == Some(Origin::Ocr) {
        confidence = confidence.min(0.65);
        reasons.push(reason(
            "OCR_LIMITATION",
            "El reconocimiento de texto puede omitir palabras. Revisa toda la etiqueta original; no se analiza el aspecto del envase.",
        ));
    }
    Assessment {
        status,
        confidence,
        reasons,
        evidence,
        engine_version: CORE_VERSION.into(),
        certification: None,
        finding: None,
        rules_version: None,
        items: Vec::new(),
    }
}

/// Return the more conservative of two assessments (riskier status wins; on a tie
/// the first one is kept). `GLUTEN_FREE` only survives if both agree.
pub fn most_conservative(primary: Assessment, other: Assessment) -> Assessment {
    use GlutenStatus::*;
    match (primary.status, other.status) {
        // A gluten-free result needs agreement; disagreement with UNKNOWN stays UNKNOWN.
        (GlutenFree, Unknown) => other,
        (Unknown, GlutenFree) => primary,
        (a, b) if b.risk() > a.risk() => other,
        _ => primary,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input(ingredients: &str) -> GlutenInput {
        GlutenInput {
            ingredients: Some(ingredients.into()),
            source: "test".into(),
            origin: Some(Origin::Provider),
            ..Default::default()
        }
    }

    #[test]
    fn normalizes_like_typescript() {
        assert_eq!(
            normalize("  Harina de TRIGO—integral; Sal  "),
            "harina de trigo integral; sal"
        );
        assert_eq!(normalize("Avena\u{00a0}sin\tgluten"), "avena sin gluten");
        assert_eq!(normalize("Café ñandú"), "cafe nandu");
    }

    #[test]
    fn detects_wheat() {
        assert_eq!(
            assess_gluten(&input("Harina de trigo, agua")).status,
            GlutenStatus::ContainsGluten
        );
    }

    #[test]
    fn buckwheat_is_not_wheat() {
        assert_eq!(
            assess_gluten(&input("trigo sarraceno, agua")).status,
            GlutenStatus::Unknown
        );
    }

    #[test]
    fn traces_after_marker() {
        let a = assess_gluten(&input("Arroz, sal. Puede contener trazas de trigo"));
        assert_eq!(a.status, GlutenStatus::MayContainTraces);
    }

    #[test]
    fn conservative_merge() {
        let free = Assessment {
            status: GlutenStatus::GlutenFree,
            confidence: 0.7,
            reasons: vec![],
            evidence: vec![],
            engine_version: "x".into(),
            certification: None,
            finding: None,
            rules_version: None,
            items: vec![],
        };
        let unknown = Assessment {
            status: GlutenStatus::Unknown,
            ..free.clone()
        };
        let contains = Assessment {
            status: GlutenStatus::ContainsGluten,
            ..free.clone()
        };
        assert_eq!(
            most_conservative(free.clone(), unknown.clone()).status,
            GlutenStatus::Unknown
        );
        assert_eq!(
            most_conservative(unknown.clone(), free.clone()).status,
            GlutenStatus::Unknown
        );
        assert_eq!(
            most_conservative(free.clone(), contains).status,
            GlutenStatus::ContainsGluten
        );
        assert_eq!(
            most_conservative(free.clone(), free).status,
            GlutenStatus::GlutenFree
        );
    }
}
