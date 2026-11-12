//! What the product card says, in the order of docs/SAFETY.md:
//! personal verdict → certified seal → automatic analysis → community.
//! Pure functions, unit-tested on the host (`cargo test -p safeeat-ui`).

use safeeat_contracts::{AiCheckStatus, GlutenStatus, Product, VerdictValue};

/// Visual tone of a badge. Every tone also carries an icon and words, never colour alone.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tone {
    Safe,
    Danger,
    Caution,
    Unknown,
}

impl Tone {
    pub fn class(self) -> &'static str {
        match self {
            Tone::Safe => "tone-safe",
            Tone::Danger => "tone-danger",
            Tone::Caution => "tone-caution",
            Tone::Unknown => "tone-unknown",
        }
    }
    pub fn icon(self) -> &'static str {
        match self {
            Tone::Safe => crate::icons::CHECK,
            Tone::Danger => crate::icons::X_CIRCLE,
            Tone::Caution => crate::icons::ALERT,
            Tone::Unknown => crate::icons::HELP,
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
                },
                VerdictValue::Safe => Summary {
                    tone: Tone::Safe,
                    text: "Marcado por ti como seguro".into(),
                },
                VerdictValue::Unsafe => Summary {
                    tone: Tone::Danger,
                    text: "Marcado por ti como no seguro".into(),
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
                }
            } else {
                Summary {
                    tone: Tone::Safe,
                    text: "Revisión verificada: sin gluten".into(),
                }
            };
        }
        if contains > 0 {
            return Summary {
                tone: Tone::Danger,
                text: format!("{} gluten", plural(contains, "usuario indica", "usuarios indican")),
            };
        }
        if free > 0 {
            return Summary {
                tone: Tone::Caution,
                text: format!(
                    "{} sin gluten · sin confirmar",
                    plural(free, "usuario indica", "usuarios indican")
                ),
            };
        }
        if let Some(ai) = &p.ai_check {
            return match ai.status {
                // Green, but the words keep the warning.
                AiCheckStatus::Verified => Summary {
                    tone: Tone::Safe,
                    text: "Verificado por IA · revisa la etiqueta".into(),
                },
                AiCheckStatus::Suspected => Summary {
                    tone: Tone::Caution,
                    text: "La IA ve posible gluten".into(),
                },
                AiCheckStatus::Pending => Summary {
                    tone: Tone::Unknown,
                    text: "No verificado · IA comprobando".into(),
                },
            };
        }
    }
    Summary {
        tone: status_tone(status),
        text: status_title(status).into(),
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
    fn ai_verified_is_green_but_says_to_check_the_label() {
        let mut p = product("UNKNOWN");
        p.ai_check = ai(AiCheckStatus::Verified);
        let s = summary(&p);
        assert_eq!(s.tone, Tone::Safe);
        assert!(s.text.contains("revisa la etiqueta"));
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
