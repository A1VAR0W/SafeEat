//! Product analysis: the core engine (`assess_gluten`, golden-tested) plus the rule
//! catalog. Decision order (docs/GLUTEN-ANALYSIS.md):
//!
//! 1. What the label declares (ingredients, allergens, traces, "sin gluten").
//! 2. Declarations and certifications that can be attributed to a source.
//! 3. This engine: core vocabulary + catalog (legal exemptions, explanations).
//! 4. Other providers, by provenance (handled by the server).
//! 5. AI, only for ingredients the catalog does not know (handled by the server, never
//!    part of the assessment).
//! 6. Anything else, or contradictions: UNKNOWN ("no verificado").
//!
//! The catalog can lower the public status in exactly two documented ways, both
//! towards UNKNOWN and never to GLUTEN_FREE:
//! - an ingredient covered by a legal exemption (Annex II.1 a-d) is not gluten evidence;
//! - a "sin gluten" that qualifies one ingredient is not a product declaration.

use safeeat_contracts::{
    Assessment, EvidenceKind, GlutenFinding, GlutenStatus, IngredientClass, IngredientFinding,
    Reason,
};

use crate::catalog::{self, classify, is_only_descriptors, severity, RULES_VERSION};
use crate::items::{self, Item};
use crate::{assess_gluten, normalize, GlutenInput};

/// Version of the analysis served by the API: engine layer + rule catalog. Cached
/// analyses with any other version are recomputed.
pub const ENGINE_VERSION: &str = "1.2.0+rules.2026.11.13";

/// Labelling regime of the source. Under EU law (1169/2011 art. 21) any ingredient made
/// from a gluten cereal must name it, which changes how vague words are read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Context {
    pub eu_labelling: bool,
}

impl Default for Context {
    fn default() -> Self {
        Context { eu_labelling: true }
    }
}

/// One classified ingredient (children for compound ingredients).
#[derive(Debug, Clone)]
pub struct ClassifiedItem {
    pub display: String,
    pub class: IngredientClass,
    pub rule: Option<&'static catalog::Rule>,
    pub claims_gluten_free: bool,
    pub span: (usize, usize),
    pub children: Vec<ClassifiedItem>,
}

