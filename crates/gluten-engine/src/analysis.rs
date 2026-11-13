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
    Assessment, EvidenceKind, EvidenceSource, GlutenFinding, GlutenStatus, IngredientClass,
    IngredientFinding, Reason,
};

/// Provenance of an assessment whose declarations come from `declared_by`: a declared
/// status (gluten, traces, "sin gluten") is as strong as its source; an UNKNOWN is
/// SafeEat's own reading, or nothing at all.
pub fn provenance(a: &Assessment, declared_by: EvidenceSource) -> EvidenceSource {
    match (a.status, a.finding) {
        (
            GlutenStatus::Unknown,
            None | Some(GlutenFinding::InsufficientInformation) | Some(GlutenFinding::Other),
        ) => EvidenceSource::Insufficient,
        (GlutenStatus::Unknown, _) => EvidenceSource::Automatic,
        _ => declared_by,
    }
}

use crate::catalog::{self, classify, is_only_descriptors, set_aside, severity, RULES_VERSION};
use crate::items::{self, Item};
use crate::{assess_gluten, normalize, GlutenInput};

/// Version of the analysis served by the API: engine layer + rule catalog. Cached
/// analyses with any other version are recomputed.
pub const ENGINE_VERSION: &str = "1.2.3+rules.2026.11.13.2";

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
const INCOMPLETE_LIST: &str = "La lista de ingredientes es demasiado larga o está incompleta: no se han podido revisar todos los ingredientes uno a uno. Revisa el envase.";
const UNBALANCED_LIST: &str = "La lista tiene paréntesis sin cerrar: la composición de algún ingrediente compuesto puede no haberse interpretado bien.";
const MIXED_TEXT: &str = "El texto de ingredientes está mezclado con la tabla nutricional u otra información del envase: puede que la lista no esté completa o no se haya leído bien. Revisa el envase.";
const NO_LIST_FOUND: &str = "El texto disponible no contiene una lista de ingredientes reconocible (parece información nutricional o advertencias). Revisa el envase.";
/// Unknown ingredients named in the explanation; the rest are counted.
const UNRECOGNIZED_SHOWN: usize = 3;
/// Longest name quoted in the explanation (characters).
const NAME_CHARS: usize = 32;

/// "acesulfamo Ky sucralosa" as written, or its first words and "…" when too long.
fn short_name(name: &str) -> String {
    let name = name.trim_matches(|c: char| c.is_whitespace() || "-.,;:".contains(c));
    if name.chars().count() <= NAME_CHARS {
        return name.to_string();
    }
    let mut out = String::new();
    for word in name.split_whitespace() {
        if out.chars().count() + word.chars().count() + 1 > NAME_CHARS {
            break;
        }
        if !out.is_empty() {
            out.push(' ');
        }
        out.push_str(word);
    }
    if out.is_empty() {
        out = name.chars().take(NAME_CHARS).collect();
    }
    format!("{out}…")
}

/// "No reconocemos «a», «b» y «c»" or, for a long list, the count and three examples.
fn unrecognized_message(names: &[String]) -> String {
    let mut unique: Vec<String> = Vec::new();
    for name in names {
        let short = short_name(name);
        if !short.is_empty() && !unique.iter().any(|u| u.eq_ignore_ascii_case(&short)) {
            unique.push(short);
        }
    }
    let quoted =
        |list: &[String]| join_names(&list.iter().map(|u| format!("«{u}»")).collect::<Vec<_>>());
    if unique.len() <= UNRECOGNIZED_SHOWN {
        format!(
            "No reconocemos {}. Compruébalo en el envase.",
            quoted(&unique)
        )
    } else {
        format!(
            "No reconocemos {} ingredientes, entre ellos {}. Compruébalo en el envase.",
            unique.len(),
            quoted(&unique[..UNRECOGNIZED_SHOWN])
        )
    }
}

