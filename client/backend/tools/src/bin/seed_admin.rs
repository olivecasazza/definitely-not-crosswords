//! Admin-user seeder — port of scripts/seed_admin_users.mjs. Reads
//! ADMIN_USERS_JSON (an array, or `{ "users": [...] }`) of
//! `{ email, name?, role?, emailVerified? }` and upserts each into "User":
//! create if absent, else update role/name/emailVerified when they differ.
//! Run with DATABASE_URL set.

use anyhow::{anyhow, Context, Result};
use serde_json::Value;
use sqlx::postgres::PgPoolOptions;
use sqlx::Row;

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

    let db_url = std::env::var("DATABASE_URL").context("DATABASE_URL must be set")?;
    let pool = PgPoolOptions::new()
        .max_connections(4)
        .connect(&db_url)
        .await?;

    for user in &users {
        let existing = sqlx::query(
            r#"SELECT id, role::text AS role, name, "emailVerified" IS NOT NULL AS verified
               FROM "User" WHERE email = $1"#,
        )
        .bind(&user.email)
        .fetch_optional(&pool)
        .await?;

        match existing {
            None => {
                sqlx::query(
                    r#"INSERT INTO "User" (id, email, name, role, "emailVerified")
                       VALUES (gen_random_uuid()::text, $1, $2, $3::"UserRole",
                               CASE WHEN $4 THEN now() ELSE NULL END)"#,
                )
                .bind(&user.email)
                .bind(&user.name)
                .bind(&user.role)
                .bind(user.email_verified)
                .execute(&pool)
                .await?;
                println!(
                    "Created seeded user {} with role {}.",
                    user.email, user.role
                );
            }
            Some(row) => {
                let cur_role: String = row.get("role");
                let cur_name: Option<String> = row.get("name");
                let cur_verified: bool = row.get("verified");

                let set_role = cur_role != user.role;
                let set_name = user.name.is_some() && user.name != cur_name;
                let set_verified = user.email_verified && !cur_verified;

                if set_role || set_name || set_verified {
                    sqlx::query(
                        r#"UPDATE "User" SET
                             role = CASE WHEN $2 THEN $3::"UserRole" ELSE role END,
                             name = CASE WHEN $4 THEN $5 ELSE name END,
                             "emailVerified" = CASE WHEN $6 THEN now() ELSE "emailVerified" END
                           WHERE email = $1"#,
                    )
                    .bind(&user.email)
                    .bind(set_role)
                    .bind(&user.role)
                    .bind(set_name)
                    .bind(&user.name)
                    .bind(set_verified)
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
