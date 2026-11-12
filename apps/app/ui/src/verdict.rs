//! What the product card says, in the order of docs/SAFETY.md:
//! personal verdict → certified seal → automatic analysis → community.
//! Pure functions, unit-tested on the host (`cargo test -p safeeat-ui`).
//!
//! Green (`Tone::Safe`) is reserved for a declaration by the manufacturer or the
//! user's own verdict. An AI result or a community review that found nothing is
//! `Tone::Info`: useful, clearly labelled, never a food-safety badge.

use safeeat_contracts::{AiCheckStatus, Certification, GlutenStatus, Product, VerdictValue};

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
        if !v.ingredients_changed {
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
                text: format!("{} gluten", plural(contains, "usuario indica", "usuarios indican")),
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
        if let Some(ai) = &p.ai_check {
            return match ai.status {
                // Orientative only: the AI never certifies and cannot see traces.
                AiCheckStatus::Verified => Summary {
                    tone: Tone::Info,
                    text: "No verificado · la IA no ve gluten".into(),
                    warning: Some("Análisis orientativo: no es una certificación"),
                },
                AiCheckStatus::Suspected => Summary {
                    tone: Tone::Caution,
                    text: "La IA ve posible gluten".into(),
                    warning: None,
                },
                AiCheckStatus::Pending => Summary {
                    tone: Tone::Unknown,
                    text: "No verificado · IA comprobando".into(),
                    warning: None,
                },
            };
        }
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
        text: status_title(status).into(),
        warning: None,
    }
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
    use safeeat_contracts::{AiCheck, PersonalVerdict, ProductReportCounts};

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

    #[test]
    fn ai_is_never_green_and_never_called_verified() {
        let mut p = product("UNKNOWN");
        p.ai_check = ai(AiCheckStatus::Verified);
        let s = summary(&p);
        assert_eq!(s.tone, Tone::Info);
        assert!(s.text.starts_with("No verificado"));
        assert!(s.warning.is_some_and(|w| w.contains("no es una certificación")));
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
                    p.ai_check = status.and_then(|s| ai(s));
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
        });
        assert_eq!(summary(&p).tone, Tone::Caution);
        p.assessment.status = GlutenStatus::Unknown;
        assert_eq!(summary(&p).tone, Tone::Safe);
        p.assessment.status = GlutenStatus::ContainsGluten;
        p.my_verdict.as_mut().unwrap().ingredients_changed = true;
        assert_eq!(summary(&p).tone, Tone::Danger);
    }
}
