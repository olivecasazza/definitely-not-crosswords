//! Live-Postgres regression coverage for active-game write serialisation.
//!
//! The browser soak proves that crossing writes reach a consumer and that the
//! final cell is coherent, but deliberately does not assert the `previousState`
//! values returned to the client. This test drives the same router entry point
//! with two concurrent writers. A database advisory-lock rendezvous is only a
//! harness for making both requests overlap; the correctness assertion is the
//! consumer-visible action chain, not the locking mechanism itself.

mod common;

use crossword_db::{AuthUser, Role};
use crossword_server::routers;
use serde_json::{json, Value};
use sqlx::{PgPool, Row};
use std::time::Duration;
use uuid::Uuid;

fn user(id: &str) -> AuthUser {
    AuthUser {
        id: id.to_string(),
        email: format!("{id}@integration.test"),
        role: Role::User,
    }
}

fn action_from(result: &Result<Value, String>) -> Option<&Value> {
    result
        .as_ref()
        .ok()
        .and_then(|value| value.get("actions"))
        .and_then(Value::as_array)
        .and_then(|actions| actions.first())
}

fn action_rows(results: &[Result<Value, String>]) -> Option<Vec<(String, String, String, String)>> {
    results
        .iter()
        .map(|result| {
            let action = action_from(result)?;
            Some((
                action.get("id")?.as_str()?.to_string(),
                action.get("userId")?.as_str()?.to_string(),
                action.get("state")?.as_str()?.to_string(),
                action.get("previousState")?.as_str()?.to_string(),
            ))
        })
        .collect()
}

fn has_exact_chain(rows: &[(String, String, String, String)], user_a: &str, user_b: &str) -> bool {
    let actual: Vec<(&str, &str, &str)> = rows
        .iter()
        .map(|(_, user, state, previous)| (user.as_str(), state.as_str(), previous.as_str()))
        .collect();
    (actual.contains(&(user_a, "X", "")) && actual.contains(&(user_b, "Y", "X")))
        || (actual.contains(&(user_b, "Y", "")) && actual.contains(&(user_a, "X", "Y")))
}

/// Rows are unique, so cleanup is safe even if setup or the test body fails.
/// The explicit async call is used on the normal path; `Drop` is a best-effort
/// fallback for a panic/timeout before the normal cleanup point.
struct FixtureGuard {
    pool: PgPool,
    game_id: String,
    user_ids: [String; 2],
    cleaned: bool,
}

