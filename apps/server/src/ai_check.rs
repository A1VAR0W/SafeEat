//! On-server AI review: a structured classifier for the ingredients the rule catalog
//! could not classify. It never decides a product's status (docs/GLUTEN-ANALYSIS.md).
//!
//! Order of the analysis: the rules (`safeeat_gluten_engine::analyze`) classify every
//! ingredient first and answer at once; only when the single open question is a few
//! ingredients the catalog does not know (finding INSUFFICIENT_INFORMATION) is the model
//! asked, one ingredient at a time, in the background:
//!   1. the product is UNKNOWN and every other ingredient is resolved by the rules;
//!   2. Open Food Facts recognised every ingredient (`unknown_ingredients_n == 0`), so
//!      the unknown words are real ingredients and not OCR noise;
//!   3. none of them contains a word of a multilingual blocklist of gluten sources and
//!      vague ingredients (a catalog gap must not reach the model);
//!   4. at most `MAX_ITEMS` ingredients of at most `MAX_ITEM_CHARS` characters.
//!
//! The answer must be JSON in a closed schema (`verdict`, `cereal`), asked for with
//! llama.cpp's `response_format` and validated again here. An error, a timeout, an
//! empty or malformed answer, or one that contradicts itself is MODEL_ERROR, never
//! gluten and never safe. Every explanation shown is written by SafeEat from the
//! validated verdict, not taken from the model.
//!
//! What the result can do: `ai_review` (app 2.0.3+) and, for installed 2.0.x apps, the
//! legacy `ai_check` only as "orientative, no gluten seen" or "pending". A suspicion is
//! shown as "sin confirmar" in neutral colours, never as a gluten or traces warning;
//! a favourable answer never turns a product green nor hides a rule's warning.
//!
//! Measured on the home server (AMD E2-6110, no AVX2): Qwen2.5-1.5B Q4_K_M reads ~6
//! tokens/s and writes ~2.5 tokens/s, so answers are cached per ingredient and per
//! product, keyed by the rules, engine, prompt and model versions.

use std::collections::HashSet;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use once_cell::sync::Lazy;
use regex::Regex;
use safeeat_contracts::{
    AiCheck, AiCheckStatus, AiItemVerdict, AiOutcome, AiReview, AiVerdict, GlutenFinding,
    GlutenStatus, Product,
};
use safeeat_gluten_engine::{normalize, ENGINE_VERSION, RULES_VERSION};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use tokio::sync::mpsc;

use crate::metrics;
use crate::providers::IngredientLanguage;
use crate::state::AppState;

/// Changes whenever the system prompt, the examples or the schema change.
pub const PROMPT_VERSION: &str = "2026.11.13-json";
const MAX_ITEMS: usize = 12;
const MAX_ITEM_CHARS: usize = 80;
const QUEUE: usize = 64;
/// Room for checks nobody is waiting for (search results enriched in the background):
/// they only run when no product she opened is queued.
const BACKGROUND_QUEUE: usize = 24;
/// The first request reads the whole prompt: about 750 tokens (estimated from its
/// length, not measured), so about 2 minutes at ~6 tokens/s on the home server. Later
/// ones reuse llama.cpp's prompt cache and only read the new ingredient.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(300);
/// A whole product never holds the worker longer than this.
const JOB_TIMEOUT: Duration = Duration::from_secs(15 * 60);
/// `{"verdict":"INSUFFICIENT_INFORMATION","cereal":"TRITICALE"}` is ~20 tokens.
const MAX_TOKENS: u32 = 32;
/// Keys of this version start with it; older queue rows are dropped on restore.
const KEY_PREFIX: &str = "a2:";
const MAX_ATTEMPTS: u8 = 3;

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

