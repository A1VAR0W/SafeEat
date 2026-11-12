//! Orientative AI analysis (wire status `VERIFIED`, shown as "Análisis orientativo por
//! IA", never as a green safety badge): a second opinion from a small model running on the server.
//!
//! Measured on the home server (AMD E2-6110, no AVX2): Qwen2.5-1.5B Q4_K_M reads
//! ~6 tokens/s, so a whole list takes about a minute and, judged as a list, it called
//! "harina de trigo" gluten-free. Asked one ingredient at a time with examples it is
//! usable but still wrong now and then (1-2 of 30 gluten products passed in our
//! test set). So the model never decides alone.
//! The model is asked only when ALL of these hold:
//!   1. the deterministic engine found no evidence at all (status UNKNOWN, no clauses);
//!   2. Open Food Facts recognised every ingredient (`unknown_ingredients_n == 0`);
//!   3. no word of a multilingual blocklist of gluten sources and vague ingredients
//!      ("harina", "pan", "cerveza", "blé", "frumento", "Weizen"...) appears;
//!   4. the model answers "no" for every single ingredient.
//!
//! Any "sí" becomes a warning listing those ingredients. Checks run in a background
//! queue (one at a time, capped container) and are cached per ingredient list.

use std::collections::HashSet;
use std::time::Duration;

use once_cell::sync::Lazy;
use regex::Regex;
use safeeat_contracts::{AiCheck, AiCheckStatus, GlutenStatus, Product};
use safeeat_gluten_engine::normalize;
use serde_json::{json, Value};
use tokio::sync::mpsc;

use crate::products::ingredients_hash;
use crate::providers::IngredientLanguage;
use crate::state::AppState;

const MAX_ITEMS: usize = 40;
const QUEUE: usize = 64;
/// Room for checks nobody is waiting for (search results enriched in the background):
/// they only run when no product she opened is queued.
const BACKGROUND_QUEUE: usize = 24;

/// Phrases that are safe on their own and would otherwise trip the blocklist
/// ("harina de arroz", "pasta de cacao", "almidón de maíz"...). Matched on normalized text.
static SAFE_PHRASES: Lazy<Regex> = Lazy::new(|| {
    Regex::new(concat!(
        r"\b(?:harina|almidon|fecula|farine|amidon|farina|farinha|flour|starch)\s+(?:de\s+|di\s+)?",
        r"(?:arroz|riz|riso|rice|maiz|mais|corn|maize|milho|patata|patatas|pomme de terre|potato|batata|",
        r"tapioca|mandioca|manioc|yuca|garbanzos?|chickpea|almendras?|almond|coco|coconut|castanas?|",
        r"guisantes?|pea|lentejas?|soja|soy|quinoa|quinua|teff|sorgo|mijo|millet|amaranto|lino|chia|",
        r"altramuz|avellanas?|trigo sarraceno|alforfon|sarrasin|buckwheat)\b",
        r"|\b(?:rice|corn|maize|potato|tapioca|chickpea|almond|coconut)\s+(?:flour|starch)\b",
        r"|\bpasta\s+de\s+(?:cacao|avellanas?|almendras?|cacahuetes?|sesamo|pistachos?|tomate|pimientos?|",
        r"aceitunas?|frutos secos|anchoas?|curry|miso|wasabi|dattiles|datiles|membrillo)\b",
        r"|\bmaltodextrina\b|\bmaltodextrin\b|\bjarabe de glucosa\b|\bglucose syrup\b",
    ))
    .expect("valid regex")
});

