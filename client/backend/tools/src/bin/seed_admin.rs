//! Admin-user seeder — port of scripts/seed_admin_users.mjs. Reads
//! ADMIN_USERS_JSON (an array, or `{ "users": [...] }`) of
//! `{ email, name?, role?, emailVerified? }` and upserts each into "User":
//! create if absent, else update role/name/emailVerified when they differ.
//! Run with DATABASE_URL set.
//!
//! Optionally reads ADMIN_PASSWORDS_JSON — `{ "<email>": "<password>" }` — and
//! gives seeded admins an initial password. Without it these rows land with
//! `password IS NULL`, which `verify_password` rejects before hashing, so EVERY
//! sign-in fails. That was unrecoverable in-product: `user.setPassword` and
//! `user.upsertFromAdmin` both require an existing admin session, signup rejects
//! the address as taken, and there is no reset flow — the seeded admin could
//! never log in at all.
//!
//! Passwords come from their own env var, not ADMIN_USERS_JSON, because
//! ADMIN_USERS_JSON is rendered from plaintext Helm values while this is wired
//! from a Secret (same split as E2E_USERS_JSON).

use anyhow::{anyhow, Context, Result};
use rand::RngCore;
use serde_json::Value;
use sqlx::postgres::PgPoolOptions;
use sqlx::Row;
use std::collections::HashMap;

/// `scrypt:<saltHex>:<hashHex>`, n=14 r=8 p=1, keylen 64 — matches
/// `routers/user.rs::hash_password_sync` so the server verifies it.
fn hash_password(plain: &str) -> Result<String> {
    let mut salt = [0u8; 16];
    rand::thread_rng().fill_bytes(&mut salt);
    let params = scrypt::Params::new(14, 8, 1, 64).map_err(|e| anyhow!(e.to_string()))?;
    let mut out = [0u8; 64];
    scrypt::scrypt(plain.as_bytes(), &salt, &params, &mut out)
        .map_err(|e| anyhow!(e.to_string()))?;
    Ok(format!("scrypt:{}:{}", hex::encode(salt), hex::encode(out)))
}

/// email -> plaintext password, emails normalized like every other lookup.
fn parse_passwords(raw: &str) -> Result<HashMap<String, String>> {
    if raw.trim().is_empty() {
        return Ok(HashMap::new());
    }
    let parsed: Value =
        serde_json::from_str(raw).context("ADMIN_PASSWORDS_JSON must be valid JSON")?;
    let obj = parsed
        .as_object()
        .ok_or_else(|| anyhow!("ADMIN_PASSWORDS_JSON must be an object of email -> password"))?;
    let mut out = HashMap::new();
    for (email, v) in obj {
        let Some(pw) = v.as_str().filter(|s| !s.is_empty()) else {
            continue;
        };
        if pw.len() < 8 {
            return Err(anyhow!(
                "ADMIN_PASSWORDS_JSON password for {email} is shorter than 8 characters."
            ));
        }
        out.insert(email.trim().to_lowercase(), pw.to_string());
    }
    Ok(out)
}

struct SeedUser {
    email: String,
    name: Option<String>,
    role: String,
    email_verified: bool,
}

