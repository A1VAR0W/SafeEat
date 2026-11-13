//! What the product card says, in the order of docs/SAFETY.md:
//! personal verdict → certified seal → automatic analysis → community.
//! Pure functions, unit-tested on the host (`cargo test -p safeeat-ui`).
//!
//! Colours: red = gluten declared, amber = traces declared (by the source, never by the
//! AI), green = a reliable "sin gluten" declaration or the user's own verdict, neutral =
//! not verified. An AI result or a community review that found nothing is
//! `Tone::Info`: useful, clearly labelled, never a food-safety badge. An AI suspicion is
//! neutral and "sin confirmar".
//!
//! No percentages: the strength of the evidence is a word with its explanation.

use safeeat_contracts::{
    AiCheckStatus, AiItemVerdict, AiOutcome, Assessment, Certification, EvidenceSource, Freshness,
    GlutenFinding, GlutenStatus, IngredientClass, IngredientFinding, Product, VerdictValue,
};

/// Short explanation under an answer that is not a confirmation either way.
pub const UNDETERMINED_BRIEF: &str =
    "La información disponible no permite determinar si este producto es apto para celíacos.";

/// The explanation under the one-line answer, when the answer alone leaves her guessing.
pub fn brief(p: &Product, summary: &Summary) -> Option<&'static str> {
    (summary.tone == Tone::Unknown && p.assessment.status == GlutenStatus::Unknown)
        .then_some(UNDETERMINED_BRIEF)
}

/// Engine reasons that only repeat the card's title ("No se ha podido determinar la
/// presencia de gluten."): the title already says it.
pub fn shown_reasons(a: &Assessment) -> Vec<String> {
    a.reasons
        .iter()
        .filter(|r| {
            !(r.code == "INSUFFICIENT_EVIDENCE"
                && r.message
                    .starts_with("No se ha podido determinar la presencia de gluten")
                && a.reasons.len() > 1)
        })
        .map(|r| r.message.clone())
        .collect()
}

/// One group of the full analysis ("Ingredientes que no reconocemos", ...).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IngredientGroup {
    pub title: &'static str,
    /// Ingredient as written, and how the rules classified it when the group mixes
    /// classes (a group of unknown ones does not repeat "No reconocido" on every row).
    pub items: Vec<(String, Option<&'static str>)>,
}

/// Ingredients grouped by what they mean for her, worst first, each name once.
/// Nutrition-table fragments and consumer warnings are kept apart from ingredients.
pub fn ingredient_groups(items: &[IngredientFinding]) -> Vec<IngredientGroup> {
    use IngredientClass as C;
    const GROUPS: [&str; 5] = [
        "Requieren atención",
        "No reconocidos",
        "Reconocidos",
        "Advertencias del envase",
        "Texto apartado: no son ingredientes",
    ];
    let mut out: Vec<IngredientGroup> = GROUPS
        .iter()
        .map(|title| IngredientGroup {
            title,
            items: Vec::new(),
        })
        .collect();
    for i in items {
        let at = match i.class {
            C::GlutenSource | C::LikelyGluten | C::Oats | C::NeedsOrigin => 0,
            C::Unrecognized | C::Other => 1,
            C::NotIngredient if i.rule.as_deref() == Some("consumer-warning") => 3,
            C::NotIngredient => 4,
            _ => 2,
        };
        let text = i.text.trim().to_string();
        let group = &mut out[at].items;
        if !text.is_empty() && !group.iter().any(|(t, _)| t.eq_ignore_ascii_case(&text)) {
            let label = matches!(at, 0 | 2).then(|| class_label(i.class));
            group.push((text, label));
        }
    }
    out.retain(|g| !g.items.is_empty());
    out
}

/// A "sin gluten" recorded by the community, not by the manufacturer or a database fed
/// by brand owners: shown as a declaration nobody has verified, never green.
pub fn community_claim(a: &Assessment) -> bool {
    a.status == GlutenStatus::GlutenFree && a.provenance == Some(EvidenceSource::Community)
}

/// The information is too old to be a current confirmation (the source could not be
/// reached for over a week, or older rules analysed it).
pub fn not_current(p: &Product) -> bool {
    matches!(p.freshness, Some(Freshness::Outdated | Freshness::OldRules))
}

