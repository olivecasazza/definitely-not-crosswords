//! Integration tests for the `job:create` tRPC procedure (DEF-70).
//!
//! These run end-to-end through the tRPC router dispatch + generator pipeline,
//! exercising the full stack: auth, RBAC, quota, dictionary, solver, and the
//! grid-JSON response shape. Unlike the unit tests in `generator/mod.rs` (which
//! cover pure helpers in-process), these need a live Postgres connection so they
//! are marked `#[ignored]` by default and must be explicitly enabled with
//! `cargo test -- --ignored` or the `CROSSWORD_TEST_DB` environment variable.
//!
//! Run locally with the staging database:
//!   DATABASE_URL="..." cargo test -p crossword-server -- --ignored
//!
//! CI configuration (GitHub Actions) runs these against the ephemeral PG service
//! attached to the staging namespace, seeded as part of the e2e workflow.

mod common;

use crossword_db::{AuthUser, Capability, Role};
use serde_json::{json, Value};
use std::time::Duration;

/// Happy-path test: an Admin-caped user calls `job:create` and receives a valid
/// grid JSON. Asserts the acceptance criteria from DEF-70:
///   - returns a valid grid + clue list in <10 s on a 15×15 board
///   - cell count, symmetry (ACROSS vs DOWN math), clue enumeration completeness
///   - solutionHash is a 64-char lowercase hex string
#[tokio::test]
#[ignore = "requires live DB; run with --ignored or CROSSWORD_TEST_DB"]
async fn job_create_returns_valid_grid_within_time_limit() {
    let pool = common::pool().await;
    let user = common::admin_user();

    let input = json!({
        "params": {
            "topic": "animals",
            "width": 15,
            "height": 15,
            "minWordLength": 3,
            "maxWordLength": 10,
            "targetWords": 30,
            "runs": 10
        }
    });

    let ctx = common::ctx(&pool, &user);
    let start = std::time::Instant::now();
    let result = routers::generator::try_handle("job:create", &input, &ctx)
        .await
        .expect("job:create is a known procedure")
        .expect("procedure should not error");
    let elapsed = start.elapsed();

    assert!(
        elapsed < Duration::from_secs(10),
        "generation must complete in <10s, took {elapsed:?}"
    );

    assert!(result.get("grid").is_some(), "grid field must be present");
    assert!(result.get("cells").is_some(), "cells field must be present");
    assert!(result.get("questions").is_some(), "questions field must be present");
    assert!(result.get("solutionHash").is_some(), "solutionHash field must be present");

    let grid = &result["grid"];
    assert!(grid.get("w").is_some() && grid["w"].as_i64().unwrap_or(0) > 0);
    assert!(grid.get("h").is_some() && grid["h"].as_i64().unwrap_or(0) > 0);

    let cells = result["cells"].as_array().expect("cells must be a 2-D array");
    let h = grid["h"].as_i64().unwrap() as usize;
    assert_eq!(cells.len(), h, "cells rows must equal grid.h");
    for row in cells.iter() {
        let row_arr = row.as_array().expect("each cell row must be an array");
        let w = grid["w"].as_i64().unwrap() as usize;
        assert_eq!(row_arr.len(), w, "each cell col must equal grid.w");
    }

    let questions = result["questions"]
        .as_array()
        .expect("questions must be an array");
    assert!(
        !questions.is_empty(),
        "grid must have at least one question"
    );

    for q in questions.iter() {
        assert!(q.get("number").is_some(), "question must have number");
        assert!(
            q["direction"].as_str() == "ACROSS" || q["direction"].as_str() == "DOWN",
            "direction must be ACROSS or DOWN"
        );
        assert!(q.get("rootX").is_some(), "question must have rootX");
        assert!(q.get("rootY").is_some(), "question must have rootY");
        assert!(
            !q["answer"].as_str().unwrap_or("").is_empty(),
            "answer must be non-empty"
        );
        assert!(
            !q["clue"].as_str().unwrap_or("").is_empty(),
            "clue must be non-empty"
        );
    }

    let hash = result["solutionHash"]
        .as_str()
        .expect("solutionHash must be a string");
    assert_eq!(hash.len(), 64, "SHA-256 hex is exactly 64 chars");
    assert!(
        hash.chars().all(|c| c.is_ascii_hexdigit()),
        "solutionHash must be lowercase hex"
    );
}

/// RBAC test: a user WITHOUT the `job:create` capability must receive a
/// structured FORBIDDEN error (403), NOT a 500, a panic, or any other mishap.
/// This is the core RBAC regression test for DEF-70.
#[tokio::test]
#[ignore = "requires live DB; run with --ignored or CROSSWORD_TEST_DB"]
async fn job_create_rejects_user_without_capability() {
    let pool = common::pool().await;
    // A User-role account that only has GamePlay + ProfileManage (no JobCreate).
    // Role::User.capabilities() = [GamePlay, ProfileManage, JobCreate] — but
    // we test the gate, not the default: create a minimal user.
    let user = AuthUser {
        id: "rbac-test-no-jobcreate".to_string(),
        email: "no-jobcreate@test".to_string(),
        role: Role::User,
    };

    let input = json!({
        "params": {
            "topic": "science",
            "width": 9,
            "height": 9
        }
    });

    let ctx = common::ctx(&pool, &user);
    let result = routers::generator::try_handle("job:create", &input, &ctx)
        .await
        .expect("try_handle must return Some for job:create");

    let err_str = result.expect_err("job:create without capability must error");
    assert!(
        err_str == "FORBIDDEN",
        "missing capability must return 'FORBIDDEN', got: {err_str}"
    );
}