fn classify_items(items: &[Item], eu: bool) -> Vec<ClassifiedItem> {
    items
        .iter()
        .filter_map(|item| {
            let mut children = classify_items(&item.children, eu);
            if children.is_empty() && is_only_descriptors(&item.matcher) {
                return None; // "antioxidante", "colorante": a function, not an ingredient
            }
            let own = classify(&item.matcher, eu);
            if children.is_empty() && own.matched.is_empty() && own.leftover.is_empty() {
                return None; // only a claim or descriptors ("Sin gluten.", "en polvo")
            }
            if children.is_empty() {
                if let Some(rule) = set_aside(&item.matcher, &own) {
                    return Some(ClassifiedItem {
                        display: item.display.clone(),
                        class: IngredientClass::NotIngredient,
                        rule: Some(rule),
                        claims_gluten_free: own.claims_gluten_free,
                        span: item.span,
                        children,
                    });
                }
            }
            // "goma base (con antioxidante: ...)": the parenthesis is an addition, so
            // the item's own name keeps its weight.
            let own_counts = matches!(
                own.class,
                IngredientClass::GlutenSource | IngredientClass::Oats
            ) || (item.partial
                && (!own.matched.is_empty() || !own.leftover.is_empty()));
            if item.partial && own_counts && !children.is_empty() {
                // Listed as an ingredient of its own, so it is never hidden behind the
                // additive in its parenthesis.
                children.insert(
                    0,
                    ClassifiedItem {
                        display: item.display.clone(),
                        class: own.class,
                        rule: own.rule,
                        claims_gluten_free: own.claims_gluten_free,
                        // The name only: masking an exempt name must not mask the
                        // ingredients in its parenthesis.
                        span: item.name_span,
                        children: Vec::new(),
                    },
                );
            }
            let worst_child = children
                .iter()
                .min_by_key(|c| std::cmp::Reverse(severity(c.class)));
            // A compound ingredient is what its listed ingredients are. Only a gluten
            // word in its own name ("galleta de trigo (...)") is kept.
            let (class, rule) = match worst_child {
                Some(child) if !own_counts || severity(child.class) > severity(own.class) => {
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
    let parsed = ingredients.map(items::parse).unwrap_or_default();
    let items = classify_items(&parsed.items, eu);
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
    // Text set aside as not being ingredients. A nutrition table in the ingredient
    // field means the list itself may be cut or misread: it never reassures.
    let real_items = all_leaves
        .iter()
        .any(|i| i.class != IngredientClass::NotIngredient);
    // Also a warning with words of its own: what else it says is unknown.
    let mixed_with_table = all_leaves.iter().any(|i| {
        i.rule.is_some_and(|r| {
            r.id == catalog::NUTRITION_TABLE.id || r.id == catalog::UNCLEAR_WARNING.id
        })
    });

    // Text read from a photo that she has not checked against the label: OCR can drop
    // a whole line ("trigo" included), so recognising every word read reassures nothing.
    let unreviewed_ocr =
        input.origin == Some(crate::Origin::Ocr) && !input.reviewed.unwrap_or(false);

    // The case the product falls in if it is (or ends up) UNKNOWN.
    let unknown_finding = if contradictory {
        GlutenFinding::Contradictory
    } else if ingredients.is_none() || items.is_empty() || !real_items {
        GlutenFinding::InsufficientInformation
    } else if uncertain
        || worst.is_some_and(|c| severity(c) >= severity(IngredientClass::NeedsOrigin))
    {
        GlutenFinding::AmbiguousIngredient
    } else if !unrecognized.is_empty() || parsed.incomplete() || mixed_with_table || unreviewed_ocr
    {
        // Ingredients that were never classified one by one leave nothing to reassure.
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
        GlutenFinding::InsufficientInformation if !real_items => {
            vec![reason("INSUFFICIENT_EVIDENCE", NO_LIST_FOUND)]
        }
        GlutenFinding::InsufficientInformation if unrecognized.is_empty() => {
            let mut list = vec![reason("INSUFFICIENT_EVIDENCE", UNDETERMINED)];
            if parsed.incomplete() {
                list.push(reason("INCOMPLETE_LIST", INCOMPLETE_LIST));
            }
            list
        }
        GlutenFinding::InsufficientInformation => vec![
            reason("INSUFFICIENT_EVIDENCE", UNDETERMINED),
            reason(
                "UNRECOGNIZED_INGREDIENTS",
                unrecognized_message(&unrecognized),
            ),
        ],
        _ => vec![reason(
            "INSUFFICIENT_EVIDENCE",
            "No disponemos de información suficiente para confirmar que sea seguro.",
        )],
    };
    if parsed.incomplete() && !unrecognized.is_empty() {
        unknown_reasons.push(reason("INCOMPLETE_LIST", INCOMPLETE_LIST));
    }
    if parsed.unbalanced {
        unknown_reasons.push(reason("UNBALANCED_LIST", UNBALANCED_LIST));
    }
    if mixed_with_table && real_items {
        unknown_reasons.push(reason("MIXED_TEXT", MIXED_TEXT));
    }
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

    /// Other texts of the same record: lists in other languages, a machine translation.
    /// Reconciled one by one by provenance (`reconcile`); they never make the result
    /// more favourable.
    pub fn with_alternatives(mut self, others: Vec<Alternative>) -> Analysis {
        for other in others {
            self = self.reconcile(other);
        }
        self
    }

    /// Policy (docs/GLUTEN-ANALYSIS.md, "Varias listas del mismo producto"):
    ///
    /// | this list        | other list            | other is evidence¹ | result                          |
    /// |------------------|-----------------------|--------------------|---------------------------------|
    /// | gluten           | anything              | -                  | gluten                          |
    /// | free/unknown/traces | warning, more severe | yes             | the warning, contradictory      |
    /// | free/unknown     | warning               | no (translation)   | UNKNOWN, contradictory          |
    /// | traces           | gluten                | no (translation)   | traces + note                   |
    /// | free             | unknown/free-less     | yes                | UNKNOWN, contradictory          |
    /// | free             | unknown               | no (translation)   | free + note (original decides)  |
    /// | unknown          | more cautious unknown | any                | that case                       |
    /// | unknown/traces   | free                  | any                | unchanged (never more favourable)|
    ///
    /// ¹ Another list of the record is label text in its own right. A machine
    /// translation is evidence only when the engine cannot read the original language.
    fn reconcile(mut self, other: Alternative) -> Analysis {
        use GlutenStatus::{ContainsGluten, GlutenFree, MayContainTraces, Unknown};
        let label = other.label.clone();
        let evidence = other.is_evidence();
        let a = self.assessment.status;
        let b = other.analysis.assessment.status;
        let b_warns = matches!(b, ContainsGluten | MayContainTraces);
        let disagree = || {
            reason(
                "TEXTS_DISAGREE",
                format!("La lista de ingredientes {label} no permite la misma conclusión. Prevalece la precaución: revisa la etiqueta."),
            )
        };
        match (a, b) {
            (ContainsGluten, _) => self,
            _ if b_warns && b.risk() > a.risk() && evidence => {
                let mut adopted = other.analysis;
                let translated = matches!(other.kind, AlternativeKind::Translation { .. });
                if a == GlutenFree || !translated {
                    adopted.assessment.finding = Some(GlutenFinding::Contradictory);
                }
                adopted.assessment.reasons.push(if translated {
                    reason(
                        "TRANSLATION_EVIDENCE",
                        format!("Detectado en la lista {label}: SafeEat no reconoce el idioma original. Compruébalo en el envase."),
                    )
                } else {
                    disagree()
                });
                adopted.ai_candidates.clear();
                adopted
            }
            (GlutenFree | Unknown, ContainsGluten | MayContainTraces) => {
                // Only a translation says so: perhaps a translation error, never proof of
                // gluten, but never green either.
                self.into_unknown(
                    GlutenFinding::Contradictory,
                    reason(
                        "TRANSLATION_DISAGREES",
                        format!("La lista {label} menciona gluten o trazas y la original no. Puede ser un error de traducción: compruébalo en el envase."),
                    ),
                )
            }
            (MayContainTraces, ContainsGluten) => {
                self.assessment.reasons.push(reason(
                    "TRANSLATION_DISAGREES",
                    format!("La lista {label} menciona gluten como ingrediente; la original solo trazas. Compruébalo en el envase."),
                ));
                self
            }
            (GlutenFree, Unknown) if evidence => {
                let mut unknown = other.analysis;
                let why = disagree();
                unknown.assessment.finding = Some(GlutenFinding::Contradictory);
                unknown.assessment.reasons.push(why.clone());
                unknown.assessment.items = std::mem::take(&mut self.assessment.items);
                unknown.ai_candidates.clear();
                unknown.unknown_finding = GlutenFinding::Contradictory;
                unknown.unknown_reasons.push(why);
                unknown
            }
            (GlutenFree, Unknown) => {
                self.assessment.reasons.push(reason(
                    "TRANSLATION_NOT_DECISIVE",
                    format!("La lista {label} no permite reconocer todos los ingredientes; la conclusión se basa en la lista original."),
                ));
                self
            }
            (Unknown, Unknown)
                if unknown_rank(other.analysis.assessment.finding)
                    > unknown_rank(self.assessment.finding) =>
            {
                let mut adopted = other.analysis;
                adopted.assessment.reasons.push(disagree());
                adopted.ai_candidates.clear();
                // Keep the items of the list the user sees.
                adopted.assessment.items = std::mem::take(&mut self.assessment.items);
                adopted
            }
            _ => self,
        }
    }

    fn into_unknown(mut self, finding: GlutenFinding, why: Reason) -> Analysis {
        self.assessment.status = GlutenStatus::Unknown;
        self.assessment.confidence = 0.0;
        self.assessment.certification = None;
        self.assessment.finding = Some(finding);
        self.assessment.reasons = self.unknown_reasons.clone();
        self.assessment.reasons.push(why.clone());
        self.unknown_finding = finding;
        self.unknown_reasons.push(why);
        self.ai_candidates.clear();
        self
    }
}

/// Where another text of the same product comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AlternativeKind {
    /// Another list of the same record (another language): label text in its own right.
    SourceList,
    /// A machine translation of this list. `original_readable`: the engine supports the
    /// original language (Spanish, English), so the original decides.
    Translation { original_readable: bool },
}

/// Another text of the same product, analysed.
#[derive(Debug, Clone)]
pub struct Alternative {
    /// "en francés", "traducida al español"...
    pub label: String,
    pub kind: AlternativeKind,
    pub analysis: Analysis,
}

impl Alternative {
    fn is_evidence(&self) -> bool {
        match self.kind {
            AlternativeKind::SourceList => true,
            AlternativeKind::Translation { original_readable } => !original_readable,
        }
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

    /// Regression: Chic-kles Fresa Morango (Hacendado), EAN 8402001044311. Open Food
    /// Facts holds a bilingual OCR mixed with the nutrition table (fetched 2026-11-13).
    #[test]
    fn chickles_8402001044311_stays_unknown_and_reads_cleanly() {
        let text = include_str!("../tests/fixtures/chickles-8402001044311.txt").trim();
        let analysis = analyze(&provider(text), Context::default());
        let a = &analysis.assessment;
        assert_eq!(a.status, GlutenStatus::Unknown);
        assert_eq!(a.finding, Some(GlutenFinding::InsufficientInformation));
        let codes: Vec<&str> = a.reasons.iter().map(|r| r.code.as_str()).collect();
        assert!(codes.contains(&"MIXED_TEXT"), "{codes:?}");
        let unrecognized = a
            .reasons
            .iter()
            .find(|r| r.code == "UNRECOGNIZED_INGREDIENTS")
            .unwrap();
        // One short sentence, not the whole label.
        assert!(
            unrecognized.message.chars().count() < 220,
            "{}",
            unrecognized.message
        );
        assert!(unrecognized.message.contains("ingredientes, entre ellos"));
        let class_of = |name: &str| {
            a.items
                .iter()
                .find(|i| i.text == name)
                .unwrap_or_else(|| panic!("{name} missing"))
                .class
        };
        for known in [
            "maltitoles",
            "glicerina",
            "glicerol",
            "cera de carnauba",
            "maltitóis",
            "extrato rico em tocoferóis",
        ] {
            assert_eq!(
                class_of(known),
                IngredientClass::GenerallyCompatible,
                "{known}"
            );
        }
        assert_eq!(class_of("goma-base"), IngredientClass::ContextDependent);
        // The compound's own name is listed, not hidden behind its antioxidant.
        assert_eq!(
            class_of("goma base de las cuales dos ma"),
            IngredientClass::Unrecognized
        );
        for aside in [
            "Energético/Energ Grasas/Lipidos Edulcorantes",
            "Contiene una fuente de fenilalanina",
            "Un consumo excesivo puede producir efectos laxantes",
        ] {
            assert_eq!(class_of(aside), IngredientClass::NotIngredient, "{aside}");
        }
        // The nutrition table and the warnings are never sent to the AI.
        assert!(analysis
            .ai_candidates
            .iter()
            .all(|c| !c.contains("fenilalanina") && !c.contains("Lipidos")));
    }

    #[test]
    fn a_table_alone_is_not_an_ingredient_list() {
        let a = run("Valor energético 1000 kJ / 239 kcal. Grasas 0 g, de las cuales saturadas 0 g. Hidratos de carbono 95 g, de los cuales azúcares 0 g, polialcoholes 95 g.");
        assert_eq!(a.status, GlutenStatus::Unknown);
        assert_eq!(a.finding, Some(GlutenFinding::InsufficientInformation));
        assert!(a
            .reasons
            .iter()
            .all(|r| r.code != "NO_GLUTEN_INGREDIENTS_IDENTIFIED"));
    }

    #[test]
    fn a_table_mixed_in_never_reassures() {
        // Every real ingredient is known, but the text is mixed with the table.
        let a = run(
            "Edulcorantes: maltitol, sorbitol. Valor energético kJ kcal grasas lípidos saturadas.",
        );
        assert_eq!(a.status, GlutenStatus::Unknown);
        assert_eq!(a.finding, Some(GlutenFinding::InsufficientInformation));
        assert!(a.reasons.iter().any(|r| r.code == "MIXED_TEXT"));
        // A warning alone is normal on a label and changes nothing.
        let a = run(
            "Edulcorantes: maltitol, sorbitol, aspartamo. Contiene una fuente de fenilalanina.",
        );
        assert_eq!(a.status, GlutenStatus::Unknown);
        assert_eq!(a.finding, Some(GlutenFinding::NoGlutenIdentified));
    }

    #[test]
    fn an_addition_in_parentheses_does_not_hide_the_ingredient() {
        let a = run("Azúcar, zorvex (con antioxidante: extracto rico en tocoferoles), sal.");
        assert_eq!(a.finding, Some(GlutenFinding::InsufficientInformation));
        assert!(a
            .items
            .iter()
            .any(|i| i.text.starts_with("zorvex") && i.class == IngredientClass::Unrecognized));
        // A real composition still decides ("galleta (harina de arroz, azúcar)").
        let a = run("Galleta (harina de arroz, azúcar), sal.");
        assert!(a.items.iter().all(|i| !i.text.starts_with("Galleta")));
    }

    #[test]
    fn an_exempt_name_does_not_mask_its_parenthesis() {
        let a = run("Jarabe de glucosa de trigo (con harina de cebada), sal.");
        assert_eq!(a.status, GlutenStatus::ContainsGluten);
        // The exemption still works on its own.
        let a = run("Jarabe de glucosa de trigo, sal.");
        assert_ne!(a.status, GlutenStatus::ContainsGluten);
    }

    #[test]
    fn a_warning_with_unknown_words_never_reassures() {
        let a =
            run("Edulcorantes: maltitol, sorbitol. Contiene una fuente de fenilalanina zorvex.");
        assert_eq!(a.status, GlutenStatus::Unknown);
        assert_eq!(a.finding, Some(GlutenFinding::InsufficientInformation));
        assert!(a.reasons.iter().any(|r| r.code == "MIXED_TEXT"));
    }

    #[test]
    fn an_unchecked_photo_reading_never_reassures() {
        let text = "Arroz, aceite de girasol, sal.";
        let read = |reviewed| {
            analyze(
                &GlutenInput {
                    ingredients: Some(text.into()),
                    source: "Foto".into(),
                    origin: Some(Origin::Ocr),
                    reviewed: Some(reviewed),
                    ..Default::default()
                },
                Context::default(),
            )
            .assessment
        };
        let unchecked = read(false);
        assert_eq!(unchecked.status, GlutenStatus::Unknown);
        assert_eq!(
            unchecked.finding,
            Some(GlutenFinding::InsufficientInformation)
        );
        assert!(unchecked.reasons.iter().any(|r| r.code == "OCR_LIMITATION"));
        // Checked by her against the label, it reads like a typed list.
        assert_eq!(read(true).finding, Some(GlutenFinding::NoGlutenIdentified));
        // A "sin gluten" read from a photo never turns green unchecked.
        let claim = analyze(
            &GlutenInput {
                ingredients: Some("Arroz, sal. Sin gluten.".into()),
                source: "Foto".into(),
                origin: Some(Origin::Ocr),
                reviewed: Some(false),
                ..Default::default()
            },
            Context::default(),
        )
        .assessment;
        assert_ne!(claim.status, GlutenStatus::GlutenFree);
        // Gluten read from a photo is still gluten.
        let wheat = analyze(
            &GlutenInput {
                ingredients: Some("Harina de trigo, sal.".into()),
                source: "Foto".into(),
                origin: Some(Origin::Ocr),
                ..Default::default()
            },
            Context::default(),
        )
        .assessment;
        assert_eq!(wheat.status, GlutenStatus::ContainsGluten);
    }

    #[test]
    fn a_flavouring_is_still_listed() {
        let a = run("Agua, aromatizantes.");
        assert!(a
            .items
            .iter()
            .any(|i| i.text == "aromatizantes" && i.rule.as_deref() == Some("flavouring")));
    }

    #[test]
    fn a_long_unknown_list_is_summed_up() {
        let message = |text: &str| {
            run(text)
                .reasons
                .into_iter()
                .find(|r| r.code == "UNRECOGNIZED_INGREDIENTS")
                .unwrap()
                .message
        };
        assert_eq!(
            message("Alfa, beta, gamma, delta, epsilon, zeta."),
            "No reconocemos 6 ingredientes, entre ellos «Alfa», «beta» y «gamma». Compruébalo en el envase."
        );
        assert_eq!(
            message("Alfa, beta."),
            "No reconocemos «Alfa» y «beta». Compruébalo en el envase."
        );
        assert_eq!(
            short_name(&"palabra ".repeat(10)),
            "palabra palabra palabra palabra…"
        );
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

    fn alt(label: &str, kind: AlternativeKind, text: &str) -> Alternative {
        Alternative {
            label: label.into(),
            kind,
            analysis: analyze(&provider(text), Context::default()),
        }
    }

    const LIST: AlternativeKind = AlternativeKind::SourceList;
    const READABLE: AlternativeKind = AlternativeKind::Translation {
        original_readable: true,
    };
    const UNREADABLE: AlternativeKind = AlternativeKind::Translation {
        original_readable: false,
    };

    fn merged(primary: &str, kind: AlternativeKind, other: &str) -> Assessment {
        analyze(&provider(primary), Context::default())
            .with_alternatives(vec![alt("en otro idioma", kind, other)])
            .assessment
    }

    fn codes(a: &Assessment) -> Vec<&str> {
        a.reasons.iter().map(|r| r.code.as_str()).collect()
    }

    const FREE: &str = "Arroz, sal. Producto sin gluten.";
    const NONE: &str = "Patatas, aceite, sal.";
    const VAGUE: &str = "Patatas, almidón, sal.";
    const WHEAT: &str = "Patatas, harina de trigo, sal.";
    const TRACES: &str = "Patatas, aceite, sal. Puede contener trazas de trigo.";

    #[test]
    fn alternatives_only_add_caution() {
        let a = merged(NONE, LIST, "Pommes de terre, farine de blé, sel.");
        assert_eq!(a.status, GlutenStatus::ContainsGluten);
        assert_eq!(a.finding, Some(GlutenFinding::Contradictory));
        let a = merged(NONE, LIST, "Pommes de terre, amidon, sel.");
        assert_eq!(a.finding, Some(GlutenFinding::AmbiguousIngredient));
        let a = merged(NONE, LIST, "Pommes de terre, huile, sel.");
        assert_eq!(a.finding, Some(GlutenFinding::NoGlutenIdentified));
    }

    /// Every status pair, for each provenance: never more favourable than the primary,
    /// a primary warning never lost, and label-text warnings adopted.
    #[test]
    fn reconciliation_matrix() {
        use GlutenStatus::*;
        let texts = [FREE, NONE, VAGUE, WHEAT, TRACES];
        for kind in [LIST, READABLE, UNREADABLE] {
            for primary in texts {
                for other in texts {
                    let base = analyze(&provider(primary), Context::default()).assessment;
                    let out = merged(primary, kind, other);
                    let ctx = format!("{kind:?}: «{primary}» + «{other}» → {:?}", out.status);
                    if base.status != GlutenFree {
                        assert_ne!(out.status, GlutenFree, "{ctx}");
                    }
                    if base.status == ContainsGluten {
                        assert_eq!(out.status, ContainsGluten, "{ctx}");
                    }
                    if base.status == MayContainTraces {
                        assert!(out.status.risk() >= MayContainTraces.risk(), "{ctx}");
                    }
                    let other_status = analyze(&provider(other), Context::default())
                        .assessment
                        .status;
                    if out.status == GlutenFree {
                        assert!(other_status == GlutenFree || kind == READABLE, "{ctx}");
                    }
                    if matches!(other_status, ContainsGluten | MayContainTraces)
                        && base.status != ContainsGluten
                    {
                        if kind == READABLE {
                            assert_ne!(out.status, GlutenFree, "{ctx}");
                        } else {
                            assert!(out.status.risk() >= other_status.risk(), "{ctx}");
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn a_free_list_with_an_unknown_other_list_is_not_green() {
        let a = merged(FREE, LIST, VAGUE);
        assert_eq!(a.status, GlutenStatus::Unknown);
        assert_eq!(a.finding, Some(GlutenFinding::Contradictory));
        assert!(codes(&a).contains(&"TEXTS_DISAGREE"));
        assert_eq!(a.certification, None);
    }

    #[test]
    fn a_vague_translation_does_not_erase_a_readable_original() {
        let a = merged(FREE, READABLE, VAGUE);
        assert_eq!(a.status, GlutenStatus::GlutenFree);
        assert!(codes(&a).contains(&"TRANSLATION_NOT_DECISIVE"));
        // ...but it decides when the original cannot be read.
        let a = merged(FREE, UNREADABLE, VAGUE);
        assert_ne!(a.status, GlutenStatus::GlutenFree);
    }

    #[test]
    fn a_wrong_translation_is_not_proof_of_gluten() {
        let a = merged(NONE, READABLE, WHEAT);
        assert_eq!(a.status, GlutenStatus::Unknown);
        assert_eq!(a.finding, Some(GlutenFinding::Contradictory));
        assert!(codes(&a).contains(&"TRANSLATION_DISAGREES"));
        let a = merged(FREE, READABLE, WHEAT);
        assert_eq!(a.status, GlutenStatus::Unknown);
        // Unreadable original: the translation is the only evidence and is adopted.
        let a = merged(NONE, UNREADABLE, WHEAT);
        assert_eq!(a.status, GlutenStatus::ContainsGluten);
        assert!(codes(&a).contains(&"TRANSLATION_EVIDENCE"));
    }

    #[test]
    fn traces_are_kept() {
        let a = merged(TRACES, READABLE, WHEAT);
        assert_eq!(a.status, GlutenStatus::MayContainTraces);
        assert!(codes(&a).contains(&"TRANSLATION_DISAGREES"));
        let a = merged(TRACES, LIST, NONE);
        assert_eq!(a.status, GlutenStatus::MayContainTraces);
        let a = merged(FREE, LIST, TRACES);
        assert_eq!(a.status, GlutenStatus::MayContainTraces);
        assert_eq!(a.finding, Some(GlutenFinding::Contradictory));
    }

    #[test]
    fn truncated_lists_never_get_the_reassuring_text() {
        let mut text = String::from("Patatas, aceite, sal");
        for _ in 0..250 {
            text.push_str(", sal");
        }
        let a = run(&text);
        assert_eq!(a.status, GlutenStatus::Unknown);
        assert_eq!(a.finding, Some(GlutenFinding::InsufficientInformation));
        assert!(codes(&a).contains(&"INCOMPLETE_LIST"), "{:?}", codes(&a));
        assert!(!codes(&a).contains(&"NO_GLUTEN_INGREDIENTS_IDENTIFIED"));
        // Gluten past the item limit is still found by the core engine.
        let a = run(&format!("{text}, harina de trigo"));
        assert_eq!(a.status, GlutenStatus::ContainsGluten);
        // Oversized input too.
        let huge = format!("{}, harina de trigo", "patata, ".repeat(4000));
        let a = run(&huge);
        assert_eq!(a.status, GlutenStatus::ContainsGluten);
    }

    #[test]
    fn unbalanced_brackets_are_reported() {
        let a = run("Patatas, aceite (girasol, sal");
        assert!(codes(&a).contains(&"UNBALANCED_LIST"), "{:?}", codes(&a));
        assert_ne!(a.status, GlutenStatus::GlutenFree);
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