/// Gluten sources and vague ingredients the engine does not know (or not in every language).
static BLOCKLIST: Lazy<Regex> = Lazy::new(|| {
    Regex::new(concat!(
        r"\b(?:",
        // Spanish
        r"harinas?|semolas?|cuscus|couscous|bulgur|burgol|seitan|panko|pan|panes|rallado|rebozad[oa]s?|",
        r"empanad[oa]s?|pasta|pastas|fideos?|tallarines|macarrones|espaguetis|lasana|galletas?|bizcochos?|",
        r"obleas?|barquillos?|cereales|cerveza|escanda|farro|freekeh|frik|graham|almidon|almidones|",
        r"dextrinas?|salvado|germen|malteado|maltosa|proteina vegetal|proteina hidrolizada|",
        r"salsa de soja|soja salsa|tamari|extracto de levadura|levadura de cerveza|crutones|picatostes|",
        r"croquetas?|hojaldre|masa|masa madre|tortitas? de trigo|wrap|cebada|centeno|",
        // English
        r"flour|bread|breadcrumbs?|crumb|crumbs|noodles?|semolina|durum|bran|biscuits?|crackers?|",
        r"wafers?|beer|ale|lager|stout|starch|dextrin|hydrolysed|hydrolyzed|soy sauce|farina|einkorn|",
        r"emmer|couscous|batter|breaded|rusk|",
        // French
        r"ble|farine|orge|seigle|avoine|epeautre|semoule|chapelure|biere|amidon|pain|gaufrette|biscuit|",
        // Italian
        r"frumento|grano|orzo|segale|semola|pane|pangrattato|",
        // German / Dutch / Portuguese
        r"weizen\w*|gerste\w*|roggen\w*|hafer\w*|dinkel\w*|\w*mehl|griess|\w*bier|malz\w*|tarwe\w*|gerst|rogge|haver|",
        r"bloem|cevada|centeio|aveia|farinha|cerveja",
        r")\b",
    ))
    .expect("valid regex")
});

static SPLIT: Lazy<Regex> = Lazy::new(|| Regex::new(r"[,;:()\[\]]|\.\s|\.$").expect("valid regex"));
static PERCENT: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"\d+(?:[.,]\d+)?\s*%").expect("valid regex"));

/// Worked examples for the model (none of them is in the evaluation set).
const SHOTS: &[(&str, &str)] = &[
    ("agua mineral", "no"),
    ("harina de trigo integral", "sí"),
    ("arroz integral", "no"),
    ("cebada perlada", "sí"),
    ("leche desnatada", "no"),
    ("pan tostado", "sí"),
    ("maíz dulce", "no"),
    ("extracto de malta", "sí"),
    ("aceite de girasol", "no"),
    ("copos de avena", "sí"),
    ("azúcar moreno", "no"),
    ("galletas", "sí"),
    ("sal marina", "no"),
    ("harina", "sí"),
];
const SYSTEM: &str = "Clasificas ingredientes para celíacos. Responde solo \"sí\" si el ingrediente contiene o puede contener gluten y \"no\" si es naturalmente sin gluten.";

#[derive(Debug, Clone)]
pub struct AiJob {
    pub product_id: String,
    pub hash: String,
    pub items: Vec<String>,
    /// Failed attempts so far; the job is retried later up to `MAX_ATTEMPTS`.
    pub attempts: u8,
}

const MAX_ATTEMPTS: u8 = 3;

/// The background queue. `pending` lets responses say "Comprobando…".
pub struct AiChecker {
    http: reqwest::Client,
    url: String,
    pub model: String,
    sender: mpsc::Sender<AiJob>,
    receiver: std::sync::Mutex<Option<mpsc::Receiver<AiJob>>>,
    background: mpsc::Sender<AiJob>,
    background_receiver: std::sync::Mutex<Option<mpsc::Receiver<AiJob>>>,
    pending: std::sync::Mutex<HashSet<String>>,
    /// Pending only in the background queue: opening the product moves it ahead.
    in_background: std::sync::Mutex<HashSet<String>>,
}

impl AiChecker {
    pub fn new(http: reqwest::Client, url: String, model: String) -> Self {
        let (sender, receiver) = mpsc::channel(QUEUE);
        let (background, background_receiver) = mpsc::channel(BACKGROUND_QUEUE);
        Self {
            http,
            url: url.trim_end_matches('/').to_string(),
            model,
            sender,
            receiver: std::sync::Mutex::new(Some(receiver)),
            background,
            background_receiver: std::sync::Mutex::new(Some(background_receiver)),
            pending: std::sync::Mutex::new(HashSet::new()),
            in_background: std::sync::Mutex::new(HashSet::new()),
        }
    }

    fn is_pending(&self, product_id: &str) -> bool {
        self.pending
            .lock()
            .expect("pending lock")
            .contains(product_id)
    }
}

/// Ingredient items worth asking about, or `None` when a deterministic gate already
/// rules a favourable AI result out (see module docs).
pub fn eligible(item: &Product, language: &IngredientLanguage) -> Option<Vec<String>> {
    if item.user_created
        || item.assessment.status != GlutenStatus::Unknown
        || !item.assessment.evidence.is_empty()
        || language.unknown != Some(0)
        || language.parsed.is_empty()
    {
        return None;
    }
    let shown = item.ingredients.as_deref()?;
    for text in [Some(shown), item.ingredients_original.as_deref()]
        .into_iter()
        .flatten()
    {
        let normalized = normalize(text);
        let without_safe = SAFE_PHRASES.replace_all(&normalized, " ");
        if BLOCKLIST.is_match(&without_safe) {
            return None;
        }
    }
    let items = split(shown);
    (!items.is_empty() && items.len() <= MAX_ITEMS && items.iter().all(|i| i.chars().count() <= 80))
        .then_some(items)
}

