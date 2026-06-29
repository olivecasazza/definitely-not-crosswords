//! `user` router — port of server/trpc/router/user.ts
//!
//! Password format: `scrypt:<saltHex>:<hashHex>` using Node.js default params:
//! N=16384 (log_n=14), r=8, p=1, keylen=64, 16-byte random salt.
//! This is byte-for-byte compatible with lib/auth/password.ts.

use crate::ctx::Ctx;
use crossword_db::Capability;
use rand::RngCore;
use serde_json::{json, Value};
use sqlx::Row;

/// ISO 8601 format for `to_char(col, ...)` — matches JS `Date.toISOString()`.
/// Prisma stores DateTime as TIMESTAMP(3) (UTC, 3 fractional digits).
const TS_FMT: &str = r#"YYYY-MM-DD"T"HH24:MI:SS.MS"Z""#;

pub async fn try_handle(proc: &str, input: &Value, ctx: &Ctx) -> Option<Result<Value, String>> {
    match proc {
        "user.signup" => Some(signup(input, ctx).await),
        "user.isUsernameUnique" => Some(is_username_unique(input, ctx).await),
        "user.isEmailUnique" => Some(is_email_unique(input, ctx).await),
        "user.verifyEmail" => Some(verify_email(input, ctx).await),
        "user.getProfile" => Some(get_profile(input, ctx).await),
        "user.updateProfile" => Some(update_profile(input, ctx).await),
        "user.deleteAccount" => Some(delete_account(input, ctx).await),
        "user.listForAdmin" => Some(list_for_admin(input, ctx).await),
        "user.roleOptions" => Some(role_options(input, ctx).await),
        "user.upsertFromAdmin" => Some(upsert_from_admin(input, ctx).await),
        "user.setRole" => Some(set_role(input, ctx).await),
        "user.setVipPass" => Some(set_vip_pass(input, ctx).await),
        "user.setPassword" => Some(set_password(input, ctx).await),
        _ => None,
    }
}

// ── Password hashing ────────────────────────────────────────────────────────

/// Hash a password on a blocking thread pool (scrypt is CPU-intensive).
/// Output format: `scrypt:<saltHex>:<hashHex>` — identical to lib/auth/password.ts.
async fn hash_password(plain: String) -> Result<String, String> {
    tokio::task::spawn_blocking(move || hash_password_sync(&plain))
        .await
        .map_err(|e| e.to_string())?
}

fn hash_password_sync(plain: &str) -> Result<String, String> {
    let mut salt = [0u8; 16];
    rand::thread_rng().fill_bytes(&mut salt);
    let params = scrypt::Params::new(14, 8, 1, 64).map_err(|e| format!("{e}"))?;
    let mut hash = [0u8; 64];
    scrypt::scrypt(plain.as_bytes(), &salt, &params, &mut hash).map_err(|e| format!("{e}"))?;
    Ok(format!(
        "scrypt:{}:{}",
        hex::encode(salt),
        hex::encode(hash)
    ))
}

// ── Public procedures ────────────────────────────────────────────────────────

