//! Port of `packages/nutrition-engine`: an orientative SafeEat score, independent of gluten.

use safeeat_contracts::{BreakdownItem, NutritionInput, NutritionScore};

const VERSION: &str = "1.0.0";

fn valid(value: Option<f64>, max: f64) -> Result<Option<f64>, ()> {
    match value {
        None => Ok(None),
        Some(v) if v.is_finite() && (0.0..=max).contains(&v) => Ok(Some(v)),
        Some(_) => Err(()),
    }
}

pub fn score_nutrition(value: &NutritionInput) -> NutritionScore {
    let empty = || NutritionScore {
        score: None,
        breakdown: vec![],
        explanation:
            "Datos insuficientes o inválidos por 100 g/ml para calcular la calidad nutricional."
                .into(),
        version: VERSION.into(),
    };
    let parsed = (|| {
        Ok::<_, ()>((
            valid(value.energy_kcal, 900.0)?,
            valid(value.saturated_fat, 100.0)?,
            valid(value.sugars, 100.0)?,
            valid(value.salt, 100.0)?,
            valid(value.fibre, 100.0)?,
            valid(value.protein, 100.0)?,
        ))
    })();
    let Ok((Some(energy), Some(saturated), Some(sugars), Some(salt), Some(fibre), Some(protein))) =
        parsed
    else {
        return empty();
    };
    if sugars + saturated + fibre + protein + salt > 105.0 {
        return empty();
    }
    let floor = |v: f64| v.floor() as i64;
    let breakdown = vec![
        ("Base", 70),
        ("Energía", -(15.min(floor(energy / 100.0) * 3))),
        ("Azúcar", -(25.min(floor(sugars / 5.0) * 5))),
        ("Grasas saturadas", -(20.min(floor(saturated / 2.0) * 4))),
        ("Sal", -(20.min(floor(salt / 0.3) * 4))),
        ("Fibra", 15.min(floor(fibre / 2.0) * 5)),
        ("Proteína", 10.min(floor(protein / 5.0) * 5)),
    ];
    let raw: i64 = breakdown.iter().map(|(_, p)| p).sum();
    let clamped = raw.clamp(0, 100) as f64;
    // JavaScript Math.round rounds .5 up.
    let score = ((clamped / 5.0) + 0.5).floor() as i64 * 5;
    NutritionScore {
        score: Some(score),
        breakdown: breakdown
            .into_iter()
            .map(|(label, points)| BreakdownItem { label: label.into(), points })
            .collect(),
        explanation: "Indicador orientativo SafeEat, redondeado a 5 puntos. No es Nutri-Score ni una evaluación médica. Es independiente del gluten.".into(),
        version: VERSION.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn incomplete_is_empty() {
        assert_eq!(score_nutrition(&NutritionInput::default()).score, None);
    }

    #[test]
    fn computes_score() {
        let s = score_nutrition(&NutritionInput {
            energy_kcal: Some(250.0),
            saturated_fat: Some(1.0),
            sugars: Some(3.0),
            salt: Some(0.2),
            fibre: Some(6.0),
            protein: Some(12.0),
        });
        // 70 - 6 - 0 - 0 - 0 + 15 + 10 = 89 → 90
        assert_eq!(s.score, Some(90));
    }
}
