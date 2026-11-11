//! SQL migration runner (same `schema_migrations` table and advisory lock as apps/api/src/db/migrate.ts).

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use sqlx::PgPool;

pub fn default_directory() -> PathBuf {
    std::env::var_os("MIGRATIONS_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("db/migrations"))
}

/// Applies every pending `*.sql` file in name order, all inside one transaction.
pub async fn migrate(db: &PgPool, directory: &Path) -> Result<Vec<String>> {
    let mut names: Vec<String> = std::fs::read_dir(directory)
        .with_context(|| format!("reading {}", directory.display()))?
        .filter_map(|entry| entry.ok()?.file_name().into_string().ok())
        .filter(|name| name.ends_with(".sql"))
        .collect();
    names.sort();
    let mut tx = db.begin().await?;
    // The pool's 10 s statement timeout is for requests; rebuilding indexes or chains over the
    // full catalog can take longer.
    sqlx::raw_sql("SET LOCAL statement_timeout=0")
        .execute(&mut *tx)
        .await?;
    sqlx::raw_sql("SELECT pg_advisory_xact_lock(87349102)")
        .execute(&mut *tx)
        .await?;
    sqlx::raw_sql("CREATE TABLE IF NOT EXISTS schema_migrations (name text PRIMARY KEY, applied_at timestamptz NOT NULL DEFAULT now())").execute(&mut *tx)
        .await?;
    let mut applied = Vec::new();
    for name in names {
        let exists = sqlx::query_scalar::<_, i32>("SELECT 1 FROM schema_migrations WHERE name=$1")
            .bind(&name)
            .fetch_optional(&mut *tx)
            .await?
            .is_some();
        if exists {
            continue;
        }
        let sql = std::fs::read_to_string(directory.join(&name))?;
        // Simple-query protocol: a file may contain many statements and dollar-quoted bodies.
        sqlx::raw_sql(&sql)
            .execute(&mut *tx)
            .await
            .with_context(|| format!("applying {name}"))?;
        sqlx::query("INSERT INTO schema_migrations(name) VALUES($1)")
            .bind(&name)
            .execute(&mut *tx)
            .await?;
        applied.push(name);
    }
    tx.commit().await?;
    Ok(applied)
}