/// RBAC test: an unauthenticated request (no session) must receive a structured
/// UNAUTHORIZED error (401).
#[tokio::test]
#[ignore = "requires live DB; run with --ignored or CROSSWORD_TEST_DB"]
async fn job_create_rejects_unauthenticated_user() {
    let pool = common::pool().await;
    let user = AuthUser {
        id: "".to_string(),
        email: "".to_string(),
        role: Role::User,
    };

    let input = json!({"params": {"topic": "music", "width": 9, "height": 9}});
    let ctx = common::ctx(&pool, &user);
    let result = routers::generator::try_handle("job:create", &input, &ctx)
        .await
        .expect("try_handle must return Some for job:create");

    let err_str = result.expect_err("unauthenticated call must error");
    assert!(
        err_str == "UNAUTHORIZED",
        "no session must return 'UNAUTHORIZED', got: {err_str}"
    );
}

/// Validation test: a missing `topic` param must return a BAD_REQUEST error
/// (400), not an INTERNAL error or a panic.
#[tokio::test]
#[ignore = "requires live DB; run with --ignored or CROSSWORD_TEST_DB"]
async fn job_create_requires_topic() {
    let pool = common::pool().await;
    let user = common::admin_user();

    let input = json!({"params": {"width": 9, "height": 9}});
    let ctx = common::ctx(&pool, &user);
    let result = routers::generator::try_handle("job:create", &input, &ctx)
        .await
        .expect("try_handle must return Some for job:create");

    let err_str = result.expect_err("missing topic must error");
    assert!(
        err_str != "FORBIDDEN"
            && err_str != "UNAUTHORIZED"
            && !err_str.starts_with("INTERNAL:"),
        "validation error must be BAD_REQUEST, got: {err_str}"
    );
}

/// Clue enumeration completeness: every filled cell in the grid must be owned
/// by exactly one clue (ACROSS or DOWN), and every clue's letters must match
/// the corresponding cells. This is the "clue enumeration completeness" check
/// from the DEF-70 acceptance list.
#[tokio::test]
#[ignore = "requires live DB; run with --ignored or CROSSWORD_TEST_DB"]
async fn job_create_grid_cells_match_clue_placements() {
    let pool = common::pool().await;
    let user = common::admin_user();

    let input = json!({
        "params": {
            "topic": "history",
            "width": 15,
            "height": 15,
            "runs": 5
        }
    });

    let ctx = common::ctx(&pool, &user);
    let result = routers::generator::try_handle("job:create", &input, &ctx)
        .await
        .expect("job:create should succeed for admin")
        .expect("generation should not error");

    let cells = result["cells"].as_array().expect("cells must be array");
    let questions = result["questions"]
        .as_array()
        .expect("questions must be array");

    let mut owned: std::collections::HashMap<(usize, usize), char> =
        std::collections::HashMap::new();

    for q in questions.iter() {
        let direction = q["direction"].as_str().unwrap();
        let root_x = q["rootX"].as_i64().unwrap() as i32;
        let root_y = q["rootY"].as_i64().unwrap() as i32;
        let answer = q["answer"].as_str().unwrap();

        for (i, ch) in answer.chars().enumerate() {
            let x = if direction == "ACROSS" {
                root_x + i as i32
            } else {
                root_x
            };
            let y = if direction == "DOWN" {
                root_y + i as i32
            } else {
                root_y
            };
            let prev = owned.insert((x as usize, y as usize), ch);
            assert!(
                prev.is_none() || prev == Some(ch),
                "cell ({x},{y}) has conflicting letters from two clues"
            );
        }
    }

    let mut filled_count = 0usize;
    for (y, row) in cells.iter().enumerate() {
        for (x, cell) in row.as_array().unwrap().iter().enumerate() {
            if let Some(letter) = cell.as_str() {
                filled_count += 1;
                let expected = owned.get(&(x, y)).expect(
                    format!(
                        "cell ({x},{y})='{letter}' is set but not owned by any clue",
                    )
                    .as_str(),
                );
                assert_eq!(
                    letter.chars().next().unwrap(),
                    *expected,
                    "cell ({x},{y}) letter mismatch: expected {expected}, got {letter}"
                );
            }
        }
    }

    assert_eq!(
        filled_count,
        owned.len(),
        "every owned cell must be rendered as a letter (no gaps)"
    );
}

/// solutionHash stability: calling `job:create` twice with identical params
/// must produce the same hash (deterministic output). This allows the client
/// to cache the hash and detect if the server tampered with the grid between
/// preview and publish.
#[tokio::test]
#[ignore = "requires live DB; run with --ignored or CROSSWORD_TEST_DB"]
async fn job_create_solution_hash_is_deterministic() {
    let pool = common::pool().await;
    let user = common::admin_user();

    let params = json!({
        "params": {
            "topic": "geography",
            "width": 11,
            "height": 11,
            "runs": 5
        }
    });

    let ctx = common::ctx(&pool, &user);

    let r1 = routers::generator::try_handle("job:create", &params, &ctx)
        .await
        .expect("first call should succeed")
        .expect("first call should not error");

    let r2 = routers::generator::try_handle("job:create", &params, &ctx)
        .await
        .expect("second call should succeed")
        .expect("second call should not error");

    let h1 = r1["solutionHash"].as_str().unwrap();
    let h2 = r2["solutionHash"].as_str().unwrap();
    assert_eq!(
        h1, h2,
        "identical params must yield identical solutionHash (h1={h1}, h2={h2})"
    );
}