async fn signup(input: &Value, ctx: &Ctx) -> Result<Value, String> {
    let email = input["email"]
        .as_str()
        .ok_or("missing email")?
        .trim()
        .to_lowercase();
    let name = input["name"].as_str().ok_or("missing name")?;
    let username = input["username"].as_str().ok_or("missing username")?;
    let password = input["password"]
        .as_str()
        .ok_or("missing password")?
        .to_string();

    let email_exists: bool =
        sqlx::query_scalar(r#"SELECT EXISTS(SELECT 1 FROM "User" WHERE email = $1)"#)
            .bind(&email)
            .fetch_one(&ctx.pool)
            .await
            .map_err(|e| e.to_string())?;
    if email_exists {
        return Err("User with this email already exists.".to_string());
    }

    let username_exists: bool =
        sqlx::query_scalar(r#"SELECT EXISTS(SELECT 1 FROM "User" WHERE username = $1)"#)
            .bind(username)
            .fetch_one(&ctx.pool)
            .await
            .map_err(|e| e.to_string())?;
    if username_exists {
        return Err("User with this username already exists.".to_string());
    }

    let hashed = hash_password(password).await?;
    let id = uuid::Uuid::new_v4().to_string();

    sqlx::query(
        r#"INSERT INTO "User" (id, email, name, username, password, "emailVerified", role, "vipPass")
           VALUES ($1, $2, $3, $4, $5, NULL, 'USER', false)"#,
    )
    .bind(&id)
    .bind(&email)
    .bind(name)
    .bind(username)
    .bind(&hashed)
    .execute(&ctx.pool)
    .await
    .map_err(|e| e.to_string())?;

    // token_str format matches TS: "token_" + random alphanumeric
    let token_str = format!(
        "token_{}",
        uuid::Uuid::new_v4().to_string().replace('-', "")
    );

    sqlx::query(
        r#"INSERT INTO "VerificationToken" (identifier, token, expires)
           VALUES ($1, $2, now() + interval '24 hours')"#,
    )
    .bind(&email)
    .bind(&token_str)
    .execute(&ctx.pool)
    .await
    .map_err(|e| e.to_string())?;

    Ok(json!({
        "success": true,
        "userId": id,
        "verificationToken": token_str,
    }))
}

async fn is_username_unique(input: &Value, ctx: &Ctx) -> Result<Value, String> {
    let username = match input["username"].as_str() {
        Some(u) if u.trim().len() >= 3 => u,
        _ => return Ok(json!({ "unique": true })),
    };

    let exists: bool =
        sqlx::query_scalar(r#"SELECT EXISTS(SELECT 1 FROM "User" WHERE username = $1)"#)
            .bind(username)
            .fetch_one(&ctx.pool)
            .await
            .map_err(|e| e.to_string())?;

    Ok(json!({ "unique": !exists }))
}

async fn is_email_unique(input: &Value, ctx: &Ctx) -> Result<Value, String> {
    let email = match input["email"].as_str() {
        Some(e) if e.contains('@') => e,
        _ => return Ok(json!({ "unique": true })),
    };

    let exists: bool =
        sqlx::query_scalar(r#"SELECT EXISTS(SELECT 1 FROM "User" WHERE email = $1)"#)
            .bind(email)
            .fetch_one(&ctx.pool)
            .await
            .map_err(|e| e.to_string())?;

    Ok(json!({ "unique": !exists }))
}

async fn verify_email(input: &Value, ctx: &Ctx) -> Result<Value, String> {
    let token = input["token"].as_str().ok_or("missing token")?;

    let row = sqlx::query(
        r#"SELECT identifier, expires < now() AS is_expired
           FROM "VerificationToken" WHERE token = $1"#,
    )
    .bind(token)
    .fetch_optional(&ctx.pool)
    .await
    .map_err(|e| e.to_string())?;

    let row = match row {
        Some(r) => r,
        None => return Err("Invalid or expired verification token.".to_string()),
    };

    let identifier: String = row.get("identifier");
    let is_expired: bool = row.get("is_expired");

    if is_expired {
        sqlx::query(r#"DELETE FROM "VerificationToken" WHERE token = $1"#)
            .bind(token)
            .execute(&ctx.pool)
            .await
            .map_err(|e| e.to_string())?;
        return Err("Verification token has expired.".to_string());
    }

    sqlx::query(r#"UPDATE "User" SET "emailVerified" = now() WHERE email = $1"#)
        .bind(&identifier)
        .execute(&ctx.pool)
        .await
        .map_err(|e| e.to_string())?;

    sqlx::query(r#"DELETE FROM "VerificationToken" WHERE token = $1"#)
        .bind(token)
        .execute(&ctx.pool)
        .await
        .map_err(|e| e.to_string())?;

    Ok(json!({ "success": true }))
}

async fn get_profile(input: &Value, ctx: &Ctx) -> Result<Value, String> {
    let email = input["email"].as_str().ok_or("missing email")?;

    let row = sqlx::query(&format!(
        r#"SELECT id, name, email, to_char("emailVerified", '{}') AS "emailVerified"
           FROM "User" WHERE email = $1"#,
        TS_FMT
    ))
    .bind(email)
    .fetch_optional(&ctx.pool)
    .await
    .map_err(|e| e.to_string())?;

    let row = match row {
        Some(r) => r,
        None => return Err("User not found.".to_string()),
    };

    Ok(json!({
        "id": row.get::<String, _>("id"),
        "name": row.get::<Option<String>, _>("name"),
        "email": row.get::<Option<String>, _>("email"),
        "emailVerified": row.get::<Option<String>, _>("emailVerified"),
    }))
}

async fn update_profile(input: &Value, ctx: &Ctx) -> Result<Value, String> {
    let email = input["email"].as_str().ok_or("missing email")?;
    let name = input["name"].as_str().ok_or("missing name")?;

    let row = sqlx::query(r#"UPDATE "User" SET name = $1 WHERE email = $2 RETURNING name"#)
        .bind(name)
        .bind(email)
        .fetch_optional(&ctx.pool)
        .await
        .map_err(|e| e.to_string())?;

    match row {
        Some(r) => Ok(json!({
            "success": true,
            "name": r.get::<Option<String>, _>("name"),
        })),
        None => Err("User not found.".to_string()),
    }
}

async fn delete_account(input: &Value, ctx: &Ctx) -> Result<Value, String> {
    let email = input["email"].as_str().ok_or("missing email")?;

    sqlx::query(r#"DELETE FROM "User" WHERE email = $1"#)
        .bind(email)
        .execute(&ctx.pool)
        .await
        .map_err(|e| e.to_string())?;

    Ok(json!({ "success": true }))
}

// ── Admin procedures ─────────────────────────────────────────────────────────

async fn list_for_admin(_input: &Value, ctx: &Ctx) -> Result<Value, String> {
    if let Err(e) = ctx.auth.require_capability(Capability::AdminAccess) {
        return Err(format!("{e:?}"));
    }

    let rows = sqlx::query(&format!(
        r#"SELECT id, email, username, name, role::text AS role, "vipPass",
                  to_char("emailVerified", '{}') AS "emailVerified"
           FROM "User"
           ORDER BY role ASC, email ASC"#,
        TS_FMT
    ))
    .fetch_all(&ctx.pool)
    .await
    .map_err(|e| e.to_string())?;

    let users: Vec<Value> = rows
        .iter()
        .map(|r| {
            json!({
                "id": r.get::<String, _>("id"),
                "email": r.get::<Option<String>, _>("email"),
                "username": r.get::<Option<String>, _>("username"),
                "name": r.get::<Option<String>, _>("name"),
                "role": r.get::<String, _>("role"),
                "vipPass": r.get::<bool, _>("vipPass"),
                "emailVerified": r.get::<Option<String>, _>("emailVerified"),
            })
        })
        .collect();

    Ok(json!(users))
}

async fn role_options(_input: &Value, ctx: &Ctx) -> Result<Value, String> {
    if let Err(e) = ctx.auth.require_capability(Capability::AdminAccess) {
        return Err(format!("{e:?}"));
    }

    // Mirrors lib/auth/roles.ts: appRoles order (USER first, ADMIN second)
    // and roleCapabilities string values.
    Ok(json!([
        { "role": "USER", "capabilities": ["game:play", "profile:manage"] },
        { "role": "ADMIN", "capabilities": ["game:play", "profile:manage", "admin:access", "generator:manage"] }
    ]))
}

async fn upsert_from_admin(input: &Value, ctx: &Ctx) -> Result<Value, String> {
    if let Err(e) = ctx.auth.require_capability(Capability::AdminAccess) {
        return Err(format!("{e:?}"));
    }

    let email = input["email"]
        .as_str()
        .ok_or("missing email")?
        .trim()
        .to_lowercase();
    let name = input["name"].as_str();
    let role = input["role"].as_str().ok_or("missing role")?;
    let password_opt = input["password"].as_str().map(|s| s.to_string());

    let hashed_opt = if let Some(pw) = password_opt {
        Some(hash_password(pw).await?)
    } else {
        None
    };

    let exists: bool =
        sqlx::query_scalar(r#"SELECT EXISTS(SELECT 1 FROM "User" WHERE email = $1)"#)
            .bind(&email)
            .fetch_one(&ctx.pool)
            .await
            .map_err(|e| e.to_string())?;

    if exists {
        // Update: role always set; name and password only if provided;
        // emailVerified set to now() only if currently null (COALESCE).
        sqlx::query(
            r#"UPDATE "User" SET
                 role = $1,
                 name = CASE WHEN $2::text IS NOT NULL THEN $2 ELSE name END,
                 password = CASE WHEN $3::text IS NOT NULL THEN $3 ELSE password END,
                 "emailVerified" = COALESCE("emailVerified", now())
               WHERE email = $4"#,
        )
        .bind(role)
        .bind(name)
        .bind(hashed_opt.as_deref())
        .bind(&email)
        .execute(&ctx.pool)
        .await
        .map_err(|e| e.to_string())?;
    } else {
        let id = uuid::Uuid::new_v4().to_string();
        sqlx::query(
            r#"INSERT INTO "User" (id, email, name, role, "emailVerified", password, username, "vipPass")
               VALUES ($1, $2, $3, $4, now(), $5, NULL, false)"#,
        )
        .bind(&id)
        .bind(&email)
        .bind(name)
        .bind(role)
        .bind(hashed_opt.as_deref())
        .execute(&ctx.pool)
        .await
        .map_err(|e| e.to_string())?;
    }

    let row = sqlx::query(&format!(
        r#"SELECT id, email, username, name, role::text AS role,
                  to_char("emailVerified", '{}') AS "emailVerified"
           FROM "User" WHERE email = $1"#,
        TS_FMT
    ))
    .bind(&email)
    .fetch_one(&ctx.pool)
    .await
    .map_err(|e| e.to_string())?;

    Ok(json!({
        "success": true,
        "user": {
            "id": row.get::<String, _>("id"),
            "email": row.get::<Option<String>, _>("email"),
            "username": row.get::<Option<String>, _>("username"),
            "name": row.get::<Option<String>, _>("name"),
            "role": row.get::<String, _>("role"),
            "emailVerified": row.get::<Option<String>, _>("emailVerified"),
        }
    }))
}

async fn set_role(input: &Value, ctx: &Ctx) -> Result<Value, String> {
    let current_user = match ctx.auth.require_capability(Capability::AdminAccess) {
        Ok(u) => u,
        Err(e) => return Err(format!("{e:?}")),
    };
    let current_id = current_user.id.clone();

    let user_id = input["userId"].as_str().ok_or("missing userId")?;
    let new_role = input["role"].as_str().ok_or("missing role")?;

    let row = sqlx::query(r#"SELECT id, role::text AS role FROM "User" WHERE id = $1"#)
        .bind(user_id)
        .fetch_optional(&ctx.pool)
        .await
        .map_err(|e| e.to_string())?;

    let row = match row {
        Some(r) => r,
        None => return Err("User not found.".to_string()),
    };

    let target_id: String = row.get("id");
    let target_role: String = row.get("role");

    // Guard: admin cannot change their own role.
    if target_id == current_id && target_role != new_role {
        return Err("Admins cannot change their own role.".to_string());
    }

    // Guard: must keep at least one admin.
    if target_role == "ADMIN" && new_role != "ADMIN" {
        let admin_count: i64 =
            sqlx::query_scalar(r#"SELECT COUNT(*) FROM "User" WHERE role = 'ADMIN'"#)
                .fetch_one(&ctx.pool)
                .await
                .map_err(|e| e.to_string())?;
        if admin_count <= 1 {
            return Err("At least one admin must remain.".to_string());
        }
    }

    let updated = sqlx::query(&format!(
        r#"UPDATE "User" SET role = $1 WHERE id = $2
           RETURNING id, email, username, name, role::text AS role,
                     to_char("emailVerified", '{}') AS "emailVerified""#,
        TS_FMT
    ))
    .bind(new_role)
    .bind(user_id)
    .fetch_one(&ctx.pool)
    .await
    .map_err(|e| e.to_string())?;

    Ok(json!({
        "success": true,
        "user": {
            "id": updated.get::<String, _>("id"),
            "email": updated.get::<Option<String>, _>("email"),
            "username": updated.get::<Option<String>, _>("username"),
            "name": updated.get::<Option<String>, _>("name"),
            "role": updated.get::<String, _>("role"),
            "emailVerified": updated.get::<Option<String>, _>("emailVerified"),
        }
    }))
}

async fn set_vip_pass(input: &Value, ctx: &Ctx) -> Result<Value, String> {
    if let Err(e) = ctx.auth.require_capability(Capability::AdminAccess) {
        return Err(format!("{e:?}"));
    }

    let user_id = input["userId"].as_str().ok_or("missing userId")?;
    let vip_pass = input["vipPass"].as_bool().ok_or("missing vipPass")?;

    let exists: bool = sqlx::query_scalar(r#"SELECT EXISTS(SELECT 1 FROM "User" WHERE id = $1)"#)
        .bind(user_id)
        .fetch_one(&ctx.pool)
        .await
        .map_err(|e| e.to_string())?;
    if !exists {
        return Err("User not found.".to_string());
    }

    let updated = sqlx::query(&format!(
        r#"UPDATE "User" SET "vipPass" = $1 WHERE id = $2
           RETURNING id, email, username, name, role::text AS role, "vipPass",
                     to_char("emailVerified", '{}') AS "emailVerified""#,
        TS_FMT
    ))
    .bind(vip_pass)
    .bind(user_id)
    .fetch_one(&ctx.pool)
    .await
    .map_err(|e| e.to_string())?;

    Ok(json!({
        "success": true,
        "user": {
            "id": updated.get::<String, _>("id"),
            "email": updated.get::<Option<String>, _>("email"),
            "username": updated.get::<Option<String>, _>("username"),
            "name": updated.get::<Option<String>, _>("name"),
            "role": updated.get::<String, _>("role"),
            "vipPass": updated.get::<bool, _>("vipPass"),
            "emailVerified": updated.get::<Option<String>, _>("emailVerified"),
        }
    }))
}

async fn set_password(input: &Value, ctx: &Ctx) -> Result<Value, String> {
    if let Err(e) = ctx.auth.require_capability(Capability::AdminAccess) {
        return Err(format!("{e:?}"));
    }

    let user_id = input["userId"].as_str().ok_or("missing userId")?;
    let password = input["password"]
        .as_str()
        .ok_or("missing password")?
        .to_string();

    let exists: bool = sqlx::query_scalar(r#"SELECT EXISTS(SELECT 1 FROM "User" WHERE id = $1)"#)
        .bind(user_id)
        .fetch_one(&ctx.pool)
        .await
        .map_err(|e| e.to_string())?;
    if !exists {
        return Err("User not found.".to_string());
    }

    let hashed = hash_password(password).await?;

    sqlx::query(r#"UPDATE "User" SET password = $1 WHERE id = $2"#)
        .bind(&hashed)
        .bind(user_id)
        .execute(&ctx.pool)
        .await
        .map_err(|e| e.to_string())?;

    Ok(json!({ "success": true }))
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    /// Verifies that our scrypt parameters and output format exactly match
    /// lib/auth/password.ts (Node.js crypto.scrypt defaults: N=16384, r=8, p=1,
    /// keylen=64).  Reference vector generated with:
    ///   node -e "const c=require('crypto'); const s=Buffer.alloc(16,0);
    ///            console.log(c.scryptSync('password123', s, 64).toString('hex'))"
    #[test]
    fn scrypt_matches_node_js() {
        const EXPECTED: &str = "e778ee713285d4e273c34420ef63702cc6a9f91db8da95eca3f0431e09dd50411e11a55d5a1d5c90f0fbdb5598fd3276076ed743913c01e4884cff8d816bf7a7";
        let salt = [0u8; 16];
        let params = scrypt::Params::new(14, 8, 1, 64).expect("valid params");
        let mut hash = [0u8; 64];
        scrypt::scrypt(b"password123", &salt, &params, &mut hash).expect("scrypt ok");
        assert_eq!(hex::encode(hash), EXPECTED);
    }
}
