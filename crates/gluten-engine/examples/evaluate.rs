//! Reproducible evaluation of the gluten analysis on hand-labelled cases.
//!
//! `cargo run --release -p safeeat-gluten-engine --example evaluate`
//!
//! Ground truth is the hand label of each case (tests/fixtures/evaluation.json), never
//! a model. Prints dangerous false negatives, false positives, abstention, coverage,
//! precision of favourable answers, sensitivity, latency and the share of products
//! that would need the AI, with exact (Clopper-Pearson) 95 % intervals.

use std::time::Instant;

use safeeat_contracts::{GlutenFinding, GlutenStatus, IngredientClass};
use safeeat_gluten_engine::{
    analyze, Alternative, AlternativeKind, Analysis, Context, GlutenInput, Origin, ENGINE_VERSION,
};
use serde_json::Value;

fn load() -> Vec<(String, String, GlutenInput, Context, Option<String>)> {
    let mut cases = Vec::new();
    let manual: Value =
        serde_json::from_str(include_str!("../tests/fixtures/evaluation.json")).unwrap();
    for c in manual["cases"].as_array().unwrap() {
        let text = |k: &str| c[k].as_str().map(str::to_string);
        cases.push((
            c["id"].as_str().unwrap().to_string(),
            c["truth"].as_str().unwrap().to_string(),
            GlutenInput {
                ingredients: text("ingredients"),
                allergens: text("allergens"),
                traces: text("traces"),
                declaration: text("declaration"),
                source: "Evaluación".into(),
                origin: Some(Origin::Provider),
                ..Default::default()
            },
            Context {
                eu_labelling: c["eu"].as_bool().unwrap_or(true),
            },
            text("alternate"),
        ));
    }
    let legacy: Value = serde_json::from_str(include_str!(
        "../../../apps/server/tests/fixtures/ai-gluten-cases.json"
    ))
    .unwrap();
    for (i, c) in legacy.as_array().unwrap().iter().enumerate() {
        cases.push((
            format!("ai-gluten-cases-{i}"),
            if c["g"] == 1 { "GLUTEN" } else { "SAFE" }.into(),
            GlutenInput {
                ingredients: c["t"].as_str().map(str::to_string),
                source: "Evaluación".into(),
                origin: Some(Origin::Provider),
                ..Default::default()
            },
            Context::default(),
            None,
        ));
    }
    cases
}

fn run(input: &GlutenInput, context: Context, alternate: &Option<String>) -> Analysis {
    let analysis = analyze(input, context);
    match alternate {
        Some(text) => {
            let other = analyze(
                &GlutenInput {
                    ingredients: Some(text.clone()),
                    ..input.clone()
                },
                context,
            );
            analysis.with_alternatives(vec![Alternative {
                label: "original".into(),
                kind: AlternativeKind::SourceList,
                analysis: other,
            }])
        }
        None => analysis,
    }
}

/// log of the binomial coefficient.
fn ln_choose(n: u64, k: u64) -> f64 {
    let lg = |x: u64| (1..=x).map(|v| (v as f64).ln()).sum::<f64>();
    lg(n) - lg(k) - lg(n - k)
}

fn binom_cdf(k: u64, n: u64, p: f64) -> f64 {
    if p <= 0.0 {
        return 1.0;
    }
    if p >= 1.0 {
        return if k >= n { 1.0 } else { 0.0 };
    }
    (0..=k)
        .map(|i| (ln_choose(n, i) + i as f64 * p.ln() + (n - i) as f64 * (1.0 - p).ln()).exp())
        .sum::<f64>()
        .min(1.0)
}

/// Exact two-sided 95 % Clopper-Pearson interval for k events in n trials.
fn clopper_pearson(k: u64, n: u64) -> (f64, f64) {
    if n == 0 {
        return (0.0, 1.0);
    }
    let solve = |f: &dyn Fn(f64) -> f64| {
        let (mut lo, mut hi) = (0.0f64, 1.0f64);
        for _ in 0..100 {
            let mid = (lo + hi) / 2.0;
            if f(mid) > 0.0 {
                lo = mid;
            } else {
                hi = mid;
            }
        }
        (lo + hi) / 2.0
    };
    let lower = if k == 0 {
        0.0
    } else {
        // P(X >= k) = 0.025
        solve(&|p| 0.025 - (1.0 - binom_cdf(k - 1, n, p)))
    };
    let upper = if k == n {
        1.0
    } else {
        // P(X <= k) = 0.025
        solve(&|p| binom_cdf(k, n, p) - 0.025)
    };
    (lower, upper)
}

fn pct(x: f64) -> String {
    format!("{:.2} %", x * 100.0)
}

fn ratio(k: usize, n: usize) -> String {
    let (lo, hi) = clopper_pearson(k as u64, n as u64);
    let point = if n == 0 { 0.0 } else { k as f64 / n as f64 };
    format!(
        "{k}/{n} = {} (IC 95 %: {} – {})",
        pct(point),
        pct(lo),
        pct(hi)
    )
}

fn percentile(sorted: &[f64], q: f64) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    let idx = ((sorted.len() as f64 - 1.0) * q).round() as usize;
    sorted[idx]
}