/// Gluten sources and vague ingredients, in several languages. Second safety net: the
/// catalog classifies these, so a match here means a catalog gap and no AI.
static BLOCKLIST: Lazy<Regex> = Lazy::new(|| {
    Regex::new(concat!(
        r"\b(?:",
        // Spanish
        r"harinas?|semolas?|cuscus|couscous|bulgur|burgol|seitan|panko|pan|panes|rallado|rebozad[oa]s?|",
        r"empanad[oa]s?|pasta|pastas|fideos?|tallarines|macarrones|espaguetis|lasana|galletas?|bizcochos?|",
        r"obleas?|barquillos?|cereales|cerveza|escanda|farro|freekeh|frik|graham|almidon|almidones|",
        r"dextrinas?|salvado|germen|malteado|maltosa|proteina vegetal|proteina hidrolizada|",
        r"salsa de soja|soja salsa|tamari|extracto de levadura|levadura de cerveza|crutones|picatostes|",
        r"croquetas?|hojaldre|masa|masa madre|tortitas? de trigo|wrap|cebada|centeno|trigo|avena|espelta|",
        r"malta|kamut|triticale|",
        // English
        r"flour|bread|breadcrumbs?|crumb|crumbs|noodles?|semolina|durum|bran|biscuits?|crackers?|",
        r"wafers?|beer|ale|lager|stout|starch|dextrin|hydrolysed|hydrolyzed|soy sauce|farina|einkorn|",
        r"emmer|couscous|batter|breaded|rusk|wheat|barley|rye|oats?|spelt|malt|",
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

const SYSTEM: &str = concat!(
    "Clasificas UN ingrediente de una etiqueta de alimentos para personas celíacas. ",
    "Responde solo con JSON: {\"verdict\": ..., \"cereal\": ...}.\n",
    "verdict:\n",
    "- KNOWN_GLUTEN: el ingrediente es o procede claramente de trigo, cebada, centeno, avena, espelta, triticale o kamut. Indica ese cereal en cereal.\n",
    "- KNOWN_SAFE_INGREDIENT: es un alimento concreto que no procede de esos cereales. cereal NONE.\n",
    "- CONTEXT_DEPENDENT: puede proceder o no de un cereal con gluten según su origen o receta. cereal NONE.\n",
    "- INSUFFICIENT_INFORMATION: no reconoces el ingrediente o el texto no es un ingrediente. cereal NONE.\n",
    "No supongas gluten por una posibilidad teórica: si dudas, usa CONTEXT_DEPENDENT o INSUFFICIENT_INFORMATION. ",
    "No hables de trazas ni de contaminación cruzada."
);

/// Worked examples (none of them is in the evaluation sets).
const SHOTS: &[(&str, &str, &str)] = &[
    ("agua mineral", "KNOWN_SAFE_INGREDIENT", "NONE"),
    ("harina de trigo integral", "KNOWN_GLUTEN", "WHEAT"),
    ("garrofín", "KNOWN_SAFE_INGREDIENT", "NONE"),
    ("cebada perlada", "KNOWN_GLUTEN", "BARLEY"),
    ("proteína vegetal", "CONTEXT_DEPENDENT", "NONE"),
    ("leche desnatada", "KNOWN_SAFE_INGREDIENT", "NONE"),
    ("seitán", "KNOWN_GLUTEN", "WHEAT"),
    ("xq7 lote", "INSUFFICIENT_INFORMATION", "NONE"),
    ("pimentón de la Vera", "KNOWN_SAFE_INGREDIENT", "NONE"),
    ("extracto de malta", "KNOWN_GLUTEN", "BARLEY"),
    ("almidón", "CONTEXT_DEPENDENT", "NONE"),
    ("copos de avena", "KNOWN_GLUTEN", "OATS"),
    ("aceite de girasol", "KNOWN_SAFE_INGREDIENT", "NONE"),
];

const VERDICTS: &[&str] = &[
    "KNOWN_GLUTEN",
    "KNOWN_SAFE_INGREDIENT",
    "CONTEXT_DEPENDENT",
    "INSUFFICIENT_INFORMATION",
];
const CEREALS: &[&str] = &[
    "WHEAT",
    "BARLEY",
    "RYE",
    "OATS",
    "SPELT",
    "TRITICALE",
    "KAMUT",
    "NONE",
];

// The request is built from structs so the keys keep this order (`json!` sorts them):
// llama.cpp turns the schema into a grammar in property order, and the model should
// write the verdict before the cereal, as the system prompt and the examples show.

#[derive(Serialize)]
pub struct RequestBody {
    messages: Vec<Message>,
    temperature: u8,
    max_tokens: u32,
    cache_prompt: bool,
    response_format: ResponseFormat,
}

#[derive(Serialize)]
struct Message {
    role: &'static str,
    content: String,
}

#[derive(Serialize)]
struct ResponseFormat {
    #[serde(rename = "type")]
    kind: &'static str,
    schema: Schema,
}

#[derive(Serialize)]
struct Schema {
    #[serde(rename = "type")]
    kind: &'static str,
    properties: Properties,
    required: [&'static str; 2],
    #[serde(rename = "additionalProperties")]
    additional_properties: bool,
}

#[derive(Serialize)]
struct Properties {
    verdict: EnumProperty,
    cereal: EnumProperty,
}

#[derive(Serialize)]
struct EnumProperty {
    #[serde(rename = "type")]
    kind: &'static str,
    #[serde(rename = "enum")]
    values: &'static [&'static str],
}

#[derive(Serialize)]
struct ShotAnswer<'a> {
    verdict: &'a str,
    cereal: &'a str,
}

fn shot_answer(verdict: &str, cereal: &str) -> String {
    serde_json::to_string(&ShotAnswer { verdict, cereal }).expect("plain strings")
}

// ---------- answer validation ----------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
enum RawVerdict {
    KnownGluten,
    KnownSafeIngredient,
    ContextDependent,
    InsufficientInformation,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Cereal {
    Wheat,
    Barley,
    Rye,
    Oats,
    Spelt,
    Triticale,
    Kamut,
    None,
}

impl Cereal {
    fn spanish(self) -> Option<&'static str> {
        Some(match self {
            Cereal::Wheat => "trigo",
            Cereal::Barley => "cebada",
            Cereal::Rye => "centeno",
            Cereal::Oats => "avena",
            Cereal::Spelt => "espelta",
            Cereal::Triticale => "triticale",
            Cereal::Kamut => "kamut",
            Cereal::None => return None,
        })
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawAnswer {
    verdict: RawVerdict,
    cereal: Cereal,
}

/// A validated answer for one ingredient.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ItemAnswer {
    pub verdict: AiVerdict,
    pub cereal: Option<Cereal>,
}

impl ItemAnswer {
    const ERROR: ItemAnswer = ItemAnswer {
        verdict: AiVerdict::ModelError,
        cereal: None,
    };
}

/// Strict reading of the model's message: exactly the schema, consistent, or an error.
pub fn parse_answer(content: &str) -> Result<ItemAnswer, String> {
    let raw: RawAnswer =
        serde_json::from_str(content.trim()).map_err(|e| format!("not the schema: {e}"))?;
    let named = raw.cereal != Cereal::None;
    let verdict = match raw.verdict {
        RawVerdict::KnownGluten if named => AiVerdict::KnownGluten,
        RawVerdict::KnownGluten => return Err("gluten without a cereal".into()),
        _ if named => return Err("a cereal without KNOWN_GLUTEN".into()),
        RawVerdict::KnownSafeIngredient => AiVerdict::KnownSafeIngredient,
        RawVerdict::ContextDependent => AiVerdict::ContextDependent,
        RawVerdict::InsufficientInformation => AiVerdict::InsufficientInformation,
    };
    Ok(ItemAnswer {
        verdict,
        cereal: named.then_some(raw.cereal),
    })
}

// ---------- keys ----------

fn sha(text: &str) -> String {
    hex::encode(Sha256::digest(text.as_bytes()))
}

/// The analysis a review belongs to: same ingredients and same versions.
pub fn analysis_key(product: &Product, model: &str) -> String {
    let ingredients = product
        .ingredients_original
        .as_deref()
        .or(product.ingredients.as_deref())
        .map(normalize)
        .unwrap_or_default();
    format!(
        "{KEY_PREFIX}{}",
        sha(&format!(
            "{ENGINE_VERSION}|{RULES_VERSION}|{model}|{PROMPT_VERSION}|{ingredients}"
        ))
    )
}

/// Cache key of one ingredient's answer.
pub fn item_key(item: &str, model: &str) -> String {
    sha(&format!(
        "{}|{model}|{PROMPT_VERSION}|{RULES_VERSION}",
        normalize(item)
    ))
}

// ---------- queue ----------

#[derive(Debug, Clone)]
pub struct AiJob {
    pub product_id: String,
    pub key: String,
    pub items: Vec<String>,
    /// Failed attempts so far; the job is retried later up to `MAX_ATTEMPTS`.
    pub attempts: u8,
    /// A search result's review: yields to products she opens between ingredients.
    pub background: bool,
}

/// What became of a job's run.
#[derive(Debug, PartialEq, Eq)]
enum Ran {
    Stored,
    /// A product she opened is waiting: the rest of this background job waits (the
    /// ingredients already answered are cached, so it resumes where it stopped).
    Yielded,
}

/// A background job gives way between ingredients whenever a priority job waits.
fn should_yield(background: bool, urgent_waiting: usize) -> bool {
    background && urgent_waiting > 0
}

/// The background queue. `pending` lets responses say "Consultando…".
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
    /// Priority jobs sent and not yet taken by the worker.
    urgent: AtomicUsize,
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
            urgent: AtomicUsize::new(0),
        }
    }

    /// Send a job to its queue (priority or background), keeping the counters right.
    fn send(&self, job: AiJob) -> Result<(), AiJob> {
        let id = job.product_id.clone();
        if job.background {
            self.background.try_send(job).map_err(|e| e.into_inner())?;
            self.in_background
                .lock()
                .expect("background lock")
                .insert(id);
        } else {
            self.urgent.fetch_add(1, Ordering::SeqCst);
            if let Err(error) = self.sender.try_send(job) {
                self.urgent.fetch_sub(1, Ordering::SeqCst);
                return Err(error.into_inner());
            }
        }
        Ok(())
    }

    fn is_pending(&self, product_id: &str) -> bool {
        self.pending
            .lock()
            .expect("pending lock")
            .contains(product_id)
    }
}

/// Only these products can show an AI review: UNKNOWN for want of a few ingredients.
fn reviewable(item: &Product) -> bool {
    !item.user_created
        && item.assessment.status == GlutenStatus::Unknown
        && item.assessment.finding == Some(GlutenFinding::InsufficientInformation)
}

/// Ingredients worth asking about (`candidates`: those the rules could not classify),
/// or `None` when a gate rules the model out (see module docs).
pub fn eligible(
    item: &Product,
    candidates: &[String],
    language: &IngredientLanguage,
) -> Option<Vec<String>> {
    if !reviewable(item)
        || candidates.is_empty()
        || candidates.len() > MAX_ITEMS
        || language.unknown != Some(0)
        || language.parsed.is_empty()
    {
        return None;
    }
    let mut seen = HashSet::new();
    let mut items = Vec::new();
    for candidate in candidates {
        let text = candidate.split_whitespace().collect::<Vec<_>>().join(" ");
        if text.chars().count() > MAX_ITEM_CHARS {
            return None;
        }
        let normalized = normalize(&text);
        if BLOCKLIST.is_match(&SAFE_PHRASES.replace_all(&normalized, " ")) {
            return None;
        }
        if seen.insert(normalized) {
            items.push(text);
        }
    }
    (!items.is_empty()).then_some(items)
}

/// Queue a review unless one exists for this exact analysis (or is already queued).
pub async fn enqueue(state: &AppState, item: &Product, items: Vec<String>) {
    let Some(ai) = state.ai_check.as_ref() else {
        return;
    };
    let key = analysis_key(item, &ai.model);
    if already_checked(state, &item.id, &key).await {
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
        key,
        items,
        attempts: 0,
        background: false,
    };
    let (key, items) = (job.key.clone(), job.items.clone());
    if ai.send(job).is_err() {
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
    .bind(&key)
    .bind(&items)
    .execute(&state.db)
    .await;
    if let Err(error) = stored {
        tracing::warn!(%error, "AI job not persisted; it will be lost on restart");
    }
}

/// Queue a review for a search result. Not persisted (a restart just drops it) and
/// returns whether it was queued.
pub async fn enqueue_background(state: &AppState, item: &Product, items: Vec<String>) -> bool {
    let Some(ai) = state.ai_check.as_ref() else {
        return false;
    };
    let key = analysis_key(item, &ai.model);
    if already_checked(state, &item.id, &key).await {
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
        key,
        items,
        attempts: 0,
        background: true,
    };
    if ai.send(job).is_err() {
        ai.pending.lock().expect("pending lock").remove(&item.id);
        return false;
    }
    true
}

/// A stored review exists. A MODEL_ERROR one counts only for a day (the model may have
/// been down), then the product can be reviewed again.
async fn already_checked(state: &AppState, product_id: &str, key: &str) -> bool {
    sqlx::query_scalar::<_, i64>(
        "SELECT count(*) FROM product_ai_reviews WHERE product_id=$1 AND analysis_key=$2
         AND (outcome<>'MODEL_ERROR' OR checked_at>now()-interval '1 day')",
    )
    .bind(product_id)
    .bind(key)
    .fetch_one(&state.db)
    .await
    .map_or(true, |n| n > 0)
}

/// Jobs left over from before a restart go back into the queue (jobs of an older
/// version are dropped: their items were chosen by other rules).
async fn restore_queue(state: &AppState, ai: &AiChecker) {
    let _ = sqlx::query("DELETE FROM product_ai_queue WHERE ingredients_hash NOT LIKE $1")
        .bind(format!("{KEY_PREFIX}%"))
        .execute(&state.db)
        .await;
    let rows = sqlx::query_as::<_, (String, String, Vec<String>)>(
        "SELECT product_id,ingredients_hash,items FROM product_ai_queue ORDER BY queued_at LIMIT $1",
    )
    .bind(QUEUE as i64)
    .fetch_all(&state.db)
    .await
    .unwrap_or_default();
    let mut restored = 0;
    for (product_id, key, items) in rows {
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
            key,
            items,
            attempts: 0,
            background: false,
        };
        if let Err(job) = ai.send(job) {
            ai.pending
                .lock()
                .expect("pending lock")
                .remove(&job.product_id);
            break;
        }
        restored += 1;
    }
    if restored > 0 {
        tracing::info!(restored, "AI reviews restored after restart");
    }
}

/// The job is finished (stored, or given up): forget it.
async fn forget(state: &AppState, job: &AiJob) {
    let _ = sqlx::query("DELETE FROM product_ai_queue WHERE product_id=$1 AND ingredients_hash=$2")
        .bind(&job.product_id)
        .bind(&job.key)
        .execute(&state.db)
        .await;
}

// ---------- the model ----------

pub fn request_body(item: &str) -> RequestBody {
    let message = |role, content: String| Message { role, content };
    let mut messages = vec![message("system", SYSTEM.to_string())];
    for (ingredient, verdict, cereal) in SHOTS {
        messages.push(message("user", format!("Ingrediente: {ingredient}")));
        messages.push(message("assistant", shot_answer(verdict, cereal)));
    }
    messages.push(message("user", format!("Ingrediente: {item}")));
    RequestBody {
        messages,
        temperature: 0,
        max_tokens: MAX_TOKENS,
        cache_prompt: true,
        response_format: ResponseFormat {
            kind: "json_object",
            schema: Schema {
                kind: "object",
                properties: Properties {
                    verdict: EnumProperty {
                        kind: "string",
                        values: VERDICTS,
                    },
                    cereal: EnumProperty {
                        kind: "string",
                        values: CEREALS,
                    },
                },
                required: ["verdict", "cereal"],
                additional_properties: false,
            },
        },
    }
}

/// `Err` = the model could not be reached (retried later); `Ok(Err)` = it answered
/// outside the schema (MODEL_ERROR, not retried: the answer is deterministic).
async fn ask(ai: &AiChecker, item: &str) -> anyhow::Result<Result<ItemAnswer, String>> {
    let start = Instant::now();
    let response = async {
        let value: Value = ai
            .http
            .post(format!("{}/v1/chat/completions", ai.url))
            .timeout(REQUEST_TIMEOUT)
            .json(&request_body(item))
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        anyhow::Ok(value)
    }
    .await;
    let answer = response.map(|value| {
        let content = value["choices"][0]["message"]["content"]
            .as_str()
            .unwrap_or("");
        parse_answer(content)
    });
    metrics::record_ai_call(start.elapsed(), matches!(answer, Ok(Ok(_))));
    answer
}

async fn cached_answer(state: &AppState, key: &str) -> Option<ItemAnswer> {
    let (verdict, cereal) = sqlx::query_as::<_, (String, Option<String>)>(
        "SELECT verdict,cereal FROM ingredient_ai_verdicts WHERE cache_key=$1",
    )
    .bind(key)
    .fetch_optional(&state.db)
    .await
    .ok()??;
    let content = json!({"verdict": verdict, "cereal": cereal.unwrap_or_else(|| "NONE".into())});
    parse_answer(&content.to_string()).ok()
}

fn wire<T: Serialize>(value: T) -> String {
    serde_json::to_value(value)
        .ok()
        .and_then(|v| v.as_str().map(str::to_string))
        .unwrap_or_default()
}

/// Worst first: a suspicion is reported over an error, an error over a doubt.
pub fn outcome(answers: &[ItemAnswer]) -> AiOutcome {
    let has = |v: AiVerdict| answers.iter().any(|a| a.verdict == v);
    if answers.is_empty() {
        AiOutcome::ModelError
    } else if has(AiVerdict::KnownGluten) {
        AiOutcome::GlutenSuspected
    } else if has(AiVerdict::ModelError) {
        AiOutcome::ModelError
    } else if answers
        .iter()
        .all(|a| a.verdict == AiVerdict::KnownSafeIngredient)
    {
        AiOutcome::NoGlutenSeen
    } else {
        AiOutcome::Undetermined
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Evidence {
    ingredient: String,
    verdict: AiVerdict,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    cereal: Option<Cereal>,
}

async fn store(
    state: &AppState,
    ai: &AiChecker,
    job: &AiJob,
    evidence: &[Evidence],
) -> anyhow::Result<AiOutcome> {
    let answers: Vec<ItemAnswer> = evidence
        .iter()
        .map(|e| ItemAnswer {
            verdict: e.verdict,
            cereal: e.cereal,
        })
        .collect();
    let result = outcome(&answers);
    sqlx::query(
        "INSERT INTO product_ai_reviews(product_id,analysis_key,outcome,evidence,model,prompt_version,rules_version,engine_version)
         VALUES($1,$2,$3,$4,$5,$6,$7,$8) ON CONFLICT (product_id,analysis_key)
         DO UPDATE SET outcome=EXCLUDED.outcome, evidence=EXCLUDED.evidence, checked_at=now()",
    )
    .bind(&job.product_id)
    .bind(&job.key)
    .bind(wire(result))
    .bind(serde_json::to_value(evidence)?)
    .bind(&ai.model)
    .bind(PROMPT_VERSION)
    .bind(RULES_VERSION)
    .bind(ENGINE_VERSION)
    .execute(&state.db)
    .await?;
    Ok(result)
}

async fn run(state: &AppState, ai: &AiChecker, job: &AiJob) -> anyhow::Result<Ran> {
    let mut evidence = Vec::with_capacity(job.items.len());
    for item in &job.items {
        let key = item_key(item, &ai.model);
        let answer = match cached_answer(state, &key).await {
            Some(hit) => {
                metrics::record_ai_cache_hit();
                hit
            }
            None if should_yield(job.background, ai.urgent.load(Ordering::SeqCst)) => {
                metrics::record_ai_yield();
                return Ok(Ran::Yielded);
            }
            None => {
                let answer = match ask(ai, item).await {
                    Ok(answer) => answer,
                    Err(_) => ask(ai, item).await?, // one retry (model restarting, cold cache)
                };
                match answer {
                    Ok(answer) => {
                        let _ = sqlx::query(
                            "INSERT INTO ingredient_ai_verdicts(cache_key,ingredient,verdict,cereal,model,prompt_version,rules_version)
                             VALUES($1,$2,$3,$4,$5,$6,$7) ON CONFLICT (cache_key) DO NOTHING",
                        )
                        .bind(&key)
                        .bind(item)
                        .bind(wire(answer.verdict))
                        .bind(answer.cereal.map(wire))
                        .bind(&ai.model)
                        .bind(PROMPT_VERSION)
                        .bind(RULES_VERSION)
                        .execute(&state.db)
                        .await;
                        answer
                    }
                    Err(why) => {
                        tracing::warn!(product = %job.product_id, %why, "AI answer rejected");
                        ItemAnswer::ERROR
                    }
                }
            }
        };
        metrics::record_ai_verdict(answer.verdict);
        evidence.push(Evidence {
            ingredient: item.clone(),
            verdict: answer.verdict,
            cereal: answer.cereal,
        });
    }
    let result = store(state, ai, job, &evidence).await?;
    tracing::info!(product = %job.product_id, outcome = %wire(result), items = job.items.len(), "AI review stored");
    Ok(Ran::Stored)
}

/// The job still matches the stored product: same analysis (ingredients, rules, model)
/// and still a product the AI may review. Otherwise it is obsolete and dropped.
async fn still_current(state: &AppState, ai: &AiChecker, job: &AiJob) -> bool {
    let Ok(Some(data)) = sqlx::query_scalar::<_, Value>("SELECT data FROM products WHERE id=$1")
        .bind(&job.product_id)
        .fetch_optional(&state.db)
        .await
    else {
        // Database unavailable: keep the job (the run itself will fail and retry).
        return true;
    };
    serde_json::from_value::<Product>(data)
        .is_ok_and(|p| reviewable(&p) && analysis_key(&p, &ai.model) == job.key)
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
                Some(job) = receiver.recv() => {
                    ai.urgent.fetch_sub(1, Ordering::SeqCst);
                    job
                }
                Some(job) = background.recv() => job,
                else => break,
            };
            ai.in_background
                .lock()
                .expect("background lock")
                .remove(&job.product_id);
            // A promoted job runs twice in the queues: the second copy has nothing to do.
            // A job whose product changed since (new ingredients, rules) is obsolete.
            if already_checked(&state, &job.product_id, &job.key).await
                || !still_current(&state, ai, &job).await
            {
                forget(&state, &job).await;
                ai.pending
                    .lock()
                    .expect("pending lock")
                    .remove(&job.product_id);
                continue;
            }
            let started = Instant::now();
            let result = match tokio::time::timeout(JOB_TIMEOUT, run(&state, ai, &job)).await {
                Ok(result) => result,
                Err(_) => Err(anyhow::anyhow!("AI review took too long")),
            };
            metrics::record_ai_job(started.elapsed(), job.background);
            if let Ok(Ran::Yielded) = result {
                // Back to the end of the background queue; nothing is lost.
                if let Err(job) = ai.send(job) {
                    ai.pending
                        .lock()
                        .expect("pending lock")
                        .remove(&job.product_id);
                }
                continue;
            }
            if let Err(error) = result {
                job.attempts += 1;
                tracing::warn!(product = %job.product_id, attempts = job.attempts, %error, "AI review failed");
                if job.attempts < MAX_ATTEMPTS {
                    // Keep it marked as pending and try again in a few minutes.
                    let state = state.clone();
                    let delay = Duration::from_secs(120 * u64::from(job.attempts));
                    tokio::spawn(async move {
                        tokio::time::sleep(delay).await;
                        let ai = state.ai_check.as_ref().expect("checked above");
                        // Retried in its own queue: a search result never jumps ahead.
                        if let Err(job) = ai.send(job) {
                            ai.pending
                                .lock()
                                .expect("pending lock")
                                .remove(&job.product_id);
                        }
                    });
                    continue;
                }
                // Given up: say so (MODEL_ERROR, never gluten, never safe).
                let failed: Vec<Evidence> = job
                    .items
                    .iter()
                    .map(|i| Evidence {
                        ingredient: i.clone(),
                        verdict: AiVerdict::ModelError,
                        cereal: None,
                    })
                    .collect();
                if let Err(error) = store(&state, ai, &job, &failed).await {
                    tracing::warn!(%error, "AI failure not stored");
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

// ---------- what the app sees ----------

fn names(items: &[&str]) -> String {
    let quoted: Vec<String> = items.iter().map(|i| format!("«{i}»")).collect();
    match quoted.len() {
        0 => String::new(),
        1 => quoted[0].clone(),
        n => format!("{} y {}", quoted[..n - 1].join(", "), quoted[n - 1]),
    }
}

fn explanation(item: &str, answer: ItemAnswer) -> String {
    match answer.verdict {
        AiVerdict::KnownGluten => format!(
            "La IA relaciona «{item}» con {}. Sin confirmar: no lo dicen nuestras reglas ni la fuente.",
            answer.cereal.and_then(Cereal::spanish).unwrap_or("un cereal con gluten")
        ),
        AiVerdict::KnownSafeIngredient => {
            format!("La IA no relaciona «{item}» con cereales con gluten (orientativo).")
        }
        AiVerdict::ContextDependent => {
            format!("Según la IA, «{item}» puede llevar gluten o no según su origen.")
        }
        AiVerdict::InsufficientInformation => {
            format!("La IA no tiene información suficiente sobre «{item}».")
        }
        _ => format!("No se ha podido consultar a la IA sobre «{item}»."),
    }
}

/// Review shown to the app, built only from validated verdicts.
pub fn review(
    result: AiOutcome,
    evidence: &[(String, ItemAnswer)],
    model: &str,
    checked_at: Option<String>,
) -> AiReview {
    let pick = |f: &dyn Fn(&ItemAnswer) -> bool| -> String {
        names(
            &evidence
                .iter()
                .filter(|(_, a)| f(a))
                .map(|(i, _)| i.as_str())
                .collect::<Vec<_>>(),
        )
    };
    let message = match result {
        AiOutcome::NoGlutenSeen => format!(
            "Orientativo: la IA no relaciona {} con cereales con gluten. No es una verificación ni detecta trazas: revisa la etiqueta.",
            pick(&|_| true)
        ),
        AiOutcome::GlutenSuspected => format!(
            "Sin confirmar: la IA relaciona {} con un cereal con gluten, pero ni nuestras reglas ni la fuente lo confirman. No se ha podido determinar la presencia de gluten: revisa la etiqueta.",
            pick(&|a| a.verdict == AiVerdict::KnownGluten)
        ),
        AiOutcome::Undetermined => format!(
            "La IA tampoco ha podido clasificar {}. No se ha podido determinar la presencia de gluten: revisa la etiqueta.",
            pick(&|a| a.verdict != AiVerdict::KnownSafeIngredient)
        ),
        AiOutcome::Pending => "Consultando a la IA los ingredientes que no reconocemos…".into(),
        _ => "No se ha podido completar la consulta a la IA. El análisis de los ingredientes no cambia."
            .into(),
    };
    AiReview {
        outcome: result,
        message,
        items: evidence
            .iter()
            .map(|(item, answer)| AiItemVerdict {
                ingredient: item.clone(),
                verdict: answer.verdict,
                cereal: answer.cereal.and_then(Cereal::spanish).map(str::to_string),
                explanation: explanation(item, *answer),
            })
            .collect(),
        model: model.to_string(),
        rules_version: RULES_VERSION.into(),
        prompt_version: PROMPT_VERSION.into(),
        checked_at,
    }
}

/// The legacy field for installed 2.0.x apps, which show SUSPECTED in amber like a
/// traces warning: only "no gluten seen" (orientative, neutral) and "pending" are sent.
pub fn legacy(review: &AiReview) -> Option<AiCheck> {
    let status = match review.outcome {
        AiOutcome::NoGlutenSeen => AiCheckStatus::Verified,
        AiOutcome::Pending => AiCheckStatus::Pending,
        _ => return None,
    };
    Some(AiCheck {
        status,
        message: review.message.clone(),
        flagged: Vec::new(),
        model: review.model.clone(),
        checked_at: review.checked_at.clone(),
    })
}

/// Attach stored reviews (one query). Only products reviewable by the AI show one.
pub async fn attach(state: &AppState, items: Vec<Product>) -> Vec<Product> {
    let Some(ai) = state.ai_check.as_ref() else {
        return items;
    };
    let keys: Vec<(String, String)> = items
        .iter()
        .filter(|p| reviewable(p))
        .map(|p| (p.id.clone(), analysis_key(p, &ai.model)))
        .collect();
    if keys.is_empty() {
        return items;
    }
    let (ids, hashes): (Vec<String>, Vec<String>) = keys.iter().cloned().unzip();
    let rows = sqlx::query_as::<_, (String, String, String, Value, String)>(
        "SELECT product_id,analysis_key,outcome,evidence,to_char(checked_at AT TIME ZONE 'UTC','YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"')
         FROM product_ai_reviews WHERE product_id=ANY($1) AND analysis_key=ANY($2)",
    )
    .bind(&ids)
    .bind(&hashes)
    .fetch_all(&state.db)
    .await
    .unwrap_or_default();
    items
        .into_iter()
        .map(|mut item| {
            let Some((_, key)) = keys.iter().find(|(id, _)| *id == item.id) else {
                return item;
            };
            let stored = rows.iter().find(|r| r.0 == item.id && r.1 == *key);
            let found = match stored {
                Some((_, _, _, evidence, at)) => {
                    let evidence: Vec<(String, ItemAnswer)> =
                        serde_json::from_value::<Vec<Evidence>>(evidence.clone())
                            .unwrap_or_default()
                            .into_iter()
                            .map(|e| {
                                (
                                    e.ingredient,
                                    ItemAnswer {
                                        verdict: e.verdict,
                                        cereal: e.cereal,
                                    },
                                )
                            })
                            .collect();
                    // Recomputed from the evidence: the stored text is never trusted alone.
                    let answers: Vec<ItemAnswer> = evidence.iter().map(|(_, a)| *a).collect();
                    Some(review(
                        outcome(&answers),
                        &evidence,
                        &ai.model,
                        Some(at.clone()),
                    ))
                }
                None if ai.is_pending(&item.id) => {
                    Some(review(AiOutcome::Pending, &[], &ai.model, None))
                }
                None => None,
            };
            item.ai_check = found.as_ref().and_then(legacy);
            item.ai_review = found;
            item
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_background_jobs_yield_and_only_to_waiting_priority_jobs() {
        assert!(!should_yield(false, 0));
        assert!(!should_yield(false, 3));
        assert!(!should_yield(true, 0));
        assert!(should_yield(true, 1));
    }

    #[test]
    fn queue_counters_follow_sends() {
        let ai = AiChecker::new(reqwest::Client::new(), "http://x".into(), "m".into());
        let job = |background| AiJob {
            product_id: format!("p{background}"),
            key: "k".into(),
            items: vec!["a".into()],
            attempts: 0,
            background,
        };
        ai.send(job(false)).unwrap();
        assert_eq!(ai.urgent.load(Ordering::SeqCst), 1);
        ai.send(job(true)).unwrap();
        assert_eq!(ai.urgent.load(Ordering::SeqCst), 1);
        assert!(ai.in_background.lock().unwrap().contains("ptrue"));
        // A full priority queue leaves the counter as it was.
        for _ in 0..QUEUE {
            let _ = ai.send(job(false));
        }
        assert_eq!(ai.urgent.load(Ordering::SeqCst), QUEUE);
        assert!(ai.send(job(false)).is_err());
        assert_eq!(ai.urgent.load(Ordering::SeqCst), QUEUE);
    }
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
                engine_version: ENGINE_VERSION.into(),
                certification: None,
                finding: Some(GlutenFinding::InsufficientInformation),
                rules_version: Some(RULES_VERSION.into()),
                items: vec![],
                provenance: None,
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
            ai_review: None,
            freshness: None,
            data_updated_at: None,
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

    fn strings(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn valid_answers_are_read() {
        assert_eq!(
            parse_answer(r#"{"verdict":"KNOWN_GLUTEN","cereal":"BARLEY"}"#),
            Ok(ItemAnswer {
                verdict: AiVerdict::KnownGluten,
                cereal: Some(Cereal::Barley)
            })
        );
        assert_eq!(
            parse_answer(" {\"verdict\": \"KNOWN_SAFE_INGREDIENT\", \"cereal\": \"NONE\"}\n")
                .unwrap()
                .verdict,
            AiVerdict::KnownSafeIngredient
        );
        assert_eq!(
            parse_answer(r#"{"verdict":"CONTEXT_DEPENDENT","cereal":"NONE"}"#)
                .unwrap()
                .verdict,
            AiVerdict::ContextDependent
        );
    }

    #[test]
    fn malformed_empty_or_inconsistent_answers_are_errors() {
        for bad in [
            "",
            "sí",
            "no",
            "{}",
            r#"{"verdict":"KNOWN_GLUTEN"}"#,
            r#"{"verdict":"MAYBE","cereal":"NONE"}"#,
            r#"{"verdict":"KNOWN_SAFE_INGREDIENT","cereal":"NONE","why":"x"}"#,
            r#"{"verdict":"KNOWN_GLUTEN","cereal":"NONE"}"#,
            r#"{"verdict":"KNOWN_SAFE_INGREDIENT","cereal":"WHEAT"}"#,
            r#"```json {"verdict":"KNOWN_SAFE_INGREDIENT","cereal":"NONE"} ```"#,
            r#"{"verdict":"KNOWN_SAFE_INGREDIENT","cereal":"NONE"} y además"#,
        ] {
            assert!(parse_answer(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn errors_are_never_gluten_and_never_safe() {
        let error = ItemAnswer::ERROR;
        let safe = ItemAnswer {
            verdict: AiVerdict::KnownSafeIngredient,
            cereal: None,
        };
        let gluten = ItemAnswer {
            verdict: AiVerdict::KnownGluten,
            cereal: Some(Cereal::Wheat),
        };
        let doubt = ItemAnswer {
            verdict: AiVerdict::InsufficientInformation,
            cereal: None,
        };
        assert_eq!(outcome(&[]), AiOutcome::ModelError);
        assert_eq!(outcome(&[error]), AiOutcome::ModelError);
        assert_eq!(outcome(&[safe, error]), AiOutcome::ModelError);
        assert_eq!(outcome(&[safe, safe]), AiOutcome::NoGlutenSeen);
        assert_eq!(outcome(&[safe, doubt]), AiOutcome::Undetermined);
        assert_eq!(outcome(&[error, gluten]), AiOutcome::GlutenSuspected);
        // Old apps only ever see "no gluten seen" or "pending", never an amber suspicion.
        let r = review(
            AiOutcome::GlutenSuspected,
            &[("kamut".into(), gluten)],
            "m",
            None,
        );
        assert!(legacy(&r).is_none());
        assert!(r.message.starts_with("Sin confirmar"));
        assert!(r.items[0].explanation.contains("trigo"));
        let r = review(AiOutcome::ModelError, &[("x".into(), error)], "m", None);
        assert!(legacy(&r).is_none());
        let r = review(
            AiOutcome::NoGlutenSeen,
            &[("garrofín".into(), safe)],
            "m",
            None,
        );
        assert_eq!(legacy(&r).unwrap().status, AiCheckStatus::Verified);
        assert!(!r.message.to_lowercase().contains("sin gluten"));
    }

    #[test]
    fn the_request_asks_for_the_closed_schema() {
        let text = serde_json::to_string(&request_body("garrofín")).unwrap();
        // The verdict comes before the cereal, in the schema and in every example.
        assert!(text.find("\"verdict\":{").unwrap() < text.find("\"cereal\":{").unwrap());
        assert!(text.contains(r#"{\"verdict\":\"KNOWN_GLUTEN\",\"cereal\":\"WHEAT\"}"#));
        let body: Value = serde_json::from_str(&text).unwrap();
        assert_eq!(body["response_format"]["type"], "json_object");
        assert_eq!(
            body["response_format"]["schema"]["additionalProperties"],
            false
        );
        assert_eq!(body["temperature"], 0);
        // Every example answer is itself valid under the schema.
        for (_, verdict, cereal) in SHOTS {
            let answer = shot_answer(verdict, cereal);
            assert!(parse_answer(&answer).is_ok(), "{answer}");
        }
        let last = body["messages"].as_array().unwrap().last().unwrap();
        assert_eq!(last["content"], "Ingrediente: garrofín");
    }

    #[test]
    fn keys_change_with_ingredients_and_versions() {
        let a = product("Patatas, sal, garrofín.");
        let mut b = a.clone();
        b.ingredients = Some("Patatas, sal, garrofín, azúcar.".into());
        assert_ne!(analysis_key(&a, "m"), analysis_key(&b, "m"));
        assert_ne!(analysis_key(&a, "m"), analysis_key(&a, "other-model"));
        assert!(analysis_key(&a, "m").starts_with(KEY_PREFIX));
        // Case and accents do not change the ingredient key; the model does.
        assert_eq!(item_key("Garrofín", "m"), item_key("garrofin", "m"));
        assert_ne!(item_key("garrofin", "m"), item_key("garrofin", "n"));
    }

    #[test]
    fn only_unclassified_ingredients_can_be_asked() {
        let item = product("Arroz, garrofín, sal.");
        assert_eq!(
            eligible(&item, &strings(&["garrofín"]), &recognised()),
            Some(strings(&["garrofín"]))
        );
        // Nothing left for the AI, OFF did not recognise everything, too many items.
        assert!(eligible(&item, &[], &recognised()).is_none());
        let mut unknown = recognised();
        unknown.unknown = Some(1);
        assert!(eligible(&item, &strings(&["garrofín"]), &unknown).is_none());
        assert!(eligible(
            &item,
            &strings(&["garrofín"]),
            &IngredientLanguage::default()
        )
        .is_none());
        let many: Vec<String> = (0..=MAX_ITEMS).map(|i| format!("x{i}")).collect();
        assert!(eligible(&item, &many, &recognised()).is_none());
        // Another finding (ambiguous, no gluten identified...) or status: no AI.
        for finding in [
            GlutenFinding::AmbiguousIngredient,
            GlutenFinding::NoGlutenIdentified,
            GlutenFinding::Contradictory,
        ] {
            let mut other = item.clone();
            other.assessment.finding = Some(finding);
            assert!(eligible(&other, &strings(&["garrofín"]), &recognised()).is_none());
        }
        let mut claimed = item.clone();
        claimed.assessment.status = GlutenStatus::GlutenFree;
        assert!(eligible(&claimed, &strings(&["garrofín"]), &recognised()).is_none());
    }

    #[test]
    fn blocklist_keeps_vague_and_foreign_gluten_sources_out() {
        let item = product("x");
        for text in [
            "bulgur",
            "seitán",
            "escanda",
            "farro perlado",
            "freekeh",
            "pan rallado",
            "harina",
            "farine de blé",
            "farina di frumento",
            "Weizenmehl",
            "proteína vegetal hidrolizada",
            "cerveza",
            "almidón",
            "obleas",
            "graham crackers",
            "kamut",
            "Dinkel",
        ] {
            assert!(
                eligible(&item, &strings(&[text]), &recognised()).is_none(),
                "{text}"
            );
        }
        for text in ["harina de arroz", "garrofín", "almidón de maíz"] {
            assert!(
                eligible(&item, &strings(&[text]), &recognised()).is_some(),
                "{text}"
            );
        }
    }
}

#[cfg(test)]
mod gate_regression {
    use super::*;
    use safeeat_gluten_engine::{analyze, Context, GlutenInput, Origin};

    /// Every gluten product of the evaluation set must be stopped before the model and
    /// never read as "no gluten identified": either the rules find it, or they leave it
    /// ambiguous, or the gates rule the model out. (The 1.5B model alone let 1-2 of these
    /// through as "safe".)
    #[test]
    fn no_gluten_case_reaches_the_model() {
        let cases: Vec<Value> =
            serde_json::from_str(include_str!("../tests/fixtures/ai-gluten-cases.json")).unwrap();
        let (mut reassuring, mut safe_cases) = (0, 0);
        for case in &cases {
            let text = case["t"].as_str().unwrap();
            let analysis = analyze(
                &GlutenInput {
                    ingredients: Some(text.into()),
                    source: "test".into(),
                    origin: Some(Origin::Provider),
                    ..Default::default()
                },
                Context::default(),
            );
            let mut item = tests_support::product(text);
            item.assessment = analysis.assessment.clone();
            let reaches_model =
                eligible(&item, &analysis.ai_candidates, &tests_support::recognised()).is_some();
            if case["g"] == 1 {
                assert!(!reaches_model, "gluten case reached the model: {text}");
                assert_ne!(
                    item.assessment.finding,
                    Some(GlutenFinding::NoGlutenIdentified),
                    "gluten case read as no gluten identified: {text}"
                );
                assert_ne!(item.assessment.status, GlutenStatus::GlutenFree, "{text}");
            } else {
                safe_cases += 1;
                // The wider vocabulary must not flag real gluten-free lists.
                assert!(
                    !matches!(
                        item.assessment.status,
                        GlutenStatus::ContainsGluten | GlutenStatus::MayContainTraces
                    ),
                    "false positive on a gluten-free list: {text}"
                );
                if item.assessment.finding == Some(GlutenFinding::NoGlutenIdentified) {
                    reassuring += 1;
                }
            }
        }
        // Most plain gluten-free lists are resolved by the rules, without the AI.
        assert!(
            reassuring * 2 >= safe_cases,
            "only {reassuring} of {safe_cases} safe cases resolved by the rules"
        );
    }
}

#[cfg(test)]
mod tests_support {
    pub use super::tests::{product, recognised};
}