/// Tone of the analysis card: never green for a community claim or old information
/// (the same rule as the one-line summary).
pub fn analysis_tone(p: &Product) -> Tone {
    if p.assessment.status == GlutenStatus::GlutenFree
        && (community_claim(&p.assessment) || not_current(p))
    {
        Tone::Info
    } else {
        status_tone(p.assessment.status)
    }
}

/// Title of the analysis card, saying who stands behind a "sin gluten".
pub fn analysis_heading(p: &Product) -> &'static str {
    if community_claim(&p.assessment) {
        "«Sin gluten» según datos de la comunidad, sin verificar"
    } else if p.assessment.status == GlutenStatus::GlutenFree && not_current(p) {
        "Sin gluten declarado en datos antiguos, sin actualizar"
    } else {
        analysis_title(&p.assessment)
    }
}

/// Banner text when the information shown is not current, independent of the status.
pub fn freshness_notice(p: &Product) -> Option<&'static str> {
    match p.freshness {
        Some(Freshness::Cached) => Some(
            "No hemos podido actualizar este producto: mostramos los últimos datos guardados (de hace pocos días).",
        ),
        Some(Freshness::Outdated) => Some(
            "Datos guardados hace más de una semana y sin actualizar: no son una confirmación actual. Comprueba el envase.",
        ),
        Some(Freshness::OldRules) => Some(
            "Analizado con reglas anteriores. Abre de nuevo el producto con conexión para actualizar el análisis.",
        ),
        // App 1.x servers only send `stale`.
        None if p.stale => Some("Datos sin actualizar: comprueba el envase."),
        _ => None,
    }
}

/// Visual tone of a badge. Every tone also carries an icon and words, never colour alone.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tone {
    Safe,
    Danger,
    Caution,
    Unknown,
    /// Orientative information (AI, community). Neutral colour, never green.
    Info,
}

impl Tone {
    pub fn class(self) -> &'static str {
        match self {
            Tone::Safe => "tone-safe",
            Tone::Danger => "tone-danger",
            Tone::Caution => "tone-caution",
            Tone::Unknown => "tone-unknown",
            Tone::Info => "tone-info",
        }
    }
    pub fn icon(self) -> &'static str {
        match self {
            Tone::Safe => crate::icons::CHECK,
            Tone::Danger => crate::icons::X_CIRCLE,
            Tone::Caution => crate::icons::ALERT,
            Tone::Unknown => crate::icons::HELP,
            Tone::Info => crate::icons::SPARKLES,
        }
    }
}

pub fn status_tone(status: GlutenStatus) -> Tone {
    match status {
        GlutenStatus::GlutenFree => Tone::Safe,
        GlutenStatus::ContainsGluten => Tone::Danger,
        GlutenStatus::MayContainTraces => Tone::Caution,
        GlutenStatus::Unknown => Tone::Unknown,
    }
}

pub fn status_title(status: GlutenStatus) -> &'static str {
    match status {
        GlutenStatus::GlutenFree => "Sin gluten declarado",
        GlutenStatus::ContainsGluten => "Contiene gluten",
        GlutenStatus::MayContainTraces => "Puede contener trazas",
        GlutenStatus::Unknown => "No verificado",
    }
}

/// One-line summary for lists (the same rules as app 1.0.x `ProductGlutenLabel`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Summary {
    pub tone: Tone,
    pub text: String,
    /// Shown under an answer that still needs care (AI or community information).
    pub warning: Option<&'static str>,
}

fn plural(n: i64, one: &str, many: &str) -> String {
    if n == 1 {
        format!("{n} {one}")
    } else {
        format!("{n} {many}")
    }
}