fn main() {
    let cases = load();
    // Warm up the lazily compiled rules so latency measures the analysis itself.
    let _ = analyze(&cases[0].2, cases[0].3);
    let mut latencies = Vec::new();
    let (mut positives, mut dangerous, mut warned, mut red) = (0, 0, 0, 0);
    let (mut traces_truth, mut amber) = (0, 0);
    let (mut negatives, mut false_positives) = (0, 0);
    let (mut favourable, mut favourable_right) = (0, 0);
    let (mut unknown, mut ai_products) = (0, 0);
    let (mut items, mut recognised, mut ai_items) = (0, 0, 0);
    let mut failures = Vec::new();
    for (id, truth, input, context, alternate) in &cases {
        let mut analysis = None;
        // 50 runs per case for a stable latency figure.
        for _ in 0..50 {
            let started = Instant::now();
            let result = run(input, *context, alternate);
            latencies.push(started.elapsed().as_secs_f64() * 1e6);
            analysis = Some(result);
        }
        let analysis = analysis.unwrap();
        let a = &analysis.assessment;
        if std::env::var_os("VERBOSE").is_some() {
            println!(
                "{id:32} {truth:12} {:?}/{:?} ai={:?}",
                a.status, a.finding, analysis.ai_candidates
            );
        }
        let reassuring = a.status == GlutenStatus::GlutenFree
            || a.finding == Some(GlutenFinding::NoGlutenIdentified);
        let alarm = matches!(
            a.status,
            GlutenStatus::ContainsGluten | GlutenStatus::MayContainTraces
        );
        items += a.items.len();
        recognised += a
            .items
            .iter()
            .filter(|i| i.class != IngredientClass::Unrecognized)
            .count();
        ai_items += analysis.ai_candidates.len();
        if !analysis.ai_candidates.is_empty() {
            ai_products += 1;
        }
        if a.status == GlutenStatus::Unknown {
            unknown += 1;
        }
        if reassuring {
            favourable += 1;
            if matches!(truth.as_str(), "SAFE" | "GF_DECLARED") {
                favourable_right += 1;
            }
        }
        match truth.as_str() {
            "GLUTEN" | "TRACES" => {
                positives += 1;
                if reassuring {
                    dangerous += 1;
                    failures.push(format!("PELIGROSO {id}: {:?} {:?}", a.status, a.finding));
                }
                if !reassuring {
                    warned += 1;
                }
                if truth == "GLUTEN" && a.status == GlutenStatus::ContainsGluten {
                    red += 1;
                }
                if truth == "TRACES" {
                    traces_truth += 1;
                    if a.status == GlutenStatus::MayContainTraces {
                        amber += 1;
                    }
                }
            }
            _ => {
                negatives += 1;
                if alarm {
                    false_positives += 1;
                    failures.push(format!(
                        "FALSO POSITIVO {id}: {:?} {:?}",
                        a.status, a.finding
                    ));
                }
            }
        }
    }
    latencies.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let gluten_truth = cases.iter().filter(|c| c.1 == "GLUTEN").count();
    println!(
        "Motor {ENGINE_VERSION} · {} casos ({positives} con riesgo, {negatives} sin riesgo)",
        cases.len()
    );
    println!(
        "Falsos negativos peligrosos (verde o «no se identifican» con riesgo real): {}",
        ratio(dangerous, positives)
    );
    println!(
        "Sensibilidad (con riesgo y sin mensaje tranquilizador):  {}",
        ratio(warned, positives)
    );
    println!(
        "Sensibilidad estricta (gluten → CONTIENE GLUTEN):       {}",
        ratio(red, gluten_truth)
    );
    println!(
        "Trazas declaradas → PUEDE CONTENER TRAZAS:              {}",
        ratio(amber, traces_truth)
    );
    println!(
        "Falsos positivos (rojo o ámbar sin riesgo):             {}",
        ratio(false_positives, negatives)
    );
    println!(
        "UNKNOWN / abstención:                                   {}",
        ratio(unknown, cases.len())
    );
    println!(
        "Precisión de las respuestas favorables:                 {}",
        ratio(favourable_right, favourable)
    );
    println!(
        "Cobertura del catálogo (ingredientes reconocidos):      {}",
        ratio(recognised, items)
    );
    println!(
        "Productos que pedirían IA:                              {}",
        ratio(ai_products, cases.len())
    );
    println!(
        "Ingredientes que pedirían IA:                           {}",
        ratio(ai_items, items)
    );
    println!(
        "Latencia del análisis determinista: p50 {:.0} µs · p95 {:.0} µs · máx {:.0} µs ({} ejecuciones)",
        percentile(&latencies, 0.5),
        percentile(&latencies, 0.95),
        latencies.last().copied().unwrap_or(0.0),
        latencies.len()
    );
    let target: f64 = 0.0001;
    let needed = ((0.05f64).ln() / (1.0 - target).ln()).ceil();
    println!(
        "Objetivo < 0,01 % de falsos negativos: NO DEMOSTRADO con {positives} positivos. Con 0 fallos harían falta ≥ {needed:.0} casos positivos etiquetados para que la cota superior unilateral al 95 % baje de 0,01 %."
    );
    for f in &failures {
        println!("  {f}");
    }
    if dangerous > 0 {
        std::process::exit(1);
    }
}
