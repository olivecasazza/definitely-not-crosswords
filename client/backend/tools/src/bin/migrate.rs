//! `migrate` — apply the sqlx migrations (ported from prisma/migrations) to the
//! database named by DATABASE_URL.
//!
//! Safe to run on every deploy:
//! - Fresh DB → applies all migrations from scratch.
//! - Thereafter → applies only the new (not-yet-applied) migrations.
//!
//! There used to be a third mode here: a one-time Prisma→sqlx "adoption" that,
//! on a database with an app schema but no `_sqlx_migrations`, baselined every
//! migration the binary knew about as already-applied. It has been removed — see
//! `migrations/20260812000000_repair_adopted_schema_drift.sql` for why. It
//! marked migrations applied without running or verifying them, so when the
//! sqlx migrations directory had drifted ahead of what Prisma had actually
//! applied, the extra migrations were silently swallowed and could never be
//! replayed. That cost production `User."vipPass"` and the whole `Discount`
//! table. Both live databases were adopted long ago and now carry a real
//! `_sqlx_migrations` history, so the path is dead weight that can only cause
//! that class of corruption again. A resurrected pre-sqlx database should fail
//! loudly on `CREATE TABLE ... already exists` rather than be baselined blind.

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

    println!("Migrations OK: {total} migrations tracked; new ones applied, existing ones skipped.");
    Ok(())
}
