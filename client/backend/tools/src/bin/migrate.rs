//! `migrate` — apply the sqlx migrations (ported from prisma/migrations) to the
//! database named by DATABASE_URL.
//!
//! Safe to run on every deploy:
//! - Fresh DB → applies all migrations from scratch.
//! - DB that already has the schema but no sqlx history (the one-time Prisma→sqlx
//!   handover) → ADOPTS it: baselines the current migrations as already-applied so
//!   we don't try to re-create existing tables, then applies nothing.
//! - Thereafter → applies only the new (not-yet-applied) migrations.

use anyhow::Context;
use sqlx::{postgres::PgPoolOptions, PgPool};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let db_url = std::env::var("DATABASE_URL").context("DATABASE_URL env var is not set")?;

    let pool = PgPoolOptions::new()
        .max_connections(4)
        .connect(&db_url)
        .await
        .context("failed to connect to the database")?;

    // Path is resolved at compile time relative to CARGO_MANIFEST_DIR (this crate
    // root), so "./migrations" points at backend/tools/migrations.
    let migrator = sqlx::migrate!("./migrations");
    let total = migrator.iter().count();

    adopt_existing_schema_if_needed(&pool, &migrator).await?;

    migrator
        .run(&pool)
        .await
        .context("failed to run migrations")?;

    pool.close().await;

    println!("Migrations OK: {total} migrations tracked; new ones applied, existing ones skipped.");
    Ok(())
}

/// One-time adoption of a database whose schema predates sqlx (it was created by
/// Prisma). If the app schema exists (the `User` table) but sqlx's tracking table
/// does not, mark every current migration as already-applied — with the embedded
/// checksums, so `migrator.run` then treats them as done. No-op once adopted, and
/// skipped entirely on a fresh database (so it migrates from scratch).
async fn adopt_existing_schema_if_needed(
    pool: &PgPool,
    migrator: &sqlx::migrate::Migrator,
) -> anyhow::Result<()> {
    let has_sqlx_history: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM information_schema.tables
         WHERE table_schema = 'public' AND table_name = '_sqlx_migrations')",
    )
    .fetch_one(pool)
    .await?;
    let has_app_schema: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM information_schema.tables
         WHERE table_schema = 'public' AND table_name = 'User')",
    )
    .fetch_one(pool)
    .await?;

    if has_sqlx_history || !has_app_schema {
        return Ok(()); // already adopted, or a fresh DB → normal migrate path
    }

    eprintln!(
        "adopting existing (pre-sqlx) schema: baselining {} migrations as applied",
        migrator.iter().count()
    );

    // sqlx's own tracking-table layout (sqlx 0.8, Postgres).
    sqlx::query(
        r#"CREATE TABLE IF NOT EXISTS _sqlx_migrations (
            version BIGINT PRIMARY KEY,
            description TEXT NOT NULL,
            installed_on TIMESTAMPTZ NOT NULL DEFAULT now(),
            success BOOLEAN NOT NULL,
            checksum BYTEA NOT NULL,
            execution_time BIGINT NOT NULL
        )"#,
    )
    .execute(pool)
    .await?;

    for m in migrator.iter() {
        sqlx::query(
            r#"INSERT INTO _sqlx_migrations
               (version, description, installed_on, success, checksum, execution_time)
               VALUES ($1, $2, now(), true, $3, 0)
               ON CONFLICT (version) DO NOTHING"#,
        )
        .bind(m.version)
        .bind(m.description.as_ref())
        .bind(m.checksum.as_ref())
        .execute(pool)
        .await?;
    }

    Ok(())
}
