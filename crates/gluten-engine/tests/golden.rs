//! Golden cases produced by the original TypeScript engine (scripts/export-gluten-golden.mts).

use safeeat_contracts::{Assessment, NutritionInput, NutritionScore};
use safeeat_gluten_engine::{assess_gluten, score_nutrition, GlutenInput};
use serde::Deserialize;

#[derive(Deserialize)]
struct Golden {
    gluten: Vec<Case<GlutenInput, Assessment>>,
    nutrition: Vec<Case<NutritionInput, NutritionScore>>,
}

#[derive(Deserialize)]
struct Case<I, O> {
    input: I,
    output: O,
}

#[test]
fn matches_typescript_engine() {
    let golden: Golden = serde_json::from_str(include_str!("golden.json")).unwrap();
    let mut failures = Vec::new();
    for case in &golden.gluten {
        let actual = assess_gluten(&case.input);
        if actual != case.output {
            failures.push(format!(
                "input {:?}\n  expected {}\n  actual   {}",
                case.input,
                serde_json::to_string(&case.output).unwrap(),
                serde_json::to_string(&actual).unwrap()
            ));
        }
    }
    for case in &golden.nutrition {
        let actual = score_nutrition(&case.input);
        if actual != case.output {
            failures.push(format!(
                "nutrition {:?}: {:?} != {:?}",
                case.input, actual, case.output
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "{} mismatches:\n{}",
        failures.len(),
        failures.join("\n")
    );
}
