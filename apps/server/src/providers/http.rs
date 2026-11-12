use std::collections::{HashMap, VecDeque};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde_json::Value;

use crate::error::ApiError;

/// Fetch JSON from a provider. `Ok(None)` on 404; any other failure is a 503.
pub async fn provider_json(
    request: reqwest::RequestBuilder,
    timeout: Duration,
) -> Result<Option<Value>, ApiError> {
    let response = request
        .timeout(timeout)
        .send()
        .await
        .map_err(|_| ApiError::provider_unavailable())?;
    if response.status() == reqwest::StatusCode::NOT_FOUND {
        return Ok(None);
    }
    if !response.status().is_success() {
        return Err(ApiError::provider_unavailable());
    }
    response
        .json::<Value>()
        .await
        .map(Some)
        .map_err(|_| ApiError::provider_unavailable())
}

/// Single-process provider budget (calls per window).
pub struct ProviderBudget {
    limit: usize,
    window: Duration,
    calls: Mutex<VecDeque<Instant>>,
}

impl ProviderBudget {
    pub fn new(limit: usize) -> Self {
        Self {
            limit,
            window: Duration::from_secs(60),
            calls: Mutex::new(VecDeque::new()),
        }
    }

    pub fn take(&self) -> Result<(), ApiError> {
        let now = Instant::now();
        let mut calls = self.calls.lock().expect("budget lock");
        while calls
            .front()
            .is_some_and(|t| now.duration_since(*t) >= self.window)
        {
            calls.pop_front();
        }
        if calls.len() >= self.limit {
            return Err(ApiError::new(
                503,
                "PROVIDER_BUSY",
                "La fuente está ocupada. Espera un minuto antes de volver a intentarlo.",
            ));
        }
        calls.push_back(now);
        Ok(())
    }

    /// Calls still free in the current window (background work leaves room for her).
    pub fn spare(&self) -> usize {
        let now = Instant::now();
        let calls = self.calls.lock().expect("budget lock");
        let used = calls
            .iter()
            .filter(|t| now.duration_since(**t) < self.window)
            .count();
        self.limit.saturating_sub(used)
    }
}

/// Small bounded TTL cache (insertion-ordered eviction).
type Entries<T> = (HashMap<String, (Instant, T)>, VecDeque<String>);

pub struct TtlCache<T: Clone> {
    ttl: Duration,
    capacity: usize,
    entries: Mutex<Entries<T>>,
}

impl<T: Clone> TtlCache<T> {
    pub fn new(ttl: Duration, capacity: usize) -> Self {
        Self {
            ttl,
            capacity,
            entries: Mutex::new((HashMap::new(), VecDeque::new())),
        }
    }

    pub fn get(&self, key: &str) -> Option<T> {
        let entries = self.entries.lock().expect("cache lock");
        entries
            .0
            .get(key)
            .filter(|(at, _)| at.elapsed() < self.ttl)
            .map(|(_, v)| v.clone())
    }

    pub fn put(&self, key: String, value: T) -> T {
        let mut entries = self.entries.lock().expect("cache lock");
        if !entries.0.contains_key(&key) {
            if entries.1.len() >= self.capacity {
                if let Some(old) = entries.1.pop_front() {
                    entries.0.remove(&old);
                }
            }
            entries.1.push_back(key.clone());
        }
        entries.0.insert(key, (Instant::now(), value.clone()));
        value
    }
}
