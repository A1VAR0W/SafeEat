//! Environment configuration (port of apps/api/src/config/env.ts).

use anyhow::{bail, Context, Result};
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeEnv {
    Development,
    Test,
    Production,
}

#[derive(Debug, Clone)]
pub struct Config {
    pub node_env: NodeEnv,
    pub app_env: String,
    pub port: u16,
    pub database_url: String,
    pub jwt_access_secret: String,
    pub jwt_refresh_secret: String,
    pub public_app_url: String,
    pub api_public_url: String,
    pub openfoodfacts_user_agent: String,
    pub openfoodfacts_base_url: String,
    pub openfoodfacts_search_url: String,
    pub usda_fdc_api_key: Option<String>,
    /// llama.cpp server for the orientative AI analysis (unset: feature off).
    pub ai_local_url: Option<String>,
    pub ai_local_model: String,
    pub trust_proxy: u8,
}

fn url(value: &str, name: &str) -> Result<String> {
    let parsed = url::Url::parse(value).with_context(|| format!("{name} must be a URL"))?;
    if !matches!(parsed.scheme(), "http" | "https") {
        bail!("{name} must be http(s)");
    }
    Ok(value.to_string())
}

fn insecure(secret: &str) -> bool {
    let lower = secret.to_lowercase();
    ["example", "change", "development", "test-secret"]
        .iter()
        .any(|w| lower.contains(w))
        || secret
            .chars()
            .collect::<std::collections::HashSet<_>>()
            .len()
            < 16
}

impl Config {
    pub fn from_env() -> Result<Self> {
        Self::from_map(&std::env::vars().collect())
    }

    pub fn from_map(env: &HashMap<String, String>) -> Result<Self> {
        let get = |k: &str| {
            env.get(k)
                .map(|v| v.trim().to_string())
                .filter(|v| !v.is_empty())
        };
        let mut invalid = Vec::new();
        let node_env = match get("NODE_ENV").as_deref() {
            None | Some("development") => NodeEnv::Development,
            Some("test") => NodeEnv::Test,
            Some("production") => NodeEnv::Production,
            Some(_) => {
                invalid.push("NODE_ENV");
                NodeEnv::Development
            }
        };
        let app_env = get("APP_ENV").unwrap_or_else(|| "development".into());
        if !["development", "preproduction", "production"].contains(&app_env.as_str()) {
            invalid.push("APP_ENV");
        }
        let port = get("PORT")
            .map(|v| v.parse::<u16>())
            .unwrap_or(Ok(4000))
            .unwrap_or_else(|_| {
                invalid.push("PORT");
                0
            });
        let database_url = get("DATABASE_URL").unwrap_or_default();
        if !database_url.starts_with("postgres") {
            invalid.push("DATABASE_URL");
        }
        let jwt_access_secret = get("JWT_ACCESS_SECRET").unwrap_or_default();
        if jwt_access_secret.len() < 48 {
            invalid.push("JWT_ACCESS_SECRET");
        }
        let jwt_refresh_secret = get("JWT_REFRESH_SECRET").unwrap_or_default();
        if jwt_refresh_secret.len() < 48 {
            invalid.push("JWT_REFRESH_SECRET");
        }
        let trust_proxy = get("TRUST_PROXY")
            .map(|v| v.parse::<u8>())
            .unwrap_or(Ok(0))
            .unwrap_or(9);
        if trust_proxy > 2 {
            invalid.push("TRUST_PROXY");
        }
        let user_agent = get("OPENFOODFACTS_USER_AGENT")
            .unwrap_or_else(|| "SafeEat/2.0 (https://github.com/A1VAR0W/SafeEat)".into());
        if user_agent.len() < 10 {
            invalid.push("OPENFOODFACTS_USER_AGENT");
        }
        if !invalid.is_empty() {
            bail!("Invalid environment variables: {}", invalid.join(", "));
        }
        let public_app_url = url(
            &get("PUBLIC_APP_URL").unwrap_or_else(|| "http://localhost:3000".into()),
            "PUBLIC_APP_URL",
        )?;
        let api_public_url = url(
            &get("API_PUBLIC_URL").unwrap_or_else(|| "http://localhost:4000".into()),
            "API_PUBLIC_URL",
        )?;
        let config = Config {
            node_env,
            app_env,
            port,
            database_url,
            jwt_access_secret,
            jwt_refresh_secret,
            public_app_url,
            api_public_url,
            openfoodfacts_user_agent: user_agent,
            openfoodfacts_base_url: url(
                &get("OPENFOODFACTS_BASE_URL")
                    .unwrap_or_else(|| "https://world.openfoodfacts.org".into()),
                "OPENFOODFACTS_BASE_URL",
            )?,
            openfoodfacts_search_url: get("OPENFOODFACTS_SEARCH_URL")
                .unwrap_or_else(|| "https://search.openfoodfacts.org".into()),
            usda_fdc_api_key: get("USDA_FDC_API_KEY"),
            ai_local_url: get("AI_LOCAL_URL")
                .map(|v| url(&v, "AI_LOCAL_URL"))
                .transpose()?,
            ai_local_model: get("AI_LOCAL_MODEL")
                .unwrap_or_else(|| "Qwen2.5 1.5B en el servidor de SafeEat".into()),
            trust_proxy,
        };
        if config.node_env == NodeEnv::Production {
            if ![&config.api_public_url, &config.public_app_url]
                .iter()
                .all(|v| v.starts_with("https://"))
            {
                bail!("Production public URLs must use HTTPS");
            }
            if insecure(&config.jwt_access_secret) || insecure(&config.jwt_refresh_secret) {
                bail!("Insecure production secret");
            }
            if config.jwt_access_secret == config.jwt_refresh_secret {
                bail!("Access and refresh secrets must differ");
            }
            if config.app_env == "development" {
                bail!("Production runtime requires explicit APP_ENV");
            }
        }
        Ok(config)
    }

    pub fn api_base(&self) -> &str {
        self.api_public_url.trim_end_matches('/')
    }

    pub fn public_base(&self) -> &str {
        self.public_app_url.trim_end_matches('/')
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base() -> HashMap<String, String> {
        [
            ("DATABASE_URL", "postgres://x"),
            (
                "JWT_ACCESS_SECRET",
                "a1b2c3d4e5f6g7h8i9j0k1l2m3n4o5p6q7r8s9t0u1v2w3x4y5z6",
            ),
            (
                "JWT_REFRESH_SECRET",
                "z6y5x4w3v2u1t0s9r8q7p6o5n4m3l2k1j0i9h8g7f6e5d4c3b2a1",
            ),
        ]
        .into_iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect()
    }

    #[test]
    fn defaults() {
        let c = Config::from_map(&base()).unwrap();
        assert_eq!(c.port, 4000);
        assert_eq!(c.app_env, "development");
    }

    #[test]
    fn production_requires_https_and_app_env() {
        let mut env = base();
        env.insert("NODE_ENV".into(), "production".into());
        assert!(Config::from_map(&env).is_err());
        env.insert("PUBLIC_APP_URL".into(), "https://a.test".into());
        env.insert("API_PUBLIC_URL".into(), "https://a.test".into());
        assert!(Config::from_map(&env).is_err());
        env.insert("APP_ENV".into(), "production".into());
        assert!(Config::from_map(&env).is_ok());
    }

    #[test]
    fn rejects_short_secrets() {
        let mut env = base();
        env.insert("JWT_ACCESS_SECRET".into(), "short".into());
        assert!(Config::from_map(&env).is_err());
    }
}