pub fn split(text: &str) -> Vec<String> {
    let text = PERCENT.replace_all(text, " ");
    let mut seen = HashSet::new();
    SPLIT
        .split(&text)
        .map(|s| s.split_whitespace().collect::<Vec<_>>().join(" "))
        .filter(|s| s.chars().count() > 1)
        .filter(|s| seen.insert(s.to_lowercase()))
        .collect()
}

/// Queue a check unless one exists for this exact list (or is already queued).
pub async fn enqueue(state: &AppState, item: &Product, items: Vec<String>) {
    let Some(ai) = state.ai_check.as_ref() else {
        return;
    };
    let hash = ingredients_hash(item);
    let done = sqlx::query_scalar::<_, i64>(
        "SELECT count(*) FROM product_ai_checks WHERE product_id=$1 AND ingredients_hash=$2",
    )
    .bind(&item.id)
    .bind(&hash)
    .fetch_one(&state.db)
    .await
    .unwrap_or(1);
    if done > 0 {
        return;
    }
    let newly = ai
        .pending
        .lock()
        .expect("pending lock")
        .insert(item.id.clone());
    // Already queued for her: nothing to do, unless it waits in the background queue.
    let promoted = !newly
        && ai
            .in_background
            .lock()
            .expect("background lock")
            .remove(&item.id);
    if !newly && !promoted {
        return;
    }
    let job = AiJob {
        product_id: item.id.clone(),
        hash,
        items,
        attempts: 0,
    };
    let (hash, items) = (job.hash.clone(), job.items.clone());
    if ai.sender.try_send(job).is_err() {
        // Queue full: try again the next time the product is fetched.
        ai.pending.lock().expect("pending lock").remove(&item.id);
        return;
    }
    // Remembered in the database so a restart does not lose the job.
    let stored = sqlx::query(
        "INSERT INTO product_ai_queue(product_id,ingredients_hash,items) VALUES($1,$2,$3)
         ON CONFLICT (product_id) DO UPDATE SET ingredients_hash=EXCLUDED.ingredients_hash, items=EXCLUDED.items, queued_at=now()",
    )
    .bind(&item.id)
    .bind(&hash)
    .bind(&items)
    .execute(&state.db)
    .await;
    if let Err(error) = stored {
        tracing::warn!(%error, "AI job not persisted; it will be lost on restart");
    }
}

/// Queue a check for a search result. Not persisted (a restart just drops it) and
/// returns whether it was queued.
pub async fn enqueue_background(state: &AppState, item: &Product, items: Vec<String>) -> bool {
    let Some(ai) = state.ai_check.as_ref() else {
        return false;
    };
    let hash = ingredients_hash(item);
    if already_checked(state, &item.id, &hash).await {
        return false;
    }
    if !ai
        .pending
        .lock()
        .expect("pending lock")
        .insert(item.id.clone())
    {
        return false;
    }
    let job = AiJob {
        product_id: item.id.clone(),
        hash,
        items,
        attempts: 0,
    };
    if ai.background.try_send(job).is_err() {
        ai.pending.lock().expect("pending lock").remove(&item.id);
        return false;
    }
    ai.in_background
        .lock()
        .expect("background lock")
        .insert(item.id.clone());
    true
}

async fn already_checked(state: &AppState, product_id: &str, hash: &str) -> bool {
    sqlx::query_scalar::<_, i64>(
        "SELECT count(*) FROM product_ai_checks WHERE product_id=$1 AND ingredients_hash=$2",
    )
    .bind(product_id)
    .bind(hash)
    .fetch_one(&state.db)
    .await
    .map_or(true, |n| n > 0)
}

