//! Auth-gate regression tests for the `stats` router (DEF-123).
//!
//! `stats.getUserStats` used to run with no auth gate and key its query on a
//! client-supplied `email`, so an anonymous caller could read any player's
//! career aggregates, per-game history and global rank by guessing an address.
//! `stats.getCompletedGame` was public for the same reason and returns a
//! per-game roster of names and scores, including for unpublished games.
//!
//! These go through `routers::dispatch` — the same entry point `main.rs`
//! `trpc_post` uses — and assert the refusal happens *before* any SQL runs, so
//! they need no database: the pool is `common::undialled_pool()`. A test that
//! regressed to a live query would surface as a connection error instead of the
//! expected UNAUTHORIZED/FORBIDDEN, which is the point.

mod common;

use crossword_db::{AuthUser, Role};
use crossword_server::routers;
use crossword_server::wire::envelope;
use serde_json::{json, Value};

fn user_a() -> AuthUser {
    AuthUser {
        id: "user-a".to_string(),
        email: "a@example.com".to_string(),
        role: Role::User,
    }
}

fn admin() -> AuthUser {
    AuthUser {
        id: "admin".to_string(),
        email: "admin@example.com".to_string(),
        role: Role::Admin,
    }
}

/// Call a procedure and return the router error string.
async fn call(proc: &str, input: &Value, user: Option<&AuthUser>) -> Result<Value, String> {
    let pool = common::undialled_pool();
    let ctx = common::ctx_as(&pool, user);
    routers::dispatch(proc, input, &ctx).await
}

#[tokio::test]
async fn anonymous_caller_cannot_read_another_players_stats() {
    let err = call(
        "stats.getUserStats",
        &json!({ "email": "a@example.com" }),
        None,
    )
    .await
    .expect_err("no session, no stats");
    assert_eq!(err, "UNAUTHORIZED");
}

#[tokio::test]
async fn anonymous_caller_cannot_read_a_players_stats_even_with_no_email() {
    // The old code demanded an `email`; the new code defaults to the caller's
    // own — but only for a caller who has one.
    let err = call("stats.getUserStats", &Value::Null, None)
        .await
        .expect_err("no session, no stats");
    assert_eq!(err, "UNAUTHORIZED");
}

#[tokio::test]
async fn signed_in_caller_cannot_read_another_players_stats() {
    // The cross-user IDOR: user A asking for user B must be refused, and the
    // reply must not confirm whether B exists.
    let err = call(
        "stats.getUserStats",
        &json!({ "email": "victim@example.com" }),
        Some(&user_a()),
    )
    .await
    .expect_err("cross-user stats read must be refused");
    assert_eq!(err, "FORBIDDEN");
}

#[tokio::test]
async fn admin_cannot_read_another_players_stats() {
    let err = call(
        "stats.getUserStats",
        &json!({ "email": "a@example.com" }),
        Some(&admin()),
    )
    .await
    .expect_err("admin is not exempt from the self scope");
    assert_eq!(err, "FORBIDDEN");
}

#[tokio::test]
async fn cross_user_refusal_reaches_the_client_as_403_and_anonymous_as_401() {
    // What the browser actually sees. The web client decides between "redirect
    // to login" and "you can't see this" off these codes, so the wire shape is
    // part of the contract, not just the router string.
    let anon = envelope(call("stats.getUserStats", &Value::Null, None).await).0;
    assert_eq!(anon[0]["error"]["data"]["code"], "UNAUTHORIZED");
    assert_eq!(anon[0]["error"]["data"]["httpStatus"], 401);

    let cross = envelope(
        call(
            "stats.getUserStats",
            &json!({ "email": "victim@example.com" }),
            Some(&user_a()),
        )
        .await,
    )
    .0;
    assert_eq!(cross[0]["error"]["data"]["code"], "FORBIDDEN");
    assert_eq!(cross[0]["error"]["data"]["httpStatus"], 403);
}

#[tokio::test]
async fn anonymous_caller_cannot_read_a_completed_game_roster() {
    let err = call(
        "stats.getCompletedGame",
        &json!({ "id": "some-completed-game" }),
        None,
    )
    .await
    .expect_err("per-game standings are no longer public");
    assert_eq!(err, "UNAUTHORIZED");
}

#[tokio::test]
async fn signed_in_caller_still_reaches_the_global_leaderboard_query() {
    // `getGlobalLeaderboard` stays public on purpose (DEF-123): an aggregate,
    // with emails gated by `visible_email`. It is the one procedure here that
    // gets past the gate without a session, so assert it does not regress into
    // a UNAUTHORIZED refusal — it proceeds to the (undialled) database instead.
    // Public is a decision, not an oversight; if this starts failing, someone
    // re-gated it and needs to say whether that was deliberate.
    for user in [None, Some(&user_a())] {
        let res = call("stats.getGlobalLeaderboard", &Value::Null, user).await;
        let err = res.expect_err("no database in this test, so the query must fail");
        assert_ne!(err, "UNAUTHORIZED", "leaderboard must stay public");
        assert_ne!(err, "FORBIDDEN", "leaderboard must stay public");
    }
}
