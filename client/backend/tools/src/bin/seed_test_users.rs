//! Test-user seeder — reconciles the accounts the Playwright e2e demo/canary
//! logs in with. Reads E2E_USERS_JSON (an array of users, or an object
//! `{ "users": [...], "team": { "name": "..." } }`) of
//! `{ email, name, password }` and upserts each into "User":
//!   • absent            → create with a scrypt password hash, verified
//!   • present           → refresh name, ensure verified, and re-hash the
//!                         password only when it no longer matches (idempotent)
//! Role is always USER — these are demo accounts, never admins.
//! When `team` is present, the seed also creates that team (PUBLIC, owned by
//! the first user) and ensures every listed user is a member — the stats
//! player list / head-to-head are scoped to teammates, so the demo's compare
//! panel only lights up if the e2e users share one.
//! Run with DATABASE_URL set. No-op when E2E_USERS_JSON is unset/empty, so
//! environments without test users skip cleanly.

use anyhow::{anyhow, Context, Result};
use rand::Rng;
use serde_json::Value;
use sqlx::postgres::PgPoolOptions;
use sqlx::Row;

struct TestUser {
    email: String,
    name: String,
    password: String,
}

struct SeedConfig {
    users: Vec<TestUser>,
    team_name: Option<String>,
}

/// Free-tier team size (matches team.rs FREE_MAX_SIZE) — e2e users aren't Pro.
const TEAM_MAX_SIZE: i32 = 4;

/// Matches the server's `verify_password` format (`scrypt:<saltHex>:<hashHex>`,
/// n=14 r=8 p=1) so seeded credentials authenticate via /api/auth/callback/credentials.
fn hash_password(plain: &str) -> Result<String> {
    let mut salt = [0u8; 16];
    rand::thread_rng().fill(&mut salt);
    let params = scrypt::Params::new(14, 8, 1, 32).map_err(|e| anyhow!(e.to_string()))?;
    let mut out = [0u8; 32];
    scrypt::scrypt(plain.as_bytes(), &salt, &params, &mut out)
        .map_err(|e| anyhow!(e.to_string()))?;
    Ok(format!("scrypt:{}:{}", hex::encode(salt), hex::encode(out)))
}

/// Server-side verification, duplicated so updates are drift-only.
fn verify_password(plain: &str, stored: Option<&str>) -> bool {
    let Some(s) = stored else { return false };
    let parts: Vec<&str> = s.splitn(3, ':').collect();
    if parts.len() != 3 || parts[0] != "scrypt" {
        return false;
    }
    let (Ok(salt), Ok(expected)) = (hex::decode(parts[1]), hex::decode(parts[2])) else {
        return false;
    };
    let params = match scrypt::Params::new(14, 8, 1, expected.len()) {
        Ok(p) => p,
        Err(_) => return false,
    };
    let mut out = vec![0u8; expected.len()];
    if scrypt::scrypt(plain.as_bytes(), &salt, &params, &mut out).is_err() {
        return false;
    }
    out == expected
}

fn parse_config(raw: &str) -> Result<SeedConfig> {
    let parsed: Value =
        serde_json::from_str(raw).context("E2E_USERS_JSON must be valid JSON")?;
    let (arr, team_name) = match &parsed {
        Value::Array(a) => (a.clone(), None),
        Value::Object(o) => {
            let users = o
                .get("users")
                .and_then(|v| v.as_array())
                .cloned()
                .unwrap_or_default();
            let team = o
                .get("team")
                .and_then(|t| t.get("name"))
                .and_then(|n| n.as_str())
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty());
            (users, team)
        }
        _ => {
            return Err(anyhow!(
                "E2E_USERS_JSON must be an array or {{ \"users\": [...] }}."
            ))
        }
    };

    let mut out = Vec::new();
    for (i, u) in arr.iter().enumerate() {
        let email = u
            .get("email")
            .and_then(|v| v.as_str())
            .map(|s| s.trim().to_lowercase())
            .filter(|s| !s.is_empty())
            .ok_or_else(|| anyhow!("e2eUsers[{i}].email is required."))?;
        let name = u
            .get("name")
            .and_then(|v| v.as_str())
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .ok_or_else(|| anyhow!("e2eUsers[{i}].name is required (believable names, please)."))?;
        let password = u
            .get("password")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .filter(|s| s.len() >= 12)
            .ok_or_else(|| anyhow!("e2eUsers[{i}].password is required (12+ chars)."))?;
        out.push(TestUser {
            email,
            name,
            password,
        });
    }
    Ok(SeedConfig {
        users: out,
        team_name,
    })
}