/// Jobs left over from before a restart go back into the queue.
async fn restore_queue(state: &AppState, ai: &AiChecker) {
    let rows = sqlx::query_as::<_, (String, String, Vec<String>)>(
        "SELECT product_id,ingredients_hash,items FROM product_ai_queue ORDER BY queued_at LIMIT $1",
    )
    .bind(QUEUE as i64)
    .fetch_all(&state.db)
    .await
    .unwrap_or_default();
    let mut restored = 0;
    for (product_id, hash, items) in rows {
        if !ai
            .pending
            .lock()
            .expect("pending lock")
            .insert(product_id.clone())
        {
            continue;
        }
        let job = AiJob {
            product_id,
            hash,
            items,
            attempts: 0,
        };
        if let Err(error) = ai.sender.try_send(job) {
            let id = &error.into_inner().product_id;
            ai.pending.lock().expect("pending lock").remove(id);
            break;
        }
        restored += 1;
    }
    if restored > 0 {
        tracing::info!(restored, "AI checks restored after restart");
    }
}

/// The job is finished (stored, or given up): forget it.
async fn forget(state: &AppState, job: &AiJob) {
    let _ = sqlx::query("DELETE FROM product_ai_queue WHERE product_id=$1 AND ingredients_hash=$2")
        .bind(&job.product_id)
        .bind(&job.hash)
        .execute(&state.db)
        .await;
}

async fn ask(ai: &AiChecker, item: &str) -> anyhow::Result<bool> {
    let mut messages = vec![json!({"role": "system", "content": SYSTEM})];
    for (ingredient, answer) in SHOTS {
        messages.push(json!({"role": "user", "content": format!("Ingrediente: {ingredient}")}));
        messages.push(json!({"role": "assistant", "content": answer}));
    }
    messages.push(json!({"role": "user", "content": format!("Ingrediente: {item}")}));
    let response: Value = ai
        .http
        .post(format!("{}/v1/chat/completions", ai.url))
        // The first request reads the whole prompt (~45 s on the home server); later
        // ones reuse llama.cpp's prompt cache and take a few seconds.
        .timeout(Duration::from_secs(150))
        .json(
            &json!({"messages": messages, "temperature": 0, "max_tokens": 3, "cache_prompt": true}),
        )
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    let answer = response["choices"][0]["message"]["content"]
        .as_str()
        .unwrap_or("")
        .trim()
        .to_lowercase();
    // Only an explicit "no" counts as safe; anything else is a "sí".
    Ok(answer.starts_with("no"))
}

async fn run(state: &AppState, ai: &AiChecker, job: &AiJob) -> anyhow::Result<()> {
    let mut flagged = Vec::new();
    for item in &job.items {
        let safe = match ask(ai, item).await {
            Ok(safe) => safe,
            Err(_) => ask(ai, item).await?, // one retry (model restarting, cold cache)
        };
        if !safe {
            flagged.push(item.clone());
        }
    }
    let status = if flagged.is_empty() {
        "VERIFIED"
    } else {
        "SUSPECTED"
    };
    sqlx::query(
        "INSERT INTO product_ai_checks(product_id,ingredients_hash,status,flagged,model)
         VALUES($1,$2,$3,$4,$5) ON CONFLICT (product_id,ingredients_hash)
         DO UPDATE SET status=EXCLUDED.status, flagged=EXCLUDED.flagged, model=EXCLUDED.model, checked_at=now()",
    )
    .bind(&job.product_id)
    .bind(&job.hash)
    .bind(status)
    .bind(&flagged)
    .bind(&ai.model)
    .execute(&state.db)
    .await?;
    tracing::info!(product = %job.product_id, status, items = job.items.len(), "AI check stored");
    Ok(())
}