impl FixtureGuard {
    async fn cleanup(&mut self) -> Result<(), String> {
        let game_error = sqlx::query(r#"DELETE FROM "Game" WHERE id = $1"#)
            .bind(&self.game_id)
            .execute(&self.pool)
            .await
            .err()
            .map(|error| error.to_string());
        let users_error = sqlx::query(r#"DELETE FROM "User" WHERE id IN ($1, $2)"#)
            .bind(&self.user_ids[0])
            .bind(&self.user_ids[1])
            .execute(&self.pool)
            .await
            .err()
            .map(|error| error.to_string());
        self.cleaned = game_error.is_none() && users_error.is_none();
        match game_error.or(users_error) {
            Some(error) => Err(error),
            None => Ok(()),
        }
    }
}

impl Drop for FixtureGuard {
    fn drop(&mut self) {
        if self.cleaned {
            return;
        }
        let Ok(handle) = tokio::runtime::Handle::try_current() else {
            return;
        };
        let pool = self.pool.clone();
        let game_id = self.game_id.clone();
        let user_ids = self.user_ids.clone();
        handle.spawn(async move {
            let _ = sqlx::query(r#"DELETE FROM "Game" WHERE id = $1"#)
                .bind(game_id)
                .execute(&pool)
                .await;
            let _ = sqlx::query(r#"DELETE FROM "User" WHERE id IN ($1, $2)"#)
                .bind(&user_ids[0])
                .bind(&user_ids[1])
                .execute(&pool)
                .await;
        });
    }
}

#[tokio::test]
#[ignore = "requires disposable live DB; run with --ignored and DATABASE_URL"]
async fn concurrent_same_cell_writes_return_a_serialised_previous_state_chain() {
    let pool = common::pool().await;
    let game_id = Uuid::new_v4().to_string();
    let active_game_id = Uuid::new_v4().to_string();
    let question_id = Uuid::new_v4().to_string();
    let member_a_id = Uuid::new_v4().to_string();
    let member_b_id = Uuid::new_v4().to_string();
    let user_a_id = Uuid::new_v4().to_string();
    let user_b_id = Uuid::new_v4().to_string();
    let user_a = user(&user_a_id);
    let user_b = user(&user_b_id);
    let mut fixture = FixtureGuard {
        pool: pool.clone(),
        game_id: game_id.clone(),
        user_ids: [user_a_id.clone(), user_b_id.clone()],
        cleaned: false,
    };

    sqlx::query(r#"INSERT INTO "User" (id, name, email) VALUES ($1, $2, $3), ($4, $5, $6)"#)
        .bind(&user_a_id)
        .bind("Concurrent A")
        .bind(&user_a.email)
        .bind(&user_b_id)
        .bind("Concurrent B")
        .bind(&user_b.email)
        .execute(&pool)
        .await
        .expect("insert test users");
    sqlx::query(
        r#"INSERT INTO "Game" (id, title, published, "updatedAt")
           VALUES ($1, 'Concurrent previousState regression', true, now())"#,
    )
    .bind(&game_id)
    .execute(&pool)
    .await
    .expect("insert test game");
    sqlx::query(
        r#"INSERT INTO "Question"
           (id, number, answer, "questionText", "rootX", "rootY", direction, "gameId")
           VALUES ($1, 1, 'AB', 'test answer', 0, 0, 'ACROSS', $2)"#,
    )
    .bind(&question_id)
    .bind(&game_id)
    .execute(&pool)
    .await
    .expect("insert test question");
    sqlx::query(
        r#"INSERT INTO "ActiveGame" (id, "gameId", "updatedAt")
           VALUES ($1, $2, now())"#,
    )
    .bind(&active_game_id)
    .bind(&game_id)
    .execute(&pool)
    .await
    .expect("insert test active game");
    sqlx::query(
        r#"INSERT INTO "GameMember"
           (id, "userId", "isOwner", "activeGameId", "updatedAt")
           VALUES ($1, $2, true, $3, now()), ($4, $5, false, $3, now())"#,
    )
    .bind(&member_a_id)
    .bind(&user_a_id)
    .bind(&active_game_id)
    .bind(&member_b_id)
    .bind(&user_b_id)
    .execute(&pool)
    .await
    .expect("insert test members");

    let lock_key = format!("activeGame:write:{active_game_id}");
    let mut barrier_connection = pool
        .acquire()
        .await
        .expect("acquire lock rendezvous connection");
    sqlx::query("SELECT pg_advisory_lock(hashtext($1))")
        .bind(&lock_key)
        .execute(&mut *barrier_connection)
        .await
        .expect("hold the game write lock for the rendezvous");

    let (started_a_tx, started_a_rx) = tokio::sync::oneshot::channel();
    let (started_b_tx, started_b_rx) = tokio::sync::oneshot::channel();
    let first_ctx = common::ctx(&pool, &user_a);
    let second_ctx = common::ctx(&pool, &user_b);
    let first_input = json!({
        "id": active_game_id,
        "actions": [{ "cordX": 0, "cordY": 0, "state": "X" }]
    });
    let second_input = json!({
        "id": active_game_id,
        "actions": [{ "cordX": 0, "cordY": 0, "state": "Y" }]
    });

    let mut first = tokio::spawn(async move {
        let _ = started_a_tx.send(());
        routers::dispatch("activeGame.addActions", &first_input, &first_ctx).await
    });
    let mut second = tokio::spawn(async move {
        let _ = started_b_tx.send(());
        routers::dispatch("activeGame.addActions", &second_input, &second_ctx).await
    });
    started_a_rx.await.expect("first writer started");
    started_b_rx.await.expect("second writer started");

    // This rendezvous is a harness precondition, not consumer correctness:
    // both current writers must park on the per-game lock before release.
    // A different serialization mechanism needs an intentionally updated
    // harness; a timeout must never silently turn this into a sequential test.
    let rendezvous: Result<(), String> = match tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let waiting: i64 = sqlx::query_scalar(
                r#"SELECT count(*)::bigint
                       FROM pg_locks
                       WHERE locktype = 'advisory'
                         AND NOT granted
                         AND objid::bigint = (hashtext($1)::bigint & 4294967295)"#,
            )
            .bind(&lock_key)
            .fetch_one(&pool)
            .await
            .map_err(|error| format!("query advisory lock waiters: {error}"))?;
            if waiting >= 2 {
                return Ok(());
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    {
        Ok(result) => result,
        Err(_) => Err("timed out waiting for both writers at the lock rendezvous".to_string()),
    };

    let unlock_result = sqlx::query("SELECT pg_advisory_unlock(hashtext($1))")
        .bind(&lock_key)
        .execute(&mut *barrier_connection)
        .await
        .map_err(|error| format!("release the game write lock rendezvous: {error}"));
    drop(barrier_connection);

    let first_result = match tokio::time::timeout(Duration::from_secs(10), &mut first).await {
        Ok(Ok(result)) => result,
        Ok(Err(error)) => Err(format!("first writer task failed: {error}")),
        Err(_) => {
            first.abort();
            let _ = (&mut first).await;
            Err("first writer task timed out after lock release".to_string())
        }
    };
    let second_result = match tokio::time::timeout(Duration::from_secs(10), &mut second).await {
        Ok(Ok(result)) => result,
        Ok(Err(error)) => Err(format!("second writer task failed: {error}")),
        Err(_) => {
            second.abort();
            let _ = (&mut second).await;
            Err("second writer task timed out after lock release".to_string())
        }
    };
    let results = [first_result, second_result];

    let persisted_result: Result<Vec<(String, String, String, String)>, sqlx::Error> = sqlx::query(
        r#"SELECT id, "userId", state, "previousState"
               FROM "GameAction"
               WHERE "activeGameId" = $1"#,
    )
    .bind(&active_game_id)
    .fetch_all(&pool)
    .await
    .map(|rows| {
        rows.into_iter()
            .map(|row| {
                (
                    row.get::<String, _>("id"),
                    row.get::<String, _>("userId"),
                    row.get::<Option<String>, _>("state").unwrap_or_default(),
                    row.get::<Option<String>, _>("previousState")
                        .unwrap_or_default(),
                )
            })
            .collect()
    });

    // The Game FK cascades through ActiveGame, GameAction, and GameMember.
    let cleanup_result = fixture.cleanup().await;

    let response_rows = action_rows(&results).unwrap_or_default();
    let persisted = persisted_result.expect("read persisted concurrent actions");
    assert!(
        cleanup_result.is_ok(),
        "fixture cleanup failed: {cleanup_result:?}"
    );
    assert!(
        rendezvous.is_ok(),
        "lock rendezvous failed; refusing to accept a sequential run: {rendezvous:?}"
    );
    assert!(
        unlock_result.is_ok(),
        "lock rendezvous release failed: {unlock_result:?}"
    );
    assert!(
        results.iter().all(Result::is_ok),
        "both writes must succeed: {results:?}"
    );
    assert_eq!(
        response_rows.len(),
        2,
        "both responses must contain one action"
    );
    assert!(
        has_exact_chain(&response_rows, &user_a_id, &user_b_id),
        "returned actions must be exactly one empty -> X/Y -> other-letter chain: {response_rows:?}"
    );
    assert_eq!(persisted.len(), 2, "both action rows must persist");
    assert!(
        has_exact_chain(&persisted, &user_a_id, &user_b_id),
        "persisted actions must be exactly one empty -> X/Y -> other-letter chain: {persisted:?}"
    );
    let mut response_identity = response_rows.clone();
    let mut persisted_identity = persisted.clone();
    response_identity.sort_unstable();
    persisted_identity.sort_unstable();
    assert_eq!(
        response_identity, persisted_identity,
        "the returned action ids/users/states must match persisted rows"
    );
    eprintln!("BUILD BOT POSITIVE PROBE 271d7a37310e002ee22f85a79f81451f09deaa59");
}