fn parse_users(raw: &str) -> Result<Vec<SeedUser>> {
    let parsed: Value = serde_json::from_str(raw).context("ADMIN_USERS_JSON must be valid JSON")?;
    let arr = match &parsed {
        Value::Array(a) => a.clone(),
        Value::Object(o) => o
            .get("users")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default(),
        _ => {
            return Err(anyhow!(
                "ADMIN_USERS_JSON must be an object with a users array."
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
            .ok_or_else(|| anyhow!("adminUsers.users[{i}].email is required."))?;
        let role = u
            .get("role")
            .and_then(|v| v.as_str())
            .map(|s| s.trim().to_uppercase())
            .unwrap_or_else(|| "ADMIN".to_string());
        if role != "USER" && role != "ADMIN" {
            return Err(anyhow!(
                "adminUsers.users[{i}].role must be one of: USER, ADMIN."
            ));
        }
        let name = u
            .get("name")
            .and_then(|v| v.as_str())
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());
        // emailVerified defaults to true (only an explicit `false` disables it).
        let email_verified = u.get("emailVerified") != Some(&Value::Bool(false));
        out.push(SeedUser {
            email,
            name,
            role,
            email_verified,
        });
    }
    Ok(out)
}

#[tokio::main]
async fn main() -> Result<()> {
    let raw = std::env::var("ADMIN_USERS_JSON").unwrap_or_default();
    if raw.trim().is_empty() {
        println!("No admin users configured; skipping admin user seed.");
        return Ok(());
    }
    let users = parse_users(&raw)?;
    if users.is_empty() {
        println!("No admin users configured; skipping admin user seed.");
        return Ok(());
    }

    let passwords = parse_passwords(&std::env::var("ADMIN_PASSWORDS_JSON").unwrap_or_default())?;

    let db_url = std::env::var("DATABASE_URL").context("DATABASE_URL must be set")?;
    let pool = PgPoolOptions::new()
        .max_connections(4)
        .connect(&db_url)
        .await?;

    for user in &users {
        let hashed = match passwords.get(&user.email) {
            Some(pw) => Some(hash_password(pw)?),
            None => None,
        };

        let existing = sqlx::query(
            r#"SELECT id, role::text AS role, name, "emailVerified" IS NOT NULL AS verified,
                      password IS NULL AS no_password
               FROM "User" WHERE email = $1"#,
        )
        .bind(&user.email)
        .fetch_optional(&pool)
        .await?;

        match existing {
            None => {
                sqlx::query(
                    r#"INSERT INTO "User" (id, email, name, role, "emailVerified", password)
                       VALUES (gen_random_uuid()::text, $1, $2, $3::"UserRole",
                               CASE WHEN $4 THEN now() ELSE NULL END, $5)"#,
                )
                .bind(&user.email)
                .bind(&user.name)
                .bind(&user.role)
                .bind(user.email_verified)
                .bind(&hashed)
                .execute(&pool)
                .await?;
                println!(
                    "Created seeded user {} with role {}{}.",
                    user.email,
                    user.role,
                    if hashed.is_some() {
                        " and an initial password"
                    } else {
                        " (no password configured — this account cannot sign in)"
                    }
                );
            }
            Some(row) => {
                let cur_role: String = row.get("role");
                let cur_name: Option<String> = row.get("name");
                let cur_verified: bool = row.get("verified");
                let no_password: bool = row.get("no_password");

                let set_role = cur_role != user.role;
                let set_name = user.name.is_some() && user.name != cur_name;
                let set_verified = user.email_verified && !cur_verified;
                // Only ever FILL a missing password. Never overwrite one — the
                // admin may have changed it via user.setPassword, and this runs
                // on every pod start.
                let set_password = no_password && hashed.is_some();

                if set_role || set_name || set_verified || set_password {
                    sqlx::query(
                        r#"UPDATE "User" SET
                             role = CASE WHEN $2 THEN $3::"UserRole" ELSE role END,
                             name = CASE WHEN $4 THEN $5 ELSE name END,
                             "emailVerified" = CASE WHEN $6 THEN now() ELSE "emailVerified" END,
                             password = CASE WHEN $7 THEN $8 ELSE password END
                           WHERE email = $1"#,
                    )
                    .bind(&user.email)
                    .bind(set_role)
                    .bind(&user.role)
                    .bind(set_name)
                    .bind(&user.name)
                    .bind(set_verified)
                    .bind(set_password)
                    .bind(&hashed)
                    .execute(&pool)
                    .await?;
                    println!("Updated seeded user {}.", user.email);
                } else {
                    println!(
                        "Seeded user {} already exists with requested role.",
                        user.email
                    );
                }
            }
        }
    }
    Ok(())
}