pub fn summary(p: &Product) -> Summary {
    // Her own verdict is what she trusts most.
    if let Some(v) = &p.my_verdict {
        // A verdict given on other ingredients or other allergen/trace information is
        // kept as history (the product page shows it) but does not decide the badge.
        if !v.ingredients_changed && !v.evidence_changed {
            return match v.verdict {
                // Her "safe" never hides a risk the analysis found.
                VerdictValue::Safe if p.assessment.status.risk() >= 2 => Summary {
                    tone: Tone::Caution,
                    text: "Marcado por ti como seguro · el análisis detecta riesgo".into(),
                    warning: None,
                },
                VerdictValue::Safe => Summary {
                    tone: Tone::Safe,
                    text: "Marcado por ti como seguro".into(),
                    warning: None,
                },
                VerdictValue::Unsafe => Summary {
                    tone: Tone::Danger,
                    text: "Marcado por ti como no seguro".into(),
                    warning: None,
                },
            };
        }
    }
    let status = p.assessment.status;
    let reports = p.gluten_reports.unwrap_or_default();
    let verified_reports = p.verified_gluten_reports.unwrap_or_default();
    let (free, contains) = (reports.gluten_free, reports.contains_gluten);
    let unknown = status == GlutenStatus::Unknown;
    let conflict = (free > 0 && contains > 0)
        || (contains > 0 && status == GlutenStatus::GlutenFree)
        || (free > 0 && status == GlutenStatus::ContainsGluten);
    if conflict {
        return Summary {
            tone: Tone::Caution,
            text: "Información sobre gluten contradictoria".into(),
            warning: None,
        };
    }
    if unknown {
        let verified = (contains > 0 && free == 0 && verified_reports.contains_gluten > 0)
            || (free > 0 && contains == 0 && verified_reports.gluten_free > 0);
        if verified {
            return if contains > 0 {
                Summary {
                    tone: Tone::Danger,
                    text: "Revisión verificada: contiene gluten".into(),
                    warning: None,
                }
            } else {
                Summary {
                    tone: Tone::Info,
                    text: "Revisión de confianza: sin gluten".into(),
                    warning: Some("Opinión de la comunidad, no del fabricante"),
                }
            };
        }
        if contains > 0 {
            return Summary {
                tone: Tone::Danger,
                text: format!(
                    "{} gluten",
                    plural(contains, "usuario indica", "usuarios indican")
                ),
                warning: None,
            };
        }
        if free > 0 {
            return Summary {
                tone: Tone::Caution,
                text: format!(
                    "{} sin gluten · sin confirmar",
                    plural(free, "usuario indica", "usuarios indican")
                ),
                warning: None,
            };
        }
        match ai_note(p).map(|n| n.outcome) {
            // Orientative only: the AI never certifies and cannot see traces.
            Some(AiOutcome::NoGlutenSeen) => {
                return Summary {
                    tone: Tone::Info,
                    text: "La IA no relaciona con gluten los ingredientes dudosos".into(),
                    warning: Some("Análisis orientativo: no es una certificación"),
                }
            }
            Some(AiOutcome::Pending) => {
                return Summary {
                    tone: Tone::Unknown,
                    text: "Consultando a la IA los ingredientes dudosos…".into(),
                    warning: None,
                }
            }
            // A suspicion stays "not verified": neutral, never a traces-like warning.
            Some(AiOutcome::GlutenSuspected) => {
                return Summary {
                    tone: Tone::Unknown,
                    text: "No se ha podido confirmar si contiene gluten".into(),
                    warning: Some("La IA sospecha de un ingrediente · sin confirmar"),
                }
            }
            _ => {}
        }
    }
    // A search preview has no ingredients yet: say what to do instead of a bare "unknown".
    if status == GlutenStatus::Unknown
        && p.assessment
            .reasons
            .iter()
            .any(|r| r.code == "SEARCH_PREVIEW")
    {
        return Summary {
            tone: Tone::Unknown,
            text: "Sin analizar todavía · ábrelo".into(),
            warning: None,
        };
    }
    // A community-recorded "sin gluten" is information, not a verified declaration.
    if community_claim(&p.assessment) {
        return Summary {
            tone: Tone::Info,
            text: "«Sin gluten» según datos de la comunidad · sin verificar".into(),
            warning: Some("Compruébalo en tu envase"),
        };
    }
    // Old data is never presented as a current confirmation.
    if status == GlutenStatus::GlutenFree && not_current(p) {
        return Summary {
            tone: Tone::Info,
            text: "Sin gluten declarado · datos antiguos sin actualizar".into(),
            warning: Some("No es una confirmación actual: compruébalo en el envase"),
        };
    }
    // A declaration with the certification symbol says so in the answer itself.
    if status == GlutenStatus::GlutenFree
        && p.assessment.certification == Some(Certification::CertifiedSeal)
    {
        return Summary {
            tone: Tone::Safe,
            text: "Sin gluten · con sello de certificación".into(),
            warning: None,
        };
    }
    Summary {
        tone: status_tone(status),
        text: short_title(&p.assessment).into(),
        warning: None,
    }
}

