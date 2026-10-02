mod common;

use crossword_db::AppEvent;
use crossword_events::EventBus;
use crossword_server::pg_events;
use sqlx::{PgPool, Row};
use std::time::Duration;

const CHANNEL: &str = "crossword_app_event";

#[tokio::test]
#[ignore = "requires live DB; run with --ignored and DATABASE_URL"]
async fn outbox_event_reaches_separate_bus() {
    let pool = common::pool().await;
    let _outbox = lock_outbox(&pool).await;
    let sender_origin = "outbox-test-a";
    let receiver_origin = "outbox-test-b";
    let event = AppEvent::GamePresence {
        active_game_id: "ag-outbox".into(),
        user_id: "u-1".into(),
        name: "Ada".into(),
        number: Some(3),
        direction: Some("across".into()),
    };
    let id = insert_row(&pool, sender_origin, &event).await;

    let receiver = EventBus::default();
    let mut rx = receiver.subscribe();
    pg_events::relay_event_for_test(&pool, &receiver, receiver_origin, &id).await;

    assert_eq!(rx.recv().await.unwrap(), event);
}

#[tokio::test]
#[ignore = "requires live DB; run with --ignored and DATABASE_URL"]
async fn outbox_drops_events_from_same_origin() {
    let pool = common::pool().await;
    let _outbox = lock_outbox(&pool).await;
    let origin = "outbox-test-same-origin";
    let event = AppEvent::GameCompleted {
        active_game_id: "ag-outbox".into(),
        completed_game_id: "cg-outbox".into(),
    };
    let id = insert_row(&pool, origin, &event).await;

    let receiver = EventBus::default();
    let mut rx = receiver.subscribe();
    pg_events::relay_event_for_test(&pool, &receiver, origin, &id).await;

    assert!(rx.try_recv().is_err());
}

