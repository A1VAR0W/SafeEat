//! SafeEat server.
//!
//! ```text
//! safeeat-server [serve]            API on $PORT (default 4000)
//! safeeat-server serve --mode web   public viewer on $PORT (default 3000)
//! safeeat-server migrate            apply db/migrations ($MIGRATIONS_DIR)
//! safeeat-server import-restaurants <region> <file.ndjson>
//! safeeat-server healthcheck <url>  exit 0 when <url> answers 2xx (Docker HEALTHCHECK)
//! ```

mod admin;
mod ai_check;
mod app;
mod auth;
mod config;
mod error;
mod extract;
mod favorites;
mod importer;
mod metrics;
mod migrate;
mod permissions;
mod products;
mod providers;
mod ratelimit;
mod restaurants;
mod reviews;
mod sharing;
mod state;
mod translate;
mod util;
mod web;

use std::net::SocketAddr;
use std::str::FromStr;
use std::time::Duration;

use anyhow::{bail, Context, Result};
use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
use sqlx::PgPool;

async fn pool(url: &str, max: u32) -> Result<PgPool> {
    let options = PgConnectOptions::from_str(url)
        .context("DATABASE_URL")?
        .options([("statement_timeout", "10000")]);
    Ok(PgPoolOptions::new()
        .max_connections(max)
        .idle_timeout(Duration::from_secs(30))
        .acquire_timeout(Duration::from_secs(5))
        .connect_with(options)
        .await?)
}

async fn shutdown() {
    let ctrl_c = async {
        let _ = tokio::signal::ctrl_c().await;
    };
    #[cfg(unix)]
    let terminate = async {
        if let Ok(mut s) = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        {
            s.recv().await;
        }
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();
    tokio::select! { _ = ctrl_c => {}, _ = terminate => {} }
}

async fn serve_api() -> Result<()> {
    let config = config::Config::from_env()?;
    let db = pool(&config.database_url, 10).await?;
    let port = config.port;
    let state = state::Inner::new(config, db)?;
    ai_check::spawn_worker(state.clone());
    metrics::spawn_logger();
    let listener = tokio::net::TcpListener::bind(SocketAddr::from(([0, 0, 0, 0], port))).await?;
    tracing::info!(port, ai = state.ai_check.is_some(), "SafeEat API listening");
    axum::serve(
        listener,
        app::router(state).into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(shutdown())
    .await?;
    Ok(())
}

async fn serve_web() -> Result<()> {
    let config = web::WebConfig::from_env()?;
    let port = config.port;
    let listener = tokio::net::TcpListener::bind(SocketAddr::from(([0, 0, 0, 0], port))).await?;
    tracing::info!(port, "SafeEat web viewer listening");
    axum::serve(listener, web::router(config)?)
        .with_graceful_shutdown(shutdown())
        .await?;
    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .json()
        .init();
    let args: Vec<String> = std::env::args().skip(1).collect();
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    match args.as_slice() {
        [] | ["serve"] | ["serve", "--mode", "api"] | ["--mode", "api"] => serve_api().await,
        ["serve", "--mode", "web"] | ["--mode", "web"] => serve_web().await,
        ["migrate"] => {
            let url = std::env::var("DATABASE_URL").context("DATABASE_URL is required")?;
            let db = pool(&url, 2).await?;
            let applied = migrate::migrate(&db, &migrate::default_directory()).await?;
            println!("Migrations applied ({} new)", applied.len());
            Ok(())
        }
        ["import-restaurants", region, file] => {
            let url = std::env::var("DATABASE_URL").context("DATABASE_URL is required")?;
            let db = pool(&url, 2).await?;
            importer::import(&db, region, std::path::Path::new(file)).await
        }
        ["healthcheck", url] => {
            let response = reqwest::Client::builder()
                .timeout(Duration::from_secs(4))
                .build()?
                .get(*url)
                .send()
                .await?;
            if !response.status().is_success() {
                bail!("{url} answered {}", response.status());
            }
            Ok(())
        }
        _ => bail!(
            "usage: safeeat-server [serve [--mode api|web] | migrate | import-restaurants <region> <file> | healthcheck <url>]"
        ),
    }
}