impl ClassifiedItem {
    fn leaves<'a>(&'a self, out: &mut Vec<&'a ClassifiedItem>) {
        if self.children.is_empty() {
            out.push(self);
        } else {
            for child in &self.children {
                child.leaves(out);
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct Analysis {
    pub assessment: Assessment,
    pub items: Vec<ClassifiedItem>,
    /// What the product would be if its "sin gluten" claim were not accepted
    /// (community labels, scoped claims): finding and reasons for UNKNOWN.
    unknown_finding: GlutenFinding,
    unknown_reasons: Vec<Reason>,
    /// Ingredients worth asking the AI about (empty when a rule or a warning decides).
    pub ai_candidates: Vec<String>,
}

fn reason(code: &str, message: impl Into<String>) -> Reason {
    Reason {
        code: code.into(),
        message: message.into(),
    }
}

const NO_GLUTEN_IDENTIFIED: &str = "No se han identificado ingredientes que declaren gluten. Sin embargo, no disponemos de información suficiente para confirmar que el producto sea apto para celíacos.";
const CHECK_LABEL: &str = "Comprueba la información de alérgenos y posibles trazas del envase.";
const UNDETERMINED: &str = "No se ha podido determinar la presencia de gluten.";

fn classify_items(items: &[Item], eu: bool) -> Vec<ClassifiedItem> {
    items
        .iter()
        .filter_map(|item| {
            let children = classify_items(&item.children, eu);
            if children.is_empty() && is_only_descriptors(&item.matcher) {
                return None; // "antioxidante", "colorante": a function, not an ingredient
            }
            let own = classify(&item.matcher, eu);
            if children.is_empty() && own.matched.is_empty() && own.leftover.is_empty() {
                return None; // only a claim or descriptors ("Sin gluten.", "en polvo")
            }
            let worst_child = children
                .iter()
                .min_by_key(|c| std::cmp::Reverse(severity(c.class)));
            // A compound ingredient is what its listed ingredients are. Only a gluten
            // word in its own name ("galleta de trigo (...)") is kept.
            let (class, rule) = match worst_child {
                Some(child)
                    if !matches!(
                        own.class,
                        IngredientClass::GlutenSource | IngredientClass::Oats
                    ) || severity(child.class) > severity(own.class) =>
                {
                    (child.class, child.rule)
                }
                _ => (own.class, own.rule),
            };
            Some(ClassifiedItem {
                display: item.display.clone(),
                class,
                rule,
                claims_gluten_free: own.claims_gluten_free,
                span: item.span,
                children,
            })
        })
        .collect()
}

fn leaves(items: &[ClassifiedItem]) -> Vec<&ClassifiedItem> {
    let mut out = Vec::new();
    for item in items {
        item.leaves(&mut out);
    }
    out
}

fn lower_first(text: &str) -> String {
    let mut chars = text.chars();
    match chars.next() {
        Some(first) => first.to_lowercase().chain(chars).collect(),
        None => String::new(),
    }
}

fn join_names(names: &[String]) -> String {
    match names {
        [] => String::new(),
        [one] => one.clone(),
        [rest @ .., last] => format!("{} y {last}", rest.join(", ")),
    }
}

/// Words that may accompany a product-level claim ("Producto sin gluten. Apto para celíacos").
fn product_level_claim(clause: &str) -> bool {
    static FILLER: once_cell::sync::Lazy<regex::Regex> = once_cell::sync::Lazy::new(|| {
        regex::Regex::new(concat!(
            r"\b(?:sin gluten|gluten free|libre de gluten|no contiene gluten|does not contain gluten|",
            r"sans gluten|glutenfrei|senza glutine|sem gluten|sense gluten|glutenvrij|",
            r"producto|productos|este|esta|es|son|apto|apta|aptos|aptas|para|celiacos|celiac|coeliacs?|",
            r"suitable|for|garantizado|alimento|todo|totalmente|naturalmente|naturally|this|product|is|",
            r"ce|produit|est|questo|prodotto|e|dieses|produkt|ist|certificado|certified|declarado)\b",
        ))
        .expect("valid regex")
    });
    !FILLER
        .replace_all(clause, " ")
        .chars()
        .any(|c| c.is_ascii_alphabetic())
}

/// Analyse one product source.
pub fn analyze(input: &GlutenInput, context: Context) -> Analysis {
    let eu = context.eu_labelling;
    let ingredients = input
        .ingredients
        .as_deref()
        .map(str::trim)
        .filter(|t| !t.is_empty());
    let items = ingredients
        .map(|text| classify_items(&items::split(text), eu))
        .unwrap_or_default();
    let mut core = assess_gluten(input);
    let mut extra: Vec<Reason> = Vec::new();
    // Set when only an exemption separated a "sin gluten" claim from green.
    let mut exempt_claim = false;

    // Legal exemptions: an exempt ingredient is not gluten evidence. Re-run the core
    // engine without them; every other field and ingredient keeps its weight.
    let exempt: Vec<&ClassifiedItem> = leaves(&items)
        .into_iter()
        .filter(|i| i.class == IngredientClass::LegalExemption)
        .collect();
    if core.status == GlutenStatus::ContainsGluten && !exempt.is_empty() {
        let text = ingredients.unwrap_or_default();
        let mut masked = String::with_capacity(text.len());
        let mut cursor = 0;
        let mut spans: Vec<(usize, usize)> = exempt.iter().map(|i| i.span).collect();
        spans.sort_unstable();
        for (start, end) in spans {
            if start < cursor {
                continue;
            }
            masked.push_str(&text[cursor..start]);
            masked.push_str(" glucosa ");
            cursor = end;
        }
        masked.push_str(&text[cursor..]);
        let without = assess_gluten(&GlutenInput {
            ingredients: Some(masked),
            ..input.clone()
        });
        if without.status.risk() < core.status.risk() {
            // The exemption may resolve a contradiction, but never makes a product green.
            exempt_claim = without.status == GlutenStatus::GlutenFree;
            core = without;
            let names: Vec<String> = exempt.iter().map(|i| lower_first(&i.display)).collect();
            extra.push(reason(
                "LEGAL_EXEMPTION",
                format!(
                    "{} procede de un cereal con gluten, pero está exento de declararse como gluten (Reglamento (UE) 1169/2011, anexo II).",
                    capitalize(&join_names(&names))
                ),
            ));
        }
    }

    let all_leaves = leaves(&items);
    let worst = all_leaves
        .iter()
        .map(|i| i.class)
        .max_by_key(|c| severity(*c));
    let uncertain = core
        .evidence
        .iter()
        .any(|e| e.kind == EvidenceKind::Uncertainty);
    let contradictory = core
        .reasons
        .iter()
        .any(|r| r.code == "CONTRADICTORY_EVIDENCE");
    let unrecognized: Vec<String> = all_leaves
        .iter()
        .filter(|i| i.class == IngredientClass::Unrecognized)
        .map(|i| i.display.clone())
        .collect();

    // The case the product falls in if it is (or ends up) UNKNOWN.
    let unknown_finding = if contradictory {
        GlutenFinding::Contradictory
    } else if ingredients.is_none() || items.is_empty() {
        GlutenFinding::InsufficientInformation
    } else if uncertain
        || worst.is_some_and(|c| severity(c) >= severity(IngredientClass::NeedsOrigin))
    {
        GlutenFinding::AmbiguousIngredient
    } else if !unrecognized.is_empty() {
        GlutenFinding::InsufficientInformation
    } else {
        GlutenFinding::NoGlutenIdentified
    };
    let core_others: Vec<Reason> = core
        .reasons
        .iter()
        .filter(|r| r.code != "INSUFFICIENT_EVIDENCE" && r.code != "EXPLICIT_CLAIM")
        .cloned()
        .collect();
    let mut unknown_reasons = match unknown_finding {
        GlutenFinding::NoGlutenIdentified => {
            let mut notes: Vec<&str> = Vec::new();
            let mut seen = Vec::new();
            for leaf in &all_leaves {
                let Some(rule) = leaf.rule else { continue };
                let Some(note) = rule.note else { continue };
                // Exempt ingredients are explained once, in LEGAL_EXEMPTION.
                if seen.contains(&rule.id) || leaf.class == IngredientClass::LegalExemption {
                    continue;
                }
                seen.push(rule.id);
                notes.push(match rule.note_also {
                    Some(also) if !notes.is_empty() => also,
                    _ => note,
                });
            }
            notes.push(CHECK_LABEL);
            vec![
                reason("NO_GLUTEN_INGREDIENTS_IDENTIFIED", NO_GLUTEN_IDENTIFIED),
                reason("INGREDIENT_NOTES", notes.join(" ")),
            ]
        }
        GlutenFinding::AmbiguousIngredient => {
            let mut list = vec![reason("INSUFFICIENT_EVIDENCE", UNDETERMINED)];
            let notes: Vec<String> = all_leaves
                .iter()
                .filter(|i| severity(i.class) >= severity(IngredientClass::NeedsOrigin))
                .filter_map(|i| i.rule.and_then(|r| r.note).map(|n| (i, n)))
                .map(|(i, n)| format!("«{}»: {}", i.display, n))
                .collect();
            if !notes.is_empty() {
                list.push(reason("AMBIGUOUS_INGREDIENTS", notes.join(" ")));
            }
            list.push(reason("CHECK_LABEL", CHECK_LABEL));
            list
        }
        GlutenFinding::InsufficientInformation if ingredients.is_none() || items.is_empty() => {
            vec![reason(
                "INSUFFICIENT_EVIDENCE",
                "No disponemos de la lista de ingredientes: no se ha podido determinar la presencia de gluten. Revisa el envase.",
            )]
        }
        GlutenFinding::InsufficientInformation => vec![
            reason("INSUFFICIENT_EVIDENCE", UNDETERMINED),
            reason(
                "UNRECOGNIZED_INGREDIENTS",
                format!(
                    "No reconocemos {}. Compruébalo en el envase.",
                    join_names(
                        &unrecognized
                            .iter()
                            .map(|u| format!("«{u}»"))
                            .collect::<Vec<_>>()
                    )
                ),
            ),
        ],
        _ => vec![reason(
            "INSUFFICIENT_EVIDENCE",
            "No disponemos de información suficiente para confirmar que sea seguro.",
        )],
    };
    unknown_reasons.extend(extra.iter().cloned());
    unknown_reasons.extend(core_others.iter().cloned());

    let mut assessment = core.clone();
    let mut finding = match core.status {
        GlutenStatus::ContainsGluten if contradictory => GlutenFinding::Contradictory,
        GlutenStatus::ContainsGluten => GlutenFinding::DeclaredGluten,
        GlutenStatus::MayContainTraces if contradictory => GlutenFinding::Contradictory,
        GlutenStatus::MayContainTraces => GlutenFinding::DeclaredTraces,
        GlutenStatus::GlutenFree => GlutenFinding::GlutenFreeClaim,
        GlutenStatus::Unknown => unknown_finding,
    };

    // A "sin gluten" in the text must be about the product, and there must be a list
    // to contrast it with. Stricter than the core engine (docs/GLUTEN-ANALYSIS.md).
    if core.status == GlutenStatus::GlutenFree {
        let has_declaration = input
            .declaration
            .as_deref()
            .is_some_and(|d| !d.trim().is_empty());
        let scoped = !has_declaration
            && core
                .evidence
                .iter()
                .filter(|e| e.kind == EvidenceKind::Claim)
                .all(|e| !product_level_claim(&normalize(&e.text)));
        let downgrade = if exempt_claim {
            Some(reason(
                "EXEMPTION_WITH_CLAIM",
                "Se declara «sin gluten» e incluye un derivado de cereal exento por ley. SafeEat no lo marca como apto: compruébalo en el envase.",
            ))
        } else if scoped {
            Some(reason(
                "SCOPED_CLAIM",
                "La mención «sin gluten» se refiere a un ingrediente, no al producto: no es una declaración del producto.",
            ))
        } else if ingredients.is_none() {
            Some(reason(
                "CLAIM_WITHOUT_INGREDIENTS",
                "La fuente lo declara sin gluten, pero no tiene la lista de ingredientes para contrastarlo. Compruébalo en el envase.",
            ))
        } else {
            None
        };
        if let Some(why) = downgrade {
            assessment.status = GlutenStatus::Unknown;
            assessment.confidence = 0.0;
            assessment.reasons = unknown_reasons.clone();
            assessment.reasons.push(why.clone());
            unknown_reasons.push(why);
            finding = unknown_finding;
        }
    }
    if assessment.status == GlutenStatus::Unknown && core.status == GlutenStatus::Unknown {
        assessment.reasons = unknown_reasons.clone();
    } else if assessment.status != GlutenStatus::Unknown {
        assessment.reasons.extend(extra.iter().cloned());
    }
    if assessment.status == GlutenStatus::ContainsGluten {
        let named: Vec<String> = all_leaves
            .iter()
            .filter(|i| i.class == IngredientClass::GlutenSource)
            .map(|i| lower_first(&i.display))
            .collect();
        if !named.is_empty() {
            let message = if named.len() == 1 {
                format!("Ingrediente con gluten declarado: {}.", named[0])
            } else {
                format!(
                    "Ingredientes con gluten declarados: {}.",
                    join_names(&named)
                )
            };
            assessment.reasons.insert(
                1.min(assessment.reasons.len()),
                reason("GLUTEN_INGREDIENTS", message),
            );
        }
    }
    if assessment.status == GlutenStatus::MayContainTraces {
        if let Some(trace) = core.evidence.iter().find(|e| e.kind == EvidenceKind::Trace) {
            let message = match trace_tags(&trace.text) {
                Some(tags) => format!("Según {}, puede contener trazas de: {tags}.", input.source),
                None => format!("Según {}: «{}».", input.source, trace.text),
            };
            assessment.reasons.insert(
                1.min(assessment.reasons.len()),
                reason("TRACE_STATEMENT", message),
            );
        }
    }

    let ai_candidates = if assessment.status == GlutenStatus::Unknown
        && finding == GlutenFinding::InsufficientInformation
        && !unrecognized.is_empty()
    {
        unrecognized
    } else {
        Vec::new()
    };
    assessment.finding = Some(finding);
    assessment.rules_version = Some(RULES_VERSION.into());
    assessment.engine_version = ENGINE_VERSION.into();
    assessment.items = all_leaves
        .iter()
        .map(|i| IngredientFinding {
            text: i.display.clone(),
            class: i.class,
            rule: i.rule.map(|r| r.id.to_string()),
            note: i.rule.and_then(|r| r.note).map(str::to_string),
        })
        .collect();
    Analysis {
        assessment,
        items,
        unknown_finding,
        unknown_reasons,
        ai_candidates,
    }
}

/// "en:gluten, en:wheat" (Open Food Facts trace tags) → "gluten, trigo".
fn trace_tags(text: &str) -> Option<String> {
    if !text.split(',').all(|t| t.trim().contains(':')) {
        return None;
    }
    let names: Vec<String> = text
        .split(',')
        .map(|t| {
            let id = t.trim().split_once(':').map_or(t.trim(), |(_, id)| id);
            match id {
                "gluten" => "gluten".to_string(),
                "wheat" => "trigo".to_string(),
                "barley" => "cebada".to_string(),
                "rye" => "centeno".to_string(),
                "oats" => "avena".to_string(),
                "spelt" => "espelta".to_string(),
                other => other.replace('-', " "),
            }
        })
        .collect();
    Some(join_names(&names))
}

fn capitalize(text: &str) -> String {
    let mut chars = text.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

/// Severity of an UNKNOWN case, to keep the most cautious one across sources.
fn unknown_rank(finding: Option<GlutenFinding>) -> u8 {
    match finding {
        Some(GlutenFinding::Contradictory) => 4,
        Some(GlutenFinding::AmbiguousIngredient) => 3,
        Some(GlutenFinding::NoGlutenIdentified) => 1,
        _ => 2,
    }
}

impl Analysis {
    /// The same analysis with its "sin gluten" not accepted (a user's label is not a
    /// manufacturer's declaration). Never changes a warning.
    pub fn without_claim(mut self, code: &str, message: &str) -> Analysis {
        if self.assessment.status == GlutenStatus::GlutenFree {
            self.assessment.status = GlutenStatus::Unknown;
            self.assessment.confidence = 0.0;
            self.assessment.finding = Some(self.unknown_finding);
            self.assessment.reasons = self.unknown_reasons.clone();
            self.assessment.reasons.push(reason(code, message));
        }
        self
    }

    /// Other texts of the same source and formulation (the list in another language,
    /// a translation). They can only make the result more cautious: a gluten or trace
    /// warning in any of them wins, and a reassuring case needs all of them to agree.
    pub fn with_alternatives(mut self, others: Vec<(String, Analysis)>) -> Analysis {
        for (label, other) in others {
            let a = &self.assessment;
            let b = &other.assessment;
            let warns = matches!(
                b.status,
                GlutenStatus::ContainsGluten | GlutenStatus::MayContainTraces
            );
            let disagree_message = format!(
                "La lista de ingredientes {label} no permite la misma conclusión. Prevalece la precaución: revisa la etiqueta."
            );
            if warns && b.status.risk() > a.status.risk() {
                let mut adopted = other;
                adopted.assessment.finding = Some(GlutenFinding::Contradictory);
                adopted
                    .assessment
                    .reasons
                    .push(reason("TEXTS_DISAGREE", disagree_message));
                adopted.ai_candidates.clear();
                self = adopted;
            } else if a.status == GlutenStatus::Unknown
                && b.status == GlutenStatus::Unknown
                && unknown_rank(b.finding) > unknown_rank(a.finding)
            {
                let mut adopted = other;
                adopted
                    .assessment
                    .reasons
                    .push(reason("TEXTS_DISAGREE", disagree_message));
                adopted.ai_candidates.clear();
                // Keep the items of the list the user sees.
                adopted.assessment.items = std::mem::take(&mut self.assessment.items);
                self = adopted;
            }
        }
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Origin;

    fn provider(ingredients: &str) -> GlutenInput {
        GlutenInput {
            ingredients: Some(ingredients.into()),
            source: "Open Food Facts (datos comunitarios)".into(),
            origin: Some(Origin::Provider),
            ..Default::default()
        }
    }

    fn run(ingredients: &str) -> Assessment {
        analyze(&provider(ingredients), Context::default()).assessment
    }

    fn messages(a: &Assessment) -> Vec<String> {
        a.reasons.iter().map(|r| r.message.clone()).collect()
    }

    /// Regression: Kimbo «Patatas fritas sabor a Toreras», EAN 8410044005008.
    #[test]
    fn kimbo_8410044005008_is_not_verified_without_alarm() {
        let a = run(
            "Patatas seleccionadas (64%), aceite de oliva (35%), sal, dextrosa y aromas naturales.",
        );
        assert_eq!(a.status, GlutenStatus::Unknown);
        assert_eq!(a.finding, Some(GlutenFinding::NoGlutenIdentified));
        assert_eq!(a.reasons[0].message, NO_GLUTEN_IDENTIFIED);
        assert_eq!(
            a.reasons[1].message,
            "La dextrosa no representa por sí misma un motivo de alerta por gluten. Los aromas naturales tampoco demuestran la presencia de gluten. Comprueba la información de alérgenos y posibles trazas del envase."
        );
        assert_eq!(a.items.len(), 5);
        let joined = messages(&a).join(" ").to_lowercase();
        assert!(!joined.contains("posible gluten"));
        assert!(!joined.contains("puede contener"));
        let analysis = analyze(&provider("Patatas seleccionadas (64%), aceite de oliva (35%), sal, dextrosa y aromas naturales."), Context::default());
        assert!(
            analysis.ai_candidates.is_empty(),
            "rules decide: no AI call"
        );
    }

    #[test]
    fn declared_gluten_names_the_ingredient() {
        let a = run("Harina de trigo, agua, sal.");
        assert_eq!(a.status, GlutenStatus::ContainsGluten);
        assert_eq!(a.finding, Some(GlutenFinding::DeclaredGluten));
        assert!(messages(&a)
            .iter()
            .any(|m| m == "Ingrediente con gluten declarado: harina de trigo."));
    }

    #[test]
    fn legal_exemptions_do_not_count_as_gluten() {
        for text in [
            "Azúcar, jarabe de glucosa de trigo, gelatina, aromas.",
            "Azúcar, dextrosa (trigo), ácido cítrico.",
            "Leche, maltodextrina de trigo, cacao.",
            "Azúcar, jarabe de glucosa (de trigo), colorante: e 150a.",
        ] {
            let a = run(text);
            assert_eq!(a.status, GlutenStatus::Unknown, "{text}");
            assert_ne!(a.status, GlutenStatus::GlutenFree, "{text}");
            assert!(
                a.reasons.iter().any(|r| r.code == "LEGAL_EXEMPTION"),
                "{text}"
            );
        }
        // Another gluten source still decides.
        let a = run("Jarabe de glucosa de trigo, harina de trigo, azúcar.");
        assert_eq!(a.status, GlutenStatus::ContainsGluten);
        // An allergen field that says gluten is never cancelled by an exemption.
        let a = analyze(
            &GlutenInput {
                allergens: Some("en:gluten".into()),
                ..provider("Azúcar, jarabe de glucosa de trigo.")
            },
            Context::default(),
        )
        .assessment;
        assert_eq!(a.status, GlutenStatus::ContainsGluten);
        // Traces survive the exemption.
        let a = run("Jarabe de glucosa de trigo, azúcar. Puede contener trazas de cebada.");
        assert_eq!(a.status, GlutenStatus::MayContainTraces);
    }

    #[test]
    fn ambiguous_ingredients_never_get_the_reassuring_text() {
        for text in [
            "Harina, agua, sal.",
            "Pollo, pan rallado, huevo.",
            "Almidón, azúcar.",
            "Copos de avena (100%).",
            "Proteína vegetal hidrolizada, sal.",
        ] {
            let a = run(text);
            assert_eq!(a.status, GlutenStatus::Unknown, "{text}");
            assert_eq!(
                a.finding,
                Some(GlutenFinding::AmbiguousIngredient),
                "{text}"
            );
            assert!(
                !messages(&a).iter().any(|m| m == NO_GLUTEN_IDENTIFIED),
                "{text}"
            );
        }
    }

    #[test]
    fn unknown_words_are_insufficient_and_go_to_the_ai() {
        let analysis = analyze(&provider("Arroz, xqztina, sal."), Context::default());
        let a = &analysis.assessment;
        assert_eq!(a.finding, Some(GlutenFinding::InsufficientInformation));
        assert_eq!(analysis.ai_candidates, vec!["xqztina".to_string()]);
        // With an ambiguous ingredient too, the AI is not asked.
        let analysis = analyze(&provider("Harina, xqztina."), Context::default());
        assert!(analysis.ai_candidates.is_empty());
    }

    #[test]
    fn no_ingredients_is_insufficient() {
        let a = analyze(
            &GlutenInput {
                source: "x".into(),
                ..Default::default()
            },
            Context::default(),
        )
        .assessment;
        assert_eq!(a.finding, Some(GlutenFinding::InsufficientInformation));
        assert_eq!(a.status, GlutenStatus::Unknown);
    }

    #[test]
    fn claims_need_to_be_about_the_product() {
        let a = run("Copos de avena sin gluten, azúcar, aromas.");
        assert_eq!(a.status, GlutenStatus::Unknown);
        assert!(a.reasons.iter().any(|r| r.code == "SCOPED_CLAIM"));
        let a = run("Harina de arroz, azúcar. Sin gluten.");
        assert_eq!(a.status, GlutenStatus::GlutenFree);
        assert_eq!(a.finding, Some(GlutenFinding::GlutenFreeClaim));
        let a = run("Copos de avena. Producto sin gluten.");
        assert_eq!(a.status, GlutenStatus::GlutenFree);
        // A declaration from the label tags with no list to contrast is not enough.
        let a = analyze(
            &GlutenInput {
                declaration: Some("sin gluten".into()),
                source: "x".into(),
                origin: Some(Origin::Provider),
                ..Default::default()
            },
            Context::default(),
        )
        .assessment;
        assert_eq!(a.status, GlutenStatus::Unknown);
        assert!(a
            .reasons
            .iter()
            .any(|r| r.code == "CLAIM_WITHOUT_INGREDIENTS"));
    }

    #[test]
    fn declared_traces_quote_the_source() {
        let a = run("Arroz, sal. Puede contener trazas de trigo.");
        assert_eq!(a.status, GlutenStatus::MayContainTraces);
        assert_eq!(a.finding, Some(GlutenFinding::DeclaredTraces));
        assert!(messages(&a)
            .iter()
            .any(|m| m.contains("puede contener trazas de trigo")));
    }

    #[test]
    fn contradictions_are_flagged() {
        let a = run("Harina de trigo, azúcar. Sin gluten.");
        assert_eq!(a.status, GlutenStatus::ContainsGluten);
        assert_eq!(a.finding, Some(GlutenFinding::Contradictory));
    }

    #[test]
    fn alternatives_only_add_caution() {
        let es = analyze(&provider("Patatas, aceite, sal."), Context::default());
        let fr = analyze(
            &provider("Pommes de terre, farine de blé, sel."),
            Context::default(),
        );
        let merged = es
            .clone()
            .with_alternatives(vec![("en francés".into(), fr)]);
        assert_eq!(merged.assessment.status, GlutenStatus::ContainsGluten);
        assert_eq!(
            merged.assessment.finding,
            Some(GlutenFinding::Contradictory)
        );
        let vague = analyze(
            &provider("Pommes de terre, amidon, sel."),
            Context::default(),
        );
        let merged = es
            .clone()
            .with_alternatives(vec![("en francés".into(), vague)]);
        assert_eq!(
            merged.assessment.finding,
            Some(GlutenFinding::AmbiguousIngredient)
        );
        let same = analyze(
            &provider("Pommes de terre, huile, sel."),
            Context::default(),
        );
        let merged = es.with_alternatives(vec![("en francés".into(), same)]);
        assert_eq!(
            merged.assessment.finding,
            Some(GlutenFinding::NoGlutenIdentified)
        );
    }

    #[test]
    fn community_claim_is_not_accepted() {
        let a = analyze(&provider("Arroz, sal. Sin gluten."), Context::default())
            .without_claim("COMMUNITY_CLAIM_UNVERIFIED", "x");
        assert_eq!(a.assessment.status, GlutenStatus::Unknown);
        assert_eq!(
            a.assessment.finding,
            Some(GlutenFinding::NoGlutenIdentified)
        );
    }

    #[test]
    fn outside_eu_flavourings_are_ambiguous() {
        let a = analyze(
            &provider("Potatoes, sunflower oil, salt, natural flavors."),
            Context {
                eu_labelling: false,
            },
        )
        .assessment;
        assert_eq!(a.finding, Some(GlutenFinding::AmbiguousIngredient));
        let a = analyze(
            &provider("Potatoes, sunflower oil, salt, natural flavors."),
            Context::default(),
        )
        .assessment;
        assert_eq!(a.finding, Some(GlutenFinding::NoGlutenIdentified));
    }
}