/// One line for lists: what the analysis found, not just the status.
fn short_title(a: &Assessment) -> &'static str {
    use GlutenFinding as F;
    match (a.status, a.finding) {
        (GlutenStatus::ContainsGluten, Some(F::Contradictory)) => {
            "Contiene gluten · las fuentes no coinciden"
        }
        (GlutenStatus::ContainsGluten, Some(_)) => "Contiene gluten declarado",
        (GlutenStatus::MayContainTraces, Some(F::Contradictory)) => {
            "Puede contener trazas · las fuentes no coinciden"
        }
        (GlutenStatus::MayContainTraces, Some(_)) => "Trazas de gluten declaradas",
        (GlutenStatus::Unknown, Some(F::NoGlutenIdentified)) => {
            "No se identifican ingredientes con gluten · falta confirmar la etiqueta"
        }
        (GlutenStatus::Unknown, Some(F::AmbiguousIngredient)) => {
            "Ingrediente de origen dudoso · revisa la etiqueta"
        }
        (GlutenStatus::Unknown, Some(F::Contradictory)) => {
            "Las fuentes no coinciden · revisa la etiqueta"
        }
        (GlutenStatus::Unknown, Some(F::InsufficientInformation)) => {
            "No se ha podido confirmar si contiene gluten"
        }
        (status, _) => status_title(status),
    }
}

/// Title of the analysis card: the case the product falls in, in a full sentence.
pub fn analysis_title(a: &Assessment) -> &'static str {
    use GlutenFinding as F;
    match (a.status, a.finding) {
        (GlutenStatus::ContainsGluten, Some(F::Contradictory)) => {
            "Contiene gluten según una de las fuentes"
        }
        (GlutenStatus::ContainsGluten, _) => "Contiene gluten declarado",
        (GlutenStatus::MayContainTraces, Some(F::Contradictory)) => {
            "Puede contener trazas según una de las fuentes"
        }
        (GlutenStatus::MayContainTraces, _) => "La fuente declara que puede contener trazas",
        (GlutenStatus::GlutenFree, _) => "Sin gluten declarado",
        (GlutenStatus::Unknown, Some(F::NoGlutenIdentified)) => {
            "No se identifican ingredientes con gluten, pero falta confirmar la etiqueta"
        }
        (GlutenStatus::Unknown, Some(F::AmbiguousIngredient)) => {
            "Hay ingredientes cuyo origen no podemos confirmar"
        }
        (GlutenStatus::Unknown, Some(F::Contradictory)) => "Las fuentes no coinciden",
        (GlutenStatus::Unknown, _) => "No se ha podido confirmar si contiene gluten",
    }
}

/// How strong the evidence is, as a word and its explanation (never a percentage).
pub fn evidence_level(a: &Assessment) -> (&'static str, &'static str) {
    use GlutenFinding as F;
    match (a.status, a.finding) {
        (_, Some(F::Contradictory)) => (
            "Contradictoria",
            "Los datos no coinciden entre sí; mostramos lo más prudente.",
        ),
        (GlutenStatus::ContainsGluten, _) => (
            "Clara",
            "La fuente declara un ingrediente o un alérgeno con gluten.",
        ),
        (GlutenStatus::MayContainTraces, _) => (
            "Clara",
            "La fuente declara una advertencia de posibles trazas.",
        ),
        (GlutenStatus::GlutenFree, _) if a.provenance == Some(EvidenceSource::Community) => (
            "Declaración no verificada",
            "Un colaborador de una base de datos abierta registró la mención «sin gluten» del envase. Ni el fabricante ni SafeEat lo han confirmado.",
        ),
        (GlutenStatus::GlutenFree, _) if a.certification == Some(Certification::CertifiedSeal) => (
            "Declaración con sello",
            "Se declara «sin gluten» con el símbolo de la espiga barrada y ningún ingrediente lo contradice.",
        ),
        (GlutenStatus::GlutenFree, _) => (
            "Declaración sin contradicciones",
            "Se declara «sin gluten» y ningún ingrediente lo contradice. No es una certificación.",
        ),
        (GlutenStatus::Unknown, Some(F::NoGlutenIdentified)) => (
            "Incompleta",
            "Ningún ingrediente declara gluten, pero tampoco hay una declaración «sin gluten».",
        ),
        (GlutenStatus::Unknown, Some(F::AmbiguousIngredient)) => (
            "Insuficiente",
            "Algún ingrediente puede llevar gluten o no según su origen.",
        ),
        (GlutenStatus::Unknown, _) => (
            "Insuficiente",
            "Faltan datos o hay ingredientes que no reconocemos.",
        ),
    }
}

