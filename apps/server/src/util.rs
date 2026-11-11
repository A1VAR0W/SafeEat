//! Small helpers: timestamps, validation and escaping.

use chrono::{DateTime, Utc};
use once_cell::sync::Lazy;
use regex::Regex;
use serde_json::Value;

use crate::error::{ApiError, ApiResult};

/// JavaScript `Date.prototype.toISOString()` format (millisecond precision, `Z`).
pub fn iso(value: DateTime<Utc>) -> String {
    value.format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string()
}

pub fn now_iso() -> String {
    iso(Utc::now())
}

/// SQL expression that renders a timestamptz column like [`iso`].
pub fn sql_iso(column: &str) -> String {
    format!(r#"to_char({column} AT TIME ZONE 'UTC','YYYY-MM-DD"T"HH24:MI:SS.MS"Z"')"#)
}

static EMAIL: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^[A-Za-z0-9_'+\-.]*[A-Za-z0-9_+\-]@([A-Za-z0-9][A-Za-z0-9\-]*\.)+[A-Za-z]{2,}$")
        .unwrap()
});

/// Zod `z.email()` (without the lookaheads, checked explicitly) + `.max(254)` + trim/lowercase.
pub fn email(value: &str) -> ApiResult<String> {
    if value.len() > 254 || value.starts_with('.') || value.contains("..") || !EMAIL.is_match(value)
    {
        return Err(ApiError::invalid_input());
    }
    Ok(value.trim().to_lowercase())
}

/// Length in UTF-16 code units, like JavaScript `string.length` (what Zod checks).
pub fn js_len(value: &str) -> usize {
    value.encode_utf16().count()
}

/// Trimmed string with Zod-like `min`/`max` (UTF-16) bounds.
pub fn trimmed(value: &str, min: usize, max: usize) -> ApiResult<String> {
    let value = value.trim().to_string();
    let len = js_len(&value);
    if len < min || len > max {
        return Err(ApiError::invalid_input());
    }
    Ok(value)
}

pub fn bounded(value: &str, min: usize, max: usize) -> ApiResult<()> {
    let len = js_len(value);
    if len < min || len > max {
        return Err(ApiError::invalid_input());
    }
    Ok(())
}

pub fn is_uuid(value: &str) -> bool {
    uuid::Uuid::parse_str(value).is_ok() && value.len() == 36
}

pub fn uuid_param(value: &str) -> ApiResult<uuid::Uuid> {
    if !is_uuid(value) {
        return Err(ApiError::invalid_input());
    }
    uuid::Uuid::parse_str(value).map_err(|_| ApiError::invalid_input())
}

pub fn barcode_param(value: &str) -> ApiResult<String> {
    if !safeeat_contracts::is_barcode(value) {
        return Err(ApiError::invalid_input());
    }
    Ok(value.to_string())
}

static RESTAURANT_ID: Lazy<Regex> = Lazy::new(|| Regex::new(r"^[A-Za-z0-9_-]{1,200}$").unwrap());

pub fn restaurant_id(value: &str) -> ApiResult<String> {
    if !RESTAURANT_ID.is_match(value) {
        return Err(ApiError::invalid_input());
    }
    Ok(value.to_string())
}

/// Parse a JSON object strictly: unknown keys are rejected (Zod `.strict()`).
pub fn strict_object<'a>(
    value: &'a Value,
    allowed: &[&str],
) -> ApiResult<&'a serde_json::Map<String, Value>> {
    let object = value.as_object().ok_or_else(ApiError::invalid_input)?;
    if object.keys().any(|k| !allowed.contains(&k.as_str())) {
        return Err(ApiError::invalid_input());
    }
    Ok(object)
}

pub fn req_str<'a>(object: &'a serde_json::Map<String, Value>, key: &str) -> ApiResult<&'a str> {
    object
        .get(key)
        .and_then(Value::as_str)
        .ok_or_else(ApiError::invalid_input)
}

pub fn opt_str<'a>(
    object: &'a serde_json::Map<String, Value>,
    key: &str,
) -> ApiResult<Option<&'a str>> {
    match object.get(key) {
        None => Ok(None),
        Some(Value::String(s)) => Ok(Some(s)),
        Some(_) => Err(ApiError::invalid_input()),
    }
}

pub fn opt_bool(object: &serde_json::Map<String, Value>, key: &str) -> ApiResult<Option<bool>> {
    match object.get(key) {
        None => Ok(None),
        Some(Value::Bool(b)) => Ok(Some(*b)),
        Some(_) => Err(ApiError::invalid_input()),
    }
}

pub fn req_f64(
    object: &serde_json::Map<String, Value>,
    key: &str,
    min: f64,
    max: f64,
) -> ApiResult<f64> {
    let value = object
        .get(key)
        .and_then(Value::as_f64)
        .ok_or_else(ApiError::invalid_input)?;
    if !(min..=max).contains(&value) {
        return Err(ApiError::invalid_input());
    }
    Ok(value)
}

/// `z.coerce.number().min().max().optional()` for query strings.
pub fn query_number(value: Option<&String>, min: f64, max: f64) -> ApiResult<Option<f64>> {
    let Some(raw) = value else { return Ok(None) };
    let trimmed = raw.trim();
    let number = if trimmed.is_empty() {
        0.0
    } else {
        trimmed
            .parse::<f64>()
            .map_err(|_| ApiError::invalid_input())?
    };
    if !number.is_finite() || !(min..=max).contains(&number) {
        return Err(ApiError::invalid_input());
    }
    Ok(Some(number))
}

pub fn query_bool(value: Option<&String>) -> ApiResult<Option<bool>> {
    match value.map(String::as_str) {
        None => Ok(None),
        Some("true") => Ok(Some(true)),
        Some("false") => Ok(Some(false)),
        Some(_) => Err(ApiError::invalid_input()),
    }
}

/// Escape `%`, `_` and `\` for SQL `ILIKE` patterns.
pub fn like_escape(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for c in value.chars() {
        if matches!(c, '\\' | '%' | '_') {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

pub fn html_escape(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for c in value.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(c),
        }
    }
    out
}

/// Only http(s) URLs are passed on to clients.
pub fn safe_http_url(value: Option<&str>) -> Option<String> {
    let parsed = url::Url::parse(value?).ok()?;
    matches!(parsed.scheme(), "http" | "https").then(|| parsed.to_string())
}

/// ISO date `YYYY-MM-DD` within [1900-01-01, today).
pub fn date_of_birth(value: &str) -> ApiResult<String> {
    let date = chrono::NaiveDate::parse_from_str(value, "%Y-%m-%d")
        .map_err(|_| ApiError::invalid_input())?;
    if value.len() != 10 || value < "1900-01-01" || date >= Utc::now().date_naive() {
        return Err(ApiError::invalid_input());
    }
    Ok(value.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn emails() {
        assert_eq!(email("Ana@Example.ES").unwrap(), "ana@example.es");
        assert!(email(" ana@example.es").is_err());
        assert!(email("a..b@example.es").is_err());
        assert!(email("nope").is_err());
    }

    #[test]
    fn iso_format() {
        let t = DateTime::parse_from_rfc3339("2026-11-11T10:20:30.123456Z")
            .unwrap()
            .with_timezone(&Utc);
        assert_eq!(iso(t), "2026-11-11T10:20:30.123Z");
    }

    #[test]
    fn dates() {
        assert!(date_of_birth("1990-02-28").is_ok());
        assert!(date_of_birth("1890-01-01").is_err());
        assert!(date_of_birth("2999-01-01").is_err());
        assert!(date_of_birth("1990-2-28").is_err());
    }
}
