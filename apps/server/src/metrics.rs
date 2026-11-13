//! In-memory counters for the gluten analysis and the on-server AI: latency (p50/p95
//! over the last analyses), how many ingredients the rules resolved, cache hits and the
//! share of AI calls. No product or personal data; reset on restart.
//! Logged every hour and served to admins at `GET /api/v1/admin/analysis-metrics`.

use std::collections::{BTreeMap, VecDeque};
use std::sync::Mutex;
use std::time::Duration;

use once_cell::sync::Lazy;
use safeeat_contracts::{AiVerdict, IngredientClass};
use safeeat_gluten_engine::Analysis;
use serde_json::{json, Value};

/// Latencies kept for the percentiles.
const WINDOW: usize = 2048;

#[derive(Default)]
struct Counters {
    analyses: u64,
    analysis_us: VecDeque<u64>,
    items: u64,
    items_by_rules: u64,
    products_with_ai_candidates: u64,
    ai_candidate_items: u64,
    ai_calls: u64,
    ai_errors: u64,
    ai_cache_hits: u64,
    ai_ms: VecDeque<u64>,
    ai_verdicts: BTreeMap<String, u64>,
}

static METRICS: Lazy<Mutex<Counters>> = Lazy::new(|| Mutex::new(Counters::default()));

fn push(window: &mut VecDeque<u64>, value: u64) {
    if window.len() == WINDOW {
        window.pop_front();
    }
    window.push_back(value);
}

fn with<T>(f: impl FnOnce(&mut Counters) -> T) -> T {
    let mut guard = METRICS.lock().unwrap_or_else(|e| e.into_inner());
    f(&mut guard)
}

pub fn record_analysis(elapsed: Duration, analysis: &Analysis) {
    let items = analysis.assessment.items.len() as u64;
    let by_rules = analysis
        .assessment
        .items
        .iter()
        .filter(|i| i.class != IngredientClass::Unrecognized)
        .count() as u64;
    with(|m| {
        m.analyses += 1;
        push(&mut m.analysis_us, elapsed.as_micros() as u64);
        m.items += items;
        m.items_by_rules += by_rules;
        if !analysis.ai_candidates.is_empty() {
            m.products_with_ai_candidates += 1;
            m.ai_candidate_items += analysis.ai_candidates.len() as u64;
        }
    });
}

/// One request to the model (`ok` = valid answer inside the schema).
pub fn record_ai_call(elapsed: Duration, ok: bool) {
    with(|m| {
        m.ai_calls += 1;
        if !ok {
            m.ai_errors += 1;
        }
        push(&mut m.ai_ms, elapsed.as_millis() as u64);
    });
}

pub fn record_ai_cache_hit() {
    with(|m| m.ai_cache_hits += 1);
}

pub fn record_ai_verdict(verdict: AiVerdict) {
    let key = serde_json::to_value(verdict)
        .ok()
        .and_then(|v| v.as_str().map(str::to_string))
        .unwrap_or_else(|| "OTHER".into());
    with(|m| *m.ai_verdicts.entry(key).or_default() += 1);
}

fn percentile(window: &VecDeque<u64>, p: f64) -> Option<u64> {
    if window.is_empty() {
        return None;
    }
    let mut sorted: Vec<u64> = window.iter().copied().collect();
    sorted.sort_unstable();
    let rank = ((p * sorted.len() as f64).ceil() as usize).clamp(1, sorted.len());
    Some(sorted[rank - 1])
}

fn ratio(a: u64, b: u64) -> Option<f64> {
    (b > 0).then(|| (a as f64 / b as f64 * 10_000.0).round() / 10_000.0)
}

pub fn snapshot() -> Value {
    with(|m| {
        json!({
            "engineVersion": safeeat_gluten_engine::ENGINE_VERSION,
            "rulesVersion": safeeat_gluten_engine::RULES_VERSION,
            "analyses": m.analyses,
            "analysisLatencyMicros": {
                "p50": percentile(&m.analysis_us, 0.5),
                "p95": percentile(&m.analysis_us, 0.95),
                "samples": m.analysis_us.len(),
            },
            "ingredients": m.items,
            "ingredientsResolvedByRules": m.items_by_rules,
            "rulesCoverage": ratio(m.items_by_rules, m.items),
            "productsWithAiCandidates": m.products_with_ai_candidates,
            "shareOfProductsNeedingAi": ratio(m.products_with_ai_candidates, m.analyses),
            "aiCandidateIngredients": m.ai_candidate_items,
            "aiCalls": m.ai_calls,
            "aiErrors": m.ai_errors,
            "aiCacheHits": m.ai_cache_hits,
            "aiCacheHitRate": ratio(m.ai_cache_hits, m.ai_cache_hits + m.ai_calls),
            "aiLatencyMillis": {
                "p50": percentile(&m.ai_ms, 0.5),
                "p95": percentile(&m.ai_ms, 0.95),
                "samples": m.ai_ms.len(),
            },
            "aiVerdicts": m.ai_verdicts,
        })
    })
}

/// Hourly log line, so the numbers survive in the container logs.
pub fn spawn_logger() {
    tokio::spawn(async {
        let mut every = tokio::time::interval(Duration::from_secs(3600));
        every.tick().await;
        loop {
            every.tick().await;
            tracing::info!(metrics = %snapshot(), "analysis metrics");
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn percentiles_use_nearest_rank() {
        let w: VecDeque<u64> = (1..=100).collect();
        assert_eq!(percentile(&w, 0.5), Some(50));
        assert_eq!(percentile(&w, 0.95), Some(95));
        assert_eq!(percentile(&VecDeque::new(), 0.5), None);
        assert_eq!(ratio(1, 0), None);
        assert_eq!(ratio(1, 3), Some(0.3333));
    }
}