/// How the rules classified an ingredient, for the detail list.
pub fn class_label(class: IngredientClass) -> &'static str {
    match class {
        IngredientClass::GlutenSource => "Fuente de gluten",
        IngredientClass::LikelyGluten => "Suele llevar gluten",
        IngredientClass::Oats => "Avena: solo apta si se declara sin gluten",
        IngredientClass::NeedsOrigin => "Depende de su origen",
        IngredientClass::ContextDependent => "No indica gluten por sí solo",
        IngredientClass::LegalExemption => "Derivado exento por ley",
        IngredientClass::GenerallyCompatible => "Compatible en general",
        IngredientClass::NaturallyGlutenFree => "No procede de cereales con gluten",
        IngredientClass::Unrecognized => "No reconocido",
        IngredientClass::NotIngredient => "No es un ingrediente",
        IngredientClass::Other => "Sin clasificar",
    }
}

/// What the on-server AI said, from `ai_review` (server 2.0.3+) or the legacy `ai_check`.
#[derive(Debug, Clone, PartialEq)]
pub struct AiNote {
    pub outcome: AiOutcome,
    pub tone: Tone,
    pub title: &'static str,
    pub message: String,
    pub items: Vec<AiItemVerdict>,
    pub model: String,
}

/// Only for products the analysis left UNKNOWN; never green, never amber.
pub fn ai_note(p: &Product) -> Option<AiNote> {
    if p.assessment.status != GlutenStatus::Unknown || p.user_created {
        return None;
    }
    let (outcome, message, items, model) = match (&p.ai_review, &p.ai_check) {
        (Some(r), _) => (
            r.outcome,
            r.message.clone(),
            r.items.clone(),
            r.model.clone(),
        ),
        (None, Some(c)) => (
            match c.status {
                AiCheckStatus::Verified => AiOutcome::NoGlutenSeen,
                AiCheckStatus::Suspected => AiOutcome::GlutenSuspected,
                AiCheckStatus::Pending => AiOutcome::Pending,
            },
            c.message.clone(),
            Vec::new(),
            c.model.clone(),
        ),
        (None, None) => return None,
    };
    let (tone, title) = match outcome {
        AiOutcome::NoGlutenSeen => (Tone::Info, "Análisis orientativo por IA"),
        AiOutcome::GlutenSuspected => (
            Tone::Unknown,
            "La IA sospecha de un ingrediente · sin confirmar",
        ),
        AiOutcome::Undetermined => (Tone::Unknown, "La IA tampoco ha podido clasificarlo"),
        AiOutcome::ModelError => (Tone::Unknown, "No se ha podido consultar a la IA"),
        AiOutcome::Pending => (Tone::Unknown, "Consultando a la IA…"),
        AiOutcome::Other => return None,
    };
    Some(AiNote {
        outcome,
        tone,
        title,
        message,
        items,
        model,
    })
}

