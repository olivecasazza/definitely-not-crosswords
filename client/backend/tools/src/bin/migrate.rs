//! `migrate` — apply the sqlx migrations (ported from prisma/migrations) to the
//! database named by DATABASE_URL. Idempotent: already-applied migrations are
//! skipped by sqlx's migration tracking (`_sqlx_migrations`).

use anyhow::Context;
use sqlx::postgres::PgPoolOptions;

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

    migrator
        .run(&pool)
        .await
        .context("failed to run migrations")?;

    pool.close().await;

    println!("Migrations OK: {total} migrations applied or already up to date.");
    Ok(())
}