#[tokio::test]
#[ignore = "requires live DB; run with --ignored and DATABASE_URL"]
async fn outbox_prune_deletes_old_rows() {
    let pool = common::pool().await;
    let _outbox = lock_outbox(&pool).await;
    let id = insert_backdated_row(
        &pool,
        "outbox-test-prune",
        &AppEvent::GameCompleted {
            active_game_id: "ag-outbox".into(),
            completed_game_id: "cg-outbox".into(),
        },
    )
    .await;

    pg_events::prune_outbox(&pool).await;

    let row = sqlx::query(r#"SELECT count(*) AS count FROM "EventOutbox" WHERE "id" = $1"#)
        .bind(id)
        .fetch_one(&pool)
        .await
        .unwrap();
    let count: i64 = row.get("count");
    assert_eq!(count, 0);
}

// --- DEF-275: recovery from a listener outage ------------------------------
//
// `pg_notify` is not durable, so an event published while a pod's listener is
// disconnected is lost to it — the row survives for OUTBOX_RETENTION with
// nothing reading it. These cover the two ways that happens: the listener
// being down for a while, and the whole pod restarting.

#[tokio::test]
#[ignore = "requires live DB; run with --ignored and DATABASE_URL"]
async fn events_published_during_a_listener_outage_are_replayed_on_reconnect() {
    let pool = common::pool().await;
    let _outbox = lock_outbox(&pool).await;
    // A fresh relay id per run: the cursor is durable, so reusing a fixed one
    // would replay rows left behind by the previous run of this very test.
    let relay = &unique_relay("def-275-outage");
    let origin = "def-275-outage-pod-1";
    let peer = "def-275-outage-peer";
    let bus = EventBus::default();
    let mut rx = bus.subscribe();

    // First connect: establishes the durable cursor, nothing to catch up on.
    assert!(
        pg_events::replay_for_test(&pool, &bus, origin, relay, &mut rx)
            .await
            .delivered
            .is_empty()
    );

    // The listener goes away. Everything below is published while it is down,
    // which is exactly what `pg_notify` drops.
    let presence = presence(1);
    let completed = completion(1);
    insert_row(&pool, peer, &presence).await;
    insert_row(&pool, peer, &completed).await;

    // Reconnect. Both rows are inside OUTBOX_RETENTION, so both arrive, in `seq`
    // order — which is commit order. Asserted on the events rather than on a row
    // count: these tests share one database, so the replay may also walk rows
    // another test inserted.
    let replayed = pg_events::replay_for_test(&pool, &bus, origin, relay, &mut rx).await;
    assert!(
        replayed.rows >= 2,
        "expected the two outage rows to be walked, got {}",
        replayed.rows
    );
    assert!(
        replayed.delivered.contains(&presence) && replayed.delivered.contains(&completed),
        "replayed {:?}",
        replayed.delivered
    );
    assert!(
        replayed.delivered.iter().position(|e| e == &presence)
            < replayed.delivered.iter().position(|e| e == &completed),
        "replay delivered out of commit order: {:?}",
        replayed.delivered
    );

    // Replaying again must not repeat them: the cursor moved past both.
    let again = pg_events::replay_for_test(&pool, &bus, origin, relay, &mut rx).await;
    assert!(
        again.delivered.is_empty(),
        "replay repeated already-delivered events: {:?}",
        again.delivered
    );
}

#[tokio::test]
#[ignore = "requires live DB; run with --ignored and DATABASE_URL"]
async fn a_pod_restart_does_not_redeliver_game_completed() {
    let pool = common::pool().await;
    let _outbox = lock_outbox(&pool).await;
    let relay = &unique_relay("def-275-restart");
    let peer = "def-275-restart-peer";
    let bus = EventBus::default();
    let mut rx = bus.subscribe();

    let before = "def-275-restart-pod-pid-1";
    pg_events::replay_for_test(&pool, &bus, before, relay, &mut rx).await;

    // The pod delivers the completion, which advances its cursor past it.
    let completed = completion(2);
    insert_row(&pool, peer, &completed).await;
    let first = pg_events::replay_for_test(&pool, &bus, before, relay, &mut rx).await;
    assert!(
        first.delivered.contains(&completed),
        "the first delivery did not reach subscribers: {:?}",
        first.delivered
    );

    // Restart: same pod, so the same relay id and therefore the same durable
    // cursor; new process, so a new origin and no memory of what it delivered.
    // This is the case a naive "replay everything newer than an empty
    // watermark" gets wrong — it navigates players to the results page twice.
    let after = "def-275-restart-pod-pid-2";
    let restarted = pg_events::replay_for_test(&pool, &bus, after, relay, &mut rx).await;
    assert!(
        restarted.delivered.iter().all(|e| e != &completed),
        "a restart re-delivered GameCompleted, which navigates players to the results page twice: \
         {:?}",
        restarted.delivered
    );
}

#[tokio::test]
#[ignore = "requires live DB; run with --ignored and DATABASE_URL"]
async fn a_relay_with_no_cursor_replays_no_history() {
    let pool = common::pool().await;
    let _outbox = lock_outbox(&pool).await;
    let peer = "def-275-cold-peer";
    let bus = EventBus::default();
    let mut rx = bus.subscribe();

    // Published before this relay ever connected, so it has no subscriber that
    // missed them. Replaying them would hand a fresh subscriber a completion
    // that navigates it straight to the results page.
    insert_row(&pool, peer, &completion(3)).await;

    let relay = &unique_relay("def-275-cold");
    let cold = pg_events::replay_for_test(&pool, &bus, "def-275-cold-pod", relay, &mut rx).await;
    assert!(
        !cold.delivered.contains(&completion(3)),
        "a relay with no cursor replayed history it never saw: {:?}",
        cold.delivered
    );

    // It still picks up everything published from here on.
    let presence = presence(3);
    insert_row(&pool, peer, &presence).await;
    let warm = pg_events::replay_for_test(&pool, &bus, "def-275-cold-pod", relay, &mut rx).await;
    assert!(
        warm.delivered.contains(&presence),
        "a new relay did not pick up events published after it connected: {:?}",
        warm.delivered
    );
}

#[tokio::test]
#[ignore = "requires live DB; run with --ignored and DATABASE_URL"]
async fn a_pod_away_longer_than_retention_loses_those_events() {
    let pool = common::pool().await;
    let _outbox = lock_outbox(&pool).await;
    let _ = tracing_subscriber::fmt()
        .with_env_filter("crossword_server=warn")
        .with_test_writer()
        .try_init();
    let relay = &unique_relay("def-275-expired");
    let peer = "def-275-expired-peer";
    let bus = EventBus::default();
    let mut rx = bus.subscribe();

    // Its last delivery, then a gap longer than the reaper keeps rows for.
    pg_events::replay_for_test(&pool, &bus, "def-275-expired-1", relay, &mut rx).await;
    insert_row(&pool, peer, &presence(7)).await;
    let before_gap =
        pg_events::replay_for_test(&pool, &bus, "def-275-expired-1", relay, &mut rx).await;
    assert!(before_gap.delivered.contains(&presence(7)));

    insert_backdated_row(&pool, peer, &presence(8)).await;
    insert_row(&pool, peer, &presence(9)).await;
    pg_events::prune_outbox(&pool).await;

    // The backdated row is gone, so the replay cannot deliver it: nothing can,
    // which is why the relay logs that it is permanently behind instead of
    // pretending it caught up. The row after the hole still arrives.
    let after_gap =
        pg_events::replay_for_test(&pool, &bus, "def-275-expired-2", relay, &mut rx).await;
    assert!(
        after_gap.delivered.contains(&presence(9)),
        "the relay did not resume after the pruned hole: {:?}",
        after_gap.delivered
    );
    assert!(
        !after_gap.delivered.contains(&presence(8)),
        "a pruned row was replayed"
    );
}

#[tokio::test]
#[ignore = "requires live DB; run with --ignored and DATABASE_URL"]
async fn a_relay_recovers_when_its_listener_backend_is_killed() {
    let pool = common::pool().await;
    let _outbox = lock_outbox(&pool).await;
    let relay = &unique_relay("def-275-killed");
    let peer = "def-275-killed-peer";

    // Backends from an already-exited process linger in pg_stat_activity until
    // the server next touches the socket, so clear them before starting the
    // relay this test means to kill.
    terminate_listeners(&pool).await;
    let bus = EventBus::default();
    let mut rx = bus.subscribe();
    let relay_loop = pg_events::spawn_relay_for_test(pool.clone(), bus, relay, relay);
    wait_until_listening(&pool, relay).await;

    // Liveness: the relay is listening and republishing a peer's event.
    let before = listening_backends(&pool).await;
    assert_eq!(before.len(), 1, "expected exactly one listener backend");
    let live = presence(4);
    insert_row(&pool, peer, &live).await;
    let got = pg_events::collect_for_test(&mut rx, 1, Duration::from_secs(20)).await;
    assert!(
        got.contains(&live),
        "relay never started listening; got {got:?}"
    );

    // Kill the connection it listens on. `pg_notify` is not durable, so the
    // replay on reconnect is now the only way the next event can arrive.
    assert!(terminate_listeners(&pool).await >= 1, "no listener to kill");

    let missed = presence(5);
    insert_row(&pool, peer, &missed).await;

    // RETRY_DELAY is 5s, so allow for it plus the reconnect handshake. Draining
    // rather than taking the first event: a previous test's row can still be
    // sitting in the replay window, and this test asserts that *this* event came
    // through, not that it came through first.
    let recovered = pg_events::collect_for_test(&mut rx, 4, Duration::from_secs(30)).await;
    assert!(
        recovered.contains(&missed),
        "the relay did not recover an event published while its listener was down; got \
         {recovered:?}"
    );

    // ...and it got there by reconnecting, rather than the killed connection
    // having somehow still been live.
    let after = listening_backends(&pool).await;
    assert_eq!(after.len(), 1);
    assert_ne!(
        after[0], before[0],
        "the relay never opened a new connection"
    );

    relay_loop.abort();
}

/// Insert an outbox row the way a peer's writer does — numbered from the same
/// counter, then notified — and return its id.
async fn insert_row(pool: &PgPool, origin: &str, event: &AppEvent) -> String {
    let id = uuid::Uuid::new_v4().to_string();
    let seq = next_seq(pool).await;
    sqlx::query(
        r#"INSERT INTO "EventOutbox" ("id", "seq", "payload", "origin")
           VALUES ($1, $2, $3, $4)"#,
    )
    .bind(&id)
    .bind(seq)
    .bind(serde_json::to_value(event).unwrap())
    .bind(origin)
    .execute(pool)
    .await
    .unwrap();
    sqlx::query("SELECT pg_notify($1, $2)")
        .bind(CHANNEL)
        .bind(&id)
        .execute(pool)
        .await
        .unwrap();
    id
}

/// As `insert_row`, but dated outside the retention window so the reaper takes it.
async fn insert_backdated_row(pool: &PgPool, origin: &str, event: &AppEvent) -> String {
    let id = uuid::Uuid::new_v4().to_string();
    let seq = next_seq(pool).await;
    sqlx::query(
        r#"INSERT INTO "EventOutbox" ("id", "seq", "payload", "origin", "createdAt")
           VALUES ($1, $2, $3, $4, now() - interval '20 minutes')"#,
    )
    .bind(&id)
    .bind(seq)
    .bind(serde_json::to_value(event).unwrap())
    .bind(origin)
    .execute(pool)
    .await
    .unwrap();
    id
}

async fn next_seq(pool: &PgPool) -> i64 {
    sqlx::query_scalar(
        r#"INSERT INTO "EventOutboxSeq" ("id", "seq") VALUES (1, 1)
           ON CONFLICT ("id") DO UPDATE SET "seq" = "EventOutboxSeq"."seq" + 1
           RETURNING "seq""#,
    )
    .fetch_one(pool)
    .await
    .unwrap()
}

/// Block until the relay has finished its first connect-and-replay.
///
/// Its cursor row is the signal, because `listen_once` registers `LISTEN`
/// *before* the replay writes one: once the row exists the session is listening,
/// and a `pg_notify` sent after this returns cannot be dropped on the floor.
/// Counting `LISTEN` backends instead would be fooled by one lingering from an
/// already-exited process.
async fn wait_until_listening(pool: &PgPool, relay_id: &str) {
    tokio::time::timeout(Duration::from_secs(20), async {
        loop {
            let cursor: i64 = sqlx::query_scalar(
                r#"SELECT count(*) FROM "EventRelayCursor" WHERE "relayId" = $1"#,
            )
            .bind(relay_id)
            .fetch_one(pool)
            .await
            .unwrap();
            if cursor > 0 {
                return;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await
    .expect("the relay never registered its LISTEN");
}

async fn listening_backends(pool: &PgPool) -> Vec<i32> {
    sqlx::query_scalar(
        r#"SELECT pid FROM pg_stat_activity
           WHERE datname = current_database() AND pid <> pg_backend_pid()
             AND query ILIKE 'listen %' ORDER BY pid"#,
    )
    .fetch_all(pool)
    .await
    .unwrap()
}

/// Kill every backend currently sitting in `LISTEN` and report how many there
/// were.
async fn terminate_listeners(pool: &PgPool) -> usize {
    let rows = sqlx::query(
        r#"SELECT pid FROM pg_stat_activity
           WHERE datname = current_database() AND pid <> pg_backend_pid()
             AND query ILIKE 'listen %'"#,
    )
    .fetch_all(pool)
    .await
    .unwrap();
    for row in &rows {
        let pid: i32 = row.get("pid");
        sqlx::query_scalar::<_, bool>("SELECT pg_terminate_backend($1)")
            .bind(pid)
            .fetch_one(pool)
            .await
            .unwrap();
    }
    rows.len()
}

/// Take the outbox exclusively for as long as the returned guard lives.
///
/// `EventOutbox` is one global queue, so these tests cannot meaningfully run
/// concurrently: every test's replay walks *every* row past its cursor, which
/// includes rows another test inserted a moment earlier. Rather than assert on
/// counts — the thing that made that interference a flaky failure — the tests
/// share a session-scoped advisory lock, so one runs at a time whichever order
/// the harness picks.
///
/// `pg_advisory_lock` belongs to the session, not the transaction, so the lock
/// is only released when the holding connection drops. The guard therefore owns
/// a dedicated connection and releases by dropping it.
struct OutboxLock {
    _conn: sqlx::pool::PoolConnection<sqlx::Postgres>,
}

async fn lock_outbox(pool: &PgPool) -> OutboxLock {
    let mut conn = pool.acquire().await.expect("must reach the database");
    sqlx::query("SELECT pg_advisory_lock($1)")
        .bind(0x0def_2750_u32 as i64)
        .execute(&mut *conn)
        .await
        .expect("must take the outbox lock");
    OutboxLock { _conn: conn }
}

/// A relay id that has no cursor yet, so a test always starts from a cold
/// relay rather than inheriting the position a previous run left in the shared
/// test database.
fn unique_relay(prefix: &str) -> String {
    format!("{prefix}-{}", uuid::Uuid::new_v4())
}

fn presence(n: i32) -> AppEvent {
    AppEvent::GamePresence {
        active_game_id: "ag-def-275".into(),
        user_id: format!("u-{n}"),
        name: "Ada".into(),
        number: Some(n),
        direction: Some("across".into()),
    }
}

fn completion(n: i32) -> AppEvent {
    AppEvent::GameCompleted {
        active_game_id: "ag-def-275".into(),
        completed_game_id: format!("cg-{n}"),
    }
}