/// "fr" → "francés" for the «ver original» toggle.
pub fn language_name(code: &str) -> &'static str {
    match code {
        "en" => "inglés",
        "fr" => "francés",
        "de" => "alemán",
        "it" => "italiano",
        "pt" => "portugués",
        "nl" => "neerlandés",
        "ca" => "catalán",
        "pl" => "polaco",
        _ => "otro idioma",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use safeeat_contracts::{AiCheck, AiReview, PersonalVerdict, ProductReportCounts};

    fn product(status: &str) -> Product {
        serde_json::from_value(serde_json::json!({
            "id": "off-1", "name": "Test", "source": "OFF", "fetchedAt": "2026-01-01T00:00:00Z",
            "assessment": {"status": status, "confidence": 0.5, "reasons": [], "evidence": [], "engineVersion": "1"},
            "quality": {"score": null, "breakdown": [], "explanation": "", "version": "1"}
        }))
        .unwrap()
    }

    fn ai(status: AiCheckStatus) -> Option<AiCheck> {
        Some(AiCheck {
            status,
            message: String::new(),
            flagged: vec![],
            model: "m".into(),
            checked_at: None,
        })
    }

    fn with_finding(status: &str, finding: GlutenFinding) -> Product {
        let mut p = product(status);
        p.assessment.finding = Some(finding);
        p
    }

    fn review(outcome: AiOutcome) -> Option<AiReview> {
        Some(AiReview {
            outcome,
            message: "m".into(),
            items: vec![],
            model: "m".into(),
            rules_version: "r".into(),
            prompt_version: "p".into(),
            checked_at: None,
        })
    }

    /// Regression 8410044005008: no gluten warning, no "sin gluten", neutral.
    #[test]
    fn no_gluten_identified_is_neutral_and_says_what_is_missing() {
        let p = with_finding("UNKNOWN", GlutenFinding::NoGlutenIdentified);
        let s = summary(&p);
        assert_eq!(s.tone, Tone::Unknown);
        assert_eq!(
            s.text,
            "No se identifican ingredientes con gluten · falta confirmar la etiqueta"
        );
        let (level, why) = evidence_level(&p.assessment);
        assert_eq!(level, "Incompleta");
        assert!(!why.contains('%'));
        assert!(analysis_title(&p.assessment).contains("falta confirmar la etiqueta"));
    }

    #[test]
    fn the_full_analysis_groups_without_repeating() {
        let items: Vec<IngredientFinding> = serde_json::from_value(serde_json::json!([
            {"text": "Energético/Energ Grasas/Lipidos", "class": "NOT_INGREDIENT", "rule": "nutrition-table"},
            {"text": "sorbitol", "class": "GENERALLY_COMPATIBLE", "rule": "sweeteners"},
            {"text": "acesulfamo Ky sucralosa", "class": "UNRECOGNIZED"},
            {"text": "Contiene una fuente de fenilalanina", "class": "NOT_INGREDIENT", "rule": "consumer-warning"},
            {"text": "Sorbitol", "class": "GENERALLY_COMPATIBLE", "rule": "sweeteners"},
            {"text": "almidón", "class": "NEEDS_ORIGIN", "rule": "starch"}
        ]))
        .unwrap();
        let groups = ingredient_groups(&items);
        let titles: Vec<&str> = groups.iter().map(|g| g.title).collect();
        assert_eq!(
            titles,
            [
                "Requieren atención",
                "No reconocidos",
                "Reconocidos",
                "Advertencias del envase",
                "Texto apartado: no son ingredientes"
            ]
        );
        assert_eq!(groups[2].items.len(), 1, "sorbitol once");
        assert_eq!(groups[2].items[0].1, Some("Compatible en general"));
        assert_eq!(groups[4].items[0].1, None);
    }

    #[test]
    fn the_title_is_not_repeated_below_it() {
        let mut p = with_finding("UNKNOWN", GlutenFinding::InsufficientInformation);
        p.assessment.reasons = serde_json::from_value(serde_json::json!([
            {"code": "INSUFFICIENT_EVIDENCE", "message": "No se ha podido determinar la presencia de gluten."},
            {"code": "UNRECOGNIZED_INGREDIENTS", "message": "No reconocemos «x». Compruébalo en el envase."}
        ]))
        .unwrap();
        assert_eq!(
            shown_reasons(&p.assessment),
            ["No reconocemos «x». Compruébalo en el envase."]
        );
        assert_eq!(brief(&p, &summary(&p)), Some(UNDETERMINED_BRIEF));
        // Alone, it is the only explanation there is: kept.
        p.assessment.reasons.truncate(1);
        assert_eq!(shown_reasons(&p.assessment).len(), 1);
        // A declared answer needs no "cannot determine" line.
        let gluten = with_finding("CONTAINS_GLUTEN", GlutenFinding::DeclaredGluten);
        assert_eq!(brief(&gluten, &summary(&gluten)), None);
    }

    #[test]
    fn texts_say_which_case_it_is() {
        assert_eq!(
            summary(&with_finding(
                "UNKNOWN",
                GlutenFinding::InsufficientInformation
            ))
            .text,
            "No se ha podido confirmar si contiene gluten"
        );
        assert_eq!(
            summary(&with_finding(
                "CONTAINS_GLUTEN",
                GlutenFinding::DeclaredGluten
            ))
            .text,
            "Contiene gluten declarado"
        );
        let traces = with_finding("MAY_CONTAIN_TRACES", GlutenFinding::DeclaredTraces);
        assert_eq!(summary(&traces).tone, Tone::Caution);
        assert_eq!(summary(&traces).text, "Trazas de gluten declaradas");
        let both = with_finding("CONTAINS_GLUTEN", GlutenFinding::Contradictory);
        assert_eq!(summary(&both).tone, Tone::Danger);
        assert_eq!(evidence_level(&both.assessment).0, "Contradictoria");
        // A 2.0.2 server sends no finding: the old titles still work.
        assert_eq!(summary(&product("UNKNOWN")).text, "No verificado");
    }

    #[test]
    fn an_ai_suspicion_is_never_amber_nor_red() {
        let mut p = with_finding("UNKNOWN", GlutenFinding::InsufficientInformation);
        p.ai_review = review(AiOutcome::GlutenSuspected);
        let s = summary(&p);
        assert_eq!(s.tone, Tone::Unknown);
        assert_eq!(s.text, "No se ha podido confirmar si contiene gluten");
        assert!(s.warning.is_some_and(|w| w.contains("sin confirmar")));
        assert_eq!(ai_note(&p).unwrap().tone, Tone::Unknown);
        // The legacy field of an older server too.
        let mut p = product("UNKNOWN");
        p.ai_check = ai(AiCheckStatus::Suspected);
        assert_eq!(summary(&p).tone, Tone::Unknown);
        assert!(!summary(&p).text.contains("posible gluten"));
        // Errors and doubts fall back to what the rules say.
        for outcome in [
            AiOutcome::ModelError,
            AiOutcome::Undetermined,
            AiOutcome::Other,
        ] {
            let mut p = with_finding("UNKNOWN", GlutenFinding::InsufficientInformation);
            p.ai_review = review(outcome);
            assert_eq!(summary(&p).tone, Tone::Unknown);
            assert_eq!(
                summary(&p).text,
                "No se ha podido confirmar si contiene gluten"
            );
        }
    }

    #[test]
    fn ai_is_never_green_and_never_called_verified() {
        let mut p = product("UNKNOWN");
        p.ai_check = ai(AiCheckStatus::Verified);
        let s = summary(&p);
        assert_eq!(s.tone, Tone::Info);
        // Says what the AI saw, never that the product is gluten-free or verified.
        assert!(s.text.starts_with("La IA"));
        let lower = s.text.to_lowercase();
        assert!(!lower.contains("sin gluten") && !lower.contains("verificad"));
        assert!(s
            .warning
            .is_some_and(|w| w.contains("no es una certificación")));
    }

    #[test]
    fn only_a_declaration_or_her_own_verdict_is_green() {
        // Every combination of community and AI information on an UNKNOWN product.
        for free in [0, 1, 3] {
            for verified in [0, 1] {
                for status in [None, Some(AiCheckStatus::Verified)] {
                    let mut p = product("UNKNOWN");
                    p.gluten_reports = Some(ProductReportCounts {
                        gluten_free: free,
                        contains_gluten: 0,
                    });
                    p.verified_gluten_reports = Some(ProductReportCounts {
                        gluten_free: verified.min(free),
                        contains_gluten: 0,
                    });
                    p.ai_check = status.and_then(ai);
                    assert_ne!(summary(&p).tone, Tone::Safe, "{free} {verified} {status:?}");
                }
            }
        }
        let mut p = product("GLUTEN_FREE");
        assert_eq!(summary(&p).tone, Tone::Safe);
        assert_eq!(summary(&p).text, "Sin gluten declarado");
        p.assessment.certification = Some(Certification::CertifiedSeal);
        assert!(summary(&p).text.contains("sello"));
    }

    #[test]
    fn ai_never_overrides_the_engine_or_community_risk() {
        let mut p = product("CONTAINS_GLUTEN");
        p.ai_check = ai(AiCheckStatus::Verified);
        assert_eq!(summary(&p).tone, Tone::Danger);
        let mut p = product("UNKNOWN");
        p.ai_check = ai(AiCheckStatus::Verified);
        p.gluten_reports = Some(ProductReportCounts {
            gluten_free: 0,
            contains_gluten: 1,
        });
        assert_eq!(summary(&p).tone, Tone::Danger);
    }

    #[test]
    fn conflicts_and_unconfirmed_reports_are_not_green() {
        let mut p = product("GLUTEN_FREE");
        p.gluten_reports = Some(ProductReportCounts {
            gluten_free: 0,
            contains_gluten: 1,
        });
        assert_eq!(summary(&p).tone, Tone::Caution);
        let mut p = product("UNKNOWN");
        p.gluten_reports = Some(ProductReportCounts {
            gluten_free: 2,
            contains_gluten: 0,
        });
        assert_eq!(summary(&p).tone, Tone::Caution);
    }

    #[test]
    fn personal_verdict_wins_until_ingredients_change() {
        let mut p = product("CONTAINS_GLUTEN");
        p.my_verdict = Some(PersonalVerdict {
            verdict: VerdictValue::Safe,
            note: String::new(),
            updated_at: String::new(),
            ingredients_changed: false,
            evidence_changed: false,
        });
        assert_eq!(summary(&p).tone, Tone::Caution);
        p.assessment.status = GlutenStatus::Unknown;
        assert_eq!(summary(&p).tone, Tone::Safe);
        p.assessment.status = GlutenStatus::ContainsGluten;
        p.my_verdict.as_mut().unwrap().ingredients_changed = true;
        assert_eq!(summary(&p).tone, Tone::Danger);
        // Same ingredients, other allergen/trace information: pending review.
        p.my_verdict.as_mut().unwrap().ingredients_changed = false;
        p.my_verdict.as_mut().unwrap().evidence_changed = true;
        p.assessment.status = GlutenStatus::Unknown;
        assert_ne!(summary(&p).tone, Tone::Safe);
    }

    /// Regression (audit 2.3): a community "sin gluten" from Open Food Facts was shown
    /// green, like a verified declaration.
    #[test]
    fn a_community_claim_is_never_green() {
        let mut p = product("GLUTEN_FREE");
        p.assessment.provenance = Some(EvidenceSource::Community);
        let s = summary(&p);
        assert_eq!(s.tone, Tone::Info);
        assert!(s.text.contains("comunidad") && s.text.contains("sin verificar"));
        assert_eq!(evidence_level(&p.assessment).0, "Declaración no verificada");
        for source in [
            EvidenceSource::Manufacturer,
            EvidenceSource::ExternalDeclaration,
        ] {
            p.assessment.provenance = Some(source);
            assert_eq!(summary(&p).tone, Tone::Safe, "{source:?}");
        }
    }

    #[test]
    fn the_analysis_card_follows_the_same_rules() {
        let mut p = product("GLUTEN_FREE");
        assert_eq!(analysis_tone(&p), Tone::Safe);
        p.assessment.provenance = Some(EvidenceSource::Community);
        assert_eq!(analysis_tone(&p), Tone::Info);
        assert!(analysis_heading(&p).contains("sin verificar"));
        p.assessment.provenance = Some(EvidenceSource::Manufacturer);
        p.freshness = Some(Freshness::Outdated);
        assert_eq!(analysis_tone(&p), Tone::Info);
        assert!(analysis_heading(&p).contains("antiguos"));
        // Warnings keep their colour and title whatever the freshness or provenance.
        for (status, tone) in [
            ("CONTAINS_GLUTEN", Tone::Danger),
            ("MAY_CONTAIN_TRACES", Tone::Caution),
        ] {
            let mut p = product(status);
            p.freshness = Some(Freshness::Outdated);
            p.assessment.provenance = Some(EvidenceSource::Community);
            assert_eq!(analysis_tone(&p), tone, "{status}");
            assert_eq!(
                analysis_heading(&p),
                analysis_title(&p.assessment),
                "{status}"
            );
        }
        let mut p = product("UNKNOWN");
        p.assessment.provenance = Some(EvidenceSource::Community);
        assert_eq!(analysis_heading(&p), analysis_title(&p.assessment));
    }

    /// Regression (audit 2.5): a cached product kept a plain green badge while the
    /// provider was down.
    #[test]
    fn old_information_is_not_a_current_confirmation() {
        let mut p = product("GLUTEN_FREE");
        for (f, green) in [
            (Freshness::Current, true),
            (Freshness::Cached, true),
            (Freshness::Outdated, false),
            (Freshness::OldRules, false),
        ] {
            p.freshness = Some(f);
            assert_eq!(summary(&p).tone == Tone::Safe, green, "{f:?}");
            assert_eq!(
                freshness_notice(&p).is_some(),
                f != Freshness::Current,
                "{f:?}"
            );
        }
        // Never turned into a warning by an old cache.
        let mut p = product("UNKNOWN");
        p.freshness = Some(Freshness::Outdated);
        assert_ne!(summary(&p).tone, Tone::Danger);
        assert_ne!(summary(&p).tone, Tone::Caution);
    }
}
