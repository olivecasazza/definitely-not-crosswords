//! Database-error redaction (DEF-242 AC6 / DEF-260 / DEF-261).
//!
//! DEF-260 sanitised `routers/active_game.rs` only, so the leak stayed
//! reachable on every other router: 140 `map_err(|e| e.to_string())` sites
//! across 12 files, on `user`, `team`, `stats`, `game_list`, `discount`,
//! `subscription` and the generator routes. `e.to_string()` on a
//! `sqlx::Error` hands the caller the driver and database text verbatim —
//! table names, column names, constraint names and the rejected value:
//!
//! ```text
//! error returned from database: invalid input value for enum "GameActionTypeEnum": "add"
//! ```
//!
//! Three layers of coverage, weakest first:
//!
//! 1. the helper, in isolation;
//! 2. a **route-level** test that induces a real `sqlx` failure through
//!    `routers::dispatch` — the same entry point `main.rs` `trpc_post` uses —
//!    on public procedures in four routers outside `active_game.rs`, and
//!    asserts the message that reaches the client;
//! 3. a **source guard** asserting the raw pattern is gone from every router,
//!    so a regression fails CI even in a route this file doesn't call.

mod common;

use crossword_server::ctx::sanitised_db_error;
use crossword_server::routers;
use serde_json::{json, Value};
use std::fs;
use std::path::{Path, PathBuf};

#[test]
fn sqlx_error_text_is_not_serialised_to_callers() {
    let raw = r#"error returned from database: invalid input value for enum \"GameActionTypeEnum\": \"forged\""#;
    let err = sqlx::Error::Protocol(raw.to_string());

    let out = sanitised_db_error("save the submitted letters", &err);

    assert_eq!(out, "save the submitted letters failed");
    assert!(!out.contains("invalid input value"));
    assert!(!out.contains("GameActionTypeEnum"));
    assert!(!out.contains("forged"));
}

// ── 2. route-level: a real database failure, on routes outside active_game ────

/// Call a public procedure whose first action is a query, against a pool that
/// is never dialled. The query genuinely fails, so the error under test is a
/// real `sqlx::Error` — not a synthesised one — which is what the original
/// finding actually leaked.
async fn call_with_db_down(proc: &str, input: &Value) -> Result<Value, String> {
    let pool = common::undialled_pool();
    // Anonymous: all four procedures below are public, so the caller is
    // refused by the database rather than by an auth gate.
    let ctx = common::ctx_as(&pool, None);
    routers::dispatch(proc, input, &ctx).await
}

/// Nothing a caller can use to fingerprint the schema or the host.
fn assert_carries_no_schema_detail(msg: &str, proc: &str) {
    let lower = msg.to_lowercase();
    for banned in [
        // driver / server identity
        "sqlx",
        "postgres",
        "connection",
        "localhost",
        // table names
        "\"user\"",
        "teammember",
        "teaminvite",
        "\"team\"",
        "\"game\"",
        "dailypick",
        "gameaction",
        "subscription",
        "verificationtoken",
        "generationquota",
        "generationjob",
        "\"question\"",
        "dictionarydefinition",
        "discount",
        "completedgame",
        "activegame",
        // column names
        "vippass",
        "emailverified",
        "ownerid",
        "gameid",
        "createdat",
        "usedthismonth",
    ] {
        assert!(
            !lower.contains(banned),
            "{proc}: leaked {banned:?} to the caller: {msg:?}"
        );
    }
}

/// AC3: a failure induced on a covered route returns a stable,
/// non-database-specific message.
#[tokio::test]
async fn game_get_daily_reports_a_stable_message_when_the_database_is_down() {
    let err = call_with_db_down("game.getDaily", &Value::Null)
        .await
        .expect_err("the pool is never dialled, so this must fail");

    assert_eq!(err, "read the current utc date failed");
    assert_carries_no_schema_detail(&err, "game.getDaily");
}

#[tokio::test]
async fn user_is_email_unique_reports_a_stable_message_when_the_database_is_down() {
    let err = call_with_db_down("user.isEmailUnique", &json!({ "email": "a@example.com" }))
        .await
        .expect_err("the pool is never dialled, so this must fail");

    assert_eq!(err, "check the email address failed");
    assert_carries_no_schema_detail(&err, "user.isEmailUnique");
}

#[tokio::test]
async fn team_list_reports_a_stable_message_when_the_database_is_down() {
    let err = call_with_db_down("team.list", &Value::Null)
        .await
        .expect_err("the pool is never dialled, so this must fail");

    assert_eq!(err, "list the teams failed");
    assert_carries_no_schema_detail(&err, "team.list");
}

#[tokio::test]
async fn stats_leaderboard_reports_a_stable_message_when_the_database_is_down() {
    let err = call_with_db_down("stats.getGlobalLeaderboard", &Value::Null)
        .await
        .expect_err("the pool is never dialled, so this must fail");

    assert_eq!(err, "load the leaderboard failed");
    assert_carries_no_schema_detail(&err, "stats.getGlobalLeaderboard");
}

// ── 3. source guard: the pattern is gone from every router ────────────────────

fn src_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("src")
}

fn rust_sources(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir).expect("src/ is readable") {
        let path = entry.expect("dir entry").path();
        if path.is_dir() {
            rust_sources(&path, out);
        } else if path.extension().and_then(|e| e.to_str()) == Some("rs") {
            out.push(path);
        }
    }
}

/// AC1/AC2: no router may reintroduce the raw pattern, including a route this
/// file never calls. The deliberately-different sites carry an explicit error
/// type (`|e: ort::Error<SessionBuilder>|`, `|e: tokio::task::JoinError|`,
/// `|e: crossword_db::AppError|`) so they do not match.
#[test]
fn no_router_reintroduces_the_raw_db_error_stringify() {
    let root = src_dir();
    let mut files = Vec::new();
    rust_sources(&root, &mut files);
    assert!(
        files.len() > 10,
        "expected the whole server tree, got {files:?}"
    );

    let needle = "map_err(|e| e.to_string())";
    let offenders: Vec<String> = files
        .iter()
        .filter_map(|f| {
            let rel = f.strip_prefix(&root).unwrap_or(f).display().to_string();
            fs::read_to_string(f)
                .ok()?
                .lines()
                .enumerate()
                .find(|(_, line)| line.contains(needle))
                .map(|(i, _)| format!("{rel}:{}", i + 1))
        })
        .collect();

    assert!(
        offenders.is_empty(),
        "raw database error stringify reintroduced at {offenders:?}"
    );
}

/// The sites left deliberately different must keep naming their real error
/// type; an unannotated `|e|` there is the tell that someone converted a
/// non-database error by reflex (AC5).
#[test]
fn non_database_error_sites_stay_type_annotated() {
    let cases: [(&str, usize); 3] = [
        ("routers/user.rs", 2),
        ("routers/generator/mod.rs", 2),
        ("routers/generator/embed.rs", 16),
    ];
    for (rel, expected) in cases {
        let src = fs::read_to_string(src_dir().join(rel)).expect("router source is readable");
        let annotated = src.lines().filter(|l| l.contains(".map_err(|e: ")).count();
        assert_eq!(
            annotated, expected,
            "{rel}: expected {expected} type-annotated non-database map_err sites, found {annotated}"
        );
    }
}