/// Background worker: one job at a time so the shared server stays responsive.
pub fn spawn_worker(state: AppState) {
    let Some(ai) = state.ai_check.as_ref() else {
        return;
    };
    let Some(mut receiver) = ai.receiver.lock().expect("receiver lock").take() else {
        return;
    };
    let Some(mut background) = ai.background_receiver.lock().expect("receiver lock").take() else {
        return;
    };
    tokio::spawn(async move {
        let ai = state.ai_check.as_ref().expect("checked above");
        // Warm the prompt cache so the first real product does not wait for it.
        if let Err(error) = ask(ai, "agua").await {
            tracing::warn!(%error, "AI model not reachable yet");
        }
        restore_queue(&state, ai).await;
        loop {
            // Products she opened always go first; search results fill the gaps.
            let mut job = tokio::select! {
                biased;
                Some(job) = receiver.recv() => job,
                Some(job) = background.recv() => job,
                else => break,
            };
            ai.in_background
                .lock()
                .expect("background lock")
                .remove(&job.product_id);
            // A promoted job runs twice in the queues: the second copy has nothing to do.
            if already_checked(&state, &job.product_id, &job.hash).await {
                ai.pending
                    .lock()
                    .expect("pending lock")
                    .remove(&job.product_id);
                continue;
            }
            if let Err(error) = run(&state, ai, &job).await {
                job.attempts += 1;
                tracing::warn!(product = %job.product_id, attempts = job.attempts, %error, "AI check failed");
                if job.attempts < MAX_ATTEMPTS {
                    // Keep it marked as pending and try again in a few minutes.
                    let state = state.clone();
                    let delay = Duration::from_secs(120 * u64::from(job.attempts));
                    tokio::spawn(async move {
                        tokio::time::sleep(delay).await;
                        let ai = state.ai_check.as_ref().expect("checked above");
                        if let Err(error) = ai.sender.try_send(job) {
                            let id = &error.into_inner().product_id;
                            ai.pending.lock().expect("pending lock").remove(id);
                        }
                    });
                    continue;
                }
            }
            forget(&state, &job).await;
            ai.pending
                .lock()
                .expect("pending lock")
                .remove(&job.product_id);
        }
    });
}

fn view(
    status: &str,
    flagged: Vec<String>,
    model: String,
    checked_at: Option<String>,
) -> Option<AiCheck> {
    let (status, message) = match status {
        "VERIFIED" => (
            AiCheckStatus::Verified,
            "Análisis orientativo: la IA no ha visto ingredientes con gluten en esta lista. No es una certificación, puede equivocarse y no detecta trazas ni contaminación cruzada. Revisa siempre la etiqueta.".to_string(),
        ),
        "SUSPECTED" => (
            AiCheckStatus::Suspected,
            format!(
                "La IA cree que estos ingredientes podrían contener gluten: {}. Revisa la etiqueta.",
                flagged.join(", ")
            ),
        ),
        _ => return None,
    };
    Some(AiCheck {
        status,
        message,
        flagged,
        model,
        checked_at,
    })
}