async fn upsert_user(pool: &sqlx::PgPool, user: &TestUser) -> Result<String> {
    let existing = sqlx::query(
        r#"SELECT id, name, password, "emailVerified" IS NOT NULL AS verified
           FROM "User" WHERE email = $1"#,
    )
    .bind(&user.email)
    .fetch_optional(pool)
    .await?;

    match existing {
        None => {
            let hash = hash_password(&user.password)?;
            let row = sqlx::query(
                r#"INSERT INTO "User" (id, email, name, role, password, "emailVerified")
                   VALUES (gen_random_uuid()::text, $1, $2, 'USER'::"UserRole", $3, now())
                   RETURNING id"#,
            )
            .bind(&user.email)
            .bind(&user.name)
            .bind(&hash)
            .fetch_one(pool)
            .await?;
            println!("Created e2e user {} ({}).", user.name, user.email);
            Ok(row.get("id"))
        }
        Some(row) => {
            let id: String = row.get("id");
            let cur_name: Option<String> = row.get("name");
            let cur_password: Option<String> = row.get("password");
            let cur_verified: bool = row.get("verified");

            let set_name = cur_name.as_deref() != Some(user.name.as_str());
            let set_password = !verify_password(&user.password, cur_password.as_deref());
            let set_verified = !cur_verified;

            if set_name || set_password || set_verified {
                let new_hash = if set_password {
                    Some(hash_password(&user.password)?)
                } else {
                    None
                };
                sqlx::query(
                    r#"UPDATE "User" SET
                         name = CASE WHEN $2 THEN $3 ELSE name END,
                         password = CASE WHEN $4 THEN COALESCE($5, password) ELSE password END,
                         "emailVerified" = CASE WHEN $6 THEN now() ELSE "emailVerified" END
                       WHERE email = $1"#,
                )
                .bind(&user.email)
                .bind(set_name)
                .bind(&user.name)
                .bind(set_password)
                .bind(&new_hash)
                .bind(set_verified)
                .execute(pool)
                .await?;
                println!("Reconciled e2e user {} ({}).", user.name, user.email);
            } else {
                println!("E2E user {} ({}) already up to date.", user.name, user.email);
            }
            Ok(id)
        }
    }
}

/// Create the shared team if absent and ensure every e2e user is a member.
/// Owner is the first listed user. Name conflicts follow team.create's rule
/// (team names are globally unique), so an existing team is adopted as-is.
async fn ensure_team(pool: &sqlx::PgPool, name: &str, user_ids: &[String]) -> Result<()> {
    let team_id: String = match sqlx::query(r#"SELECT id FROM "Team" WHERE name = $1"#)
        .bind(name)
        .fetch_optional(pool)
        .await?
    {
        Some(row) => row.get("id"),
        None => {
            let owner = user_ids.first().context("team needs at least one user")?;
            let row = sqlx::query(
                r#"INSERT INTO "Team" (id, name, "ownerId", visibility, "maxSize", "createdAt")
                   VALUES (gen_random_uuid()::text, $1, $2, 'PUBLIC'::"TeamVisibility", $3, now())
                   RETURNING id"#,
            )
            .bind(name)
            .bind(owner)
            .bind(TEAM_MAX_SIZE)
            .fetch_one(pool)
            .await?;
            let id: String = row.get("id");
            println!("Created e2e team {name}.");
            id
        }
    };

    for uid in user_ids {
        let inserted = sqlx::query(
            r#"INSERT INTO "TeamMember" (id, "teamId", "userId", "joinedAt")
               SELECT gen_random_uuid()::text, $1, $2, now()
               WHERE NOT EXISTS (
                   SELECT 1 FROM "TeamMember" WHERE "teamId" = $1 AND "userId" = $2
               )"#,
        )
        .bind(&team_id)
        .bind(uid)
        .execute(pool)
        .await?;
        if inserted.rows_affected() > 0 {
            println!("Added {uid} to e2e team {name}.");
        }
    }
    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    let raw = std::env::var("E2E_USERS_JSON").unwrap_or_default();
    if raw.trim().is_empty() {
        println!("No e2e users configured; skipping test user seed.");
        return Ok(());
    }
    let config = parse_config(&raw)?;
    if config.users.is_empty() {
        println!("No e2e users configured; skipping test user seed.");
        return Ok(());
    }

    let db_url = std::env::var("DATABASE_URL").context("DATABASE_URL must be set")?;
    let pool = PgPoolOptions::new()
        .max_connections(4)
        .connect(&db_url)
        .await?;

    let mut user_ids = Vec::with_capacity(config.users.len());
    for user in &config.users {
        user_ids.push(upsert_user(&pool, user).await?);
    }

    if let Some(name) = &config.team_name {
        ensure_team(&pool, name, &user_ids).await?;
    }
    Ok(())
}