/// Attach stored checks (one query). Only products the engine left UNKNOWN show one.
pub async fn attach(state: &AppState, items: Vec<Product>) -> Vec<Product> {
    let candidates: Vec<String> = items
        .iter()
        .filter(|p| p.assessment.status == GlutenStatus::Unknown && !p.user_created)
        .map(|p| p.id.clone())
        .collect();
    if candidates.is_empty() {
        return items;
    }
    let rows = sqlx::query_as::<_, (String, String, String, Vec<String>, String, String)>(
        "SELECT product_id,ingredients_hash,status,flagged,model,to_char(checked_at AT TIME ZONE 'UTC','YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"')
         FROM product_ai_checks WHERE product_id=ANY($1)",
    )
    .bind(&candidates)
    .fetch_all(&state.db)
    .await
    .unwrap_or_default();
    items
        .into_iter()
        .map(|mut item| {
            if !candidates.contains(&item.id) {
                return item;
            }
            let hash = ingredients_hash(&item);
            let stored = rows.iter().find(|r| r.0 == item.id && r.1 == hash);
            item.ai_check = match stored {
                Some((_, _, status, flagged, model, at)) => {
                    view(status, flagged.clone(), model.clone(), Some(at.clone()))
                }
                None => state
                    .ai_check
                    .as_ref()
                    .filter(|ai| ai.is_pending(&item.id))
                    .map(|ai| AiCheck {
                        status: AiCheckStatus::Pending,
                        message: "Comprobando los ingredientes con IA…".into(),
                        flagged: Vec::new(),
                        model: ai.model.clone(),
                        checked_at: None,
                    }),
            };
            item
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::ParsedIngredient;
    use safeeat_contracts::{Assessment, NutritionInput, NutritionScore};

    pub fn product(ingredients: &str) -> Product {
        Product {
            id: "off-1".into(),
            barcode: Some("8410000000001".into()),
            name: "x".into(),
            brand: None,
            image_url: None,
            ingredients_image_url: None,
            information_source: Default::default(),
            community_contribution: false,
            gluten_reports: None,
            verified_gluten_reports: None,
            ingredients: Some(ingredients.into()),
            assessment: Assessment {
                status: GlutenStatus::Unknown,
                confidence: 0.0,
                reasons: vec![],
                evidence: vec![],
                engine_version: "1".into(),
                certification: None,
            },
            nutrition: NutritionInput::default(),
            quality: NutritionScore {
                score: None,
                breakdown: vec![],
                explanation: String::new(),
                version: "1".into(),
            },
            nutri_score: None,
            source: "OFF".into(),
            source_url: None,
            fetched_at: String::new(),
            stale: false,
            user_created: false,
            my_verdict: None,
            ingredients_original: None,
            ingredients_language: None,
            ai_check: None,
            community_ingredients: None,
            provider_assessment: None,
        }
    }

    pub fn recognised() -> IngredientLanguage {
        IngredientLanguage {
            code: Some("es".into()),
            parsed: vec![ParsedIngredient::default()],
            unknown: Some(0),
            trace_tags: vec![],
        }
    }

    #[test]
    fn blocklist_keeps_vague_and_foreign_gluten_sources_out() {
        for text in [
            "Bulgur, cebolla, perejil.",
            "Seitán, salsa de soja, ajo.",
            "Escanda, agua, sal.",
            "Farro perlado, tomate.",
            "Freekeh, garbanzos.",
            "Pollo, pan rallado, huevo.",
            "Harina, agua, sal.",
            "Farine de blé, sucre.",
            "Farina di frumento, acqua.",
            "Weizenmehl, Zucker.",
            "Proteína vegetal hidrolizada, sal.",
            "Cerveza, cebolla.",
            "Almidón, azúcar.",
            "Obleas, chocolate.",
            "Graham crackers, mantequilla.",
        ] {
            assert!(eligible(&product(text), &recognised()).is_none(), "{text}");
        }
    }

    #[test]
    fn plain_safe_lists_can_be_asked() {
        for text in [
            "Harina de maíz, harina de arroz, fécula de patata, azúcar, huevo.",
            "Leche, azúcar, cacao, almidón de maíz, sal.",
            "Chocolate negro 70 % (pasta de cacao, azúcar, manteca de cacao).",
            "Arroz, agua, sal.",
        ] {
            assert!(eligible(&product(text), &recognised()).is_some(), "{text}");
        }
    }

    #[test]
    fn other_gates_rule_it_out() {
        let text = "Arroz, agua, sal.";
        let mut unknown = recognised();
        unknown.unknown = Some(1);
        assert!(eligible(&product(text), &unknown).is_none());
        assert!(eligible(&product(text), &IngredientLanguage::default()).is_none());
        let mut claimed = product(text);
        claimed.assessment.status = GlutenStatus::GlutenFree;
        assert!(eligible(&claimed, &recognised()).is_none());
    }

    #[test]
    fn splits_items() {
        assert_eq!(
            split("Chocolate negro 70% (pasta de cacao, azúcar), sal."),
            vec!["Chocolate negro", "pasta de cacao", "azúcar", "sal"]
        );
    }
}

#[cfg(test)]
mod gate_regression {
    use super::*;
    use safeeat_gluten_engine::{assess_gluten, GlutenInput};

    /// Every gluten product of the evaluation set must be stopped before the model:
    /// either the engine finds it or the gates rule it out. (The 1.5B model alone let
    /// 1-2 of these through as "safe".)
    #[test]
    fn no_gluten_case_reaches_the_model() {
        let cases: Vec<Value> =
            serde_json::from_str(include_str!("../tests/fixtures/ai-gluten-cases.json")).unwrap();
        let mut asked_safe = 0;
        for case in &cases {
            let text = case["t"].as_str().unwrap();
            let mut item = tests_support::product(text);
            item.assessment = assess_gluten(&GlutenInput {
                ingredients: Some(text.into()),
                source: "test".into(),
                ..Default::default()
            });
            let reaches_model = eligible(&item, &tests_support::recognised()).is_some();
            if case["g"] == 0 {
                // The wider vocabulary of engine 1.1.0 must not flag real gluten-free lists.
                assert!(
                    !matches!(
                        item.assessment.status,
                        GlutenStatus::ContainsGluten | GlutenStatus::MayContainTraces
                    ),
                    "false positive on a gluten-free list: {text}"
                );
            }
            if case["g"] == 1 {
                assert!(!reaches_model, "gluten case reached the model: {text}");
            } else if reaches_model {
                asked_safe += 1;
            }
        }
        // Most plain gluten-free lists still reach the model.
        assert!(
            asked_safe >= 14,
            "only {asked_safe} safe cases reach the model"
        );
    }
}

#[cfg(test)]
mod tests_support {
    pub use super::tests::{product, recognised};
}
