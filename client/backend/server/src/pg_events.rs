//! Cross-pod event fan-out over a Postgres outbox.
//!
//! `EventBus::publish` writes a row and `pg_notify`s its id; every pod `LISTEN`s
//! on that channel and republishes rows written by *other* pods to its local
//! subscribers. `pg_notify` is not durable, so an event published while a pod's
//! listener is disconnected is lost to it, and the row that would have carried
//! it survives for `OUTBOX_RETENTION` with nothing reading it. The cursor below
//! is what reads it.
//!
//! Three properties make replaying into live subscribers safe:
//!
//! 1. `LISTEN` is registered *before* the replay `SELECT`. Rows written during
//!    the handshake are then caught by the replay instead of falling between the
//!    two paths.
//! 2. `EventOutbox.seq` is gap-free and commit-ordered (see the writer), so a
//!    cursor that only ever moves to a `seq` this pod actually processed cannot
//!    step over an undelivered row.
//! 3. The cursor lives in Postgres, so a *restarted* pod resumes from where it
//!    left off instead of re-delivering up to `OUTBOX_RETENTION` of history to
//!    subscribers that already saw it. Re-delivering `GameCompleted` would
//!    navigate players to the results page a second time.
//!
//! A relay with no cursor row is genuinely new — it has delivered nothing, so
//! it starts at the current head rather than replaying history it never saw.
//! Its `LISTEN` is already up by the time that head is read, so it misses
//! nothing; a brand new pod's clients reconcile over the websocket instead
//! (DEF-175's reconnect ladder).

use crossword_db::AppEvent;
use crossword_events::EventBus;
use sqlx::{postgres::PgListener, postgres::PgRow, PgPool, Row};
use std::time::Duration;
use tokio::sync::{broadcast, mpsc};

const CHANNEL: &str = "crossword_app_event";
const RETRY_DELAY: Duration = Duration::from_secs(5);
const OUTBOX_RETENTION: Duration = Duration::from_secs(600);

pub fn event_bus(pool: PgPool) -> EventBus {
    let origin = format!(
        "{}:{}:{}",
        std::env::var("HOSTNAME").unwrap_or_else(|_| "localhost".into()),
        std::process::id(),
        uuid::Uuid::new_v4()
    );
    let (tx, rx) = mpsc::unbounded_channel();
    let bus = EventBus::with_outbox(256, move |event| {
        if tx.send(event).is_err() {
            tracing::error!("event outbox writer is unavailable");
        }
    });
    spawn_writer(pool.clone(), origin.clone(), rx);
    spawn_listener(pool, bus.clone(), origin, relay_id());
    bus
}

/// Identity of this pod's replay cursor.
///
/// It has to survive a container restart (`HOSTNAME` does, in k8s) or the pod
/// would come back with an empty cursor and replay history it already
/// delivered — the `GameCompleted` failure this cursor exists to prevent. It
/// also has to be distinct per live process, or two pods would advance each
/// other's cursor and silently skip deltas. `EVENT_RELAY_ID` overrides it.
fn relay_id() -> String {
    std::env::var("EVENT_RELAY_ID")
        .or_else(|_| std::env::var("HOSTNAME"))
        .unwrap_or_else(|_| "localhost".into())
}

pub async fn prune_outbox(pool: &PgPool) {
    let res = sqlx::query(r#"DELETE FROM "EventOutbox" WHERE "createdAt" < now() - $1::interval"#)
        .bind(format!("{} seconds", OUTBOX_RETENTION.as_secs()))
        .execute(pool)
        .await;

    if let Err(e) = res {
        tracing::error!("event outbox prune failed: {e}");
    }
}

fn spawn_writer(pool: PgPool, origin: String, mut rx: mpsc::UnboundedReceiver<AppEvent>) {
    tokio::spawn(async move {
        while let Some(event) = rx.recv().await {
            if let Err(e) = write_event(&pool, &origin, event).await {
                tracing::error!("event outbox write failed: {e}");
            }
        }
    });
}

async fn write_event(pool: &PgPool, origin: &str, event: AppEvent) -> anyhow::Result<()> {
    let id = uuid::Uuid::new_v4().to_string();
    let payload = serde_json::to_value(event)?;
    let mut tx = pool.begin().await?;

    // `seq` comes from a single counter row bumped inside this transaction, not
    // from a sequence: the row lock serialises writers, so `seq` order is commit
    // order and has no holes. A plain BIGSERIAL cannot promise either —
    // `nextval` runs before commit, so a writer that commits late leaves a hole
    // behind a higher `seq`, and a hole lets a cursor advance past a row that
    // was never delivered. The cost is that all pods serialise here; the
    // transaction is two statements, so the window is microseconds.
    let seq: i64 = sqlx::query_scalar(
        r#"INSERT INTO "EventOutboxSeq" ("id", "seq") VALUES (1, 1)
           ON CONFLICT ("id") DO UPDATE SET "seq" = "EventOutboxSeq"."seq" + 1
           RETURNING "seq""#,
    )
    .fetch_one(&mut *tx)
    .await?;

    sqlx::query(
        r#"INSERT INTO "EventOutbox" ("id", "seq", "payload", "origin") VALUES ($1, $2, $3, $4)"#,
    )
    .bind(&id)
    .bind(seq)
    .bind(payload)
    .bind(origin)
    .execute(&mut *tx)
    .await?;

    // Notified inside the transaction, so the notification is only delivered
    // once the row it names is visible to the reader that follows it up.
    sqlx::query("SELECT pg_notify($1, $2)")
        .bind(CHANNEL)
        .bind(&id)
        .execute(&mut *tx)
        .await?;

    tx.commit().await?;
    Ok(())
}

fn spawn_listener(
    pool: PgPool,
    bus: EventBus,
    origin: String,
    relay_id: String,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        loop {
            match listen_once(&pool, &bus, &origin, &relay_id).await {
                Ok(()) => tracing::warn!("event outbox listener stopped; reconnecting"),
                Err(e) => tracing::error!("event outbox listener failed: {e}"),
            }
            tokio::time::sleep(RETRY_DELAY).await;
        }
    })
}

async fn listen_once(
    pool: &PgPool,
    bus: &EventBus,
    origin: &str,
    relay_id: &str,
) -> anyhow::Result<()> {
    let mut listener = PgListener::connect_with(pool).await?;
    listener.listen(CHANNEL).await?;
    // Recover anything the last connection missed *after* LISTEN is up, so a row
    // written during this handshake is reachable by the replay and not lost.
    let (mut delivered_through, _) = replay(pool, bus, origin, relay_id).await?;

    loop {
        // `try_recv`, not `recv`. `PgListener::recv` is a loop around
        // `try_recv`, and `try_recv` reports a lost connection as `Ok(None)`
        // while reconnecting and re-`LISTEN`ing by itself — `recv` then spins
        // straight on. That hides the one event this module needs to see: the
        // disconnect that dropped notifications. Only `try_recv`'s `None`
        // surfaces it, and that is what makes the replay above reachable.
        match listener.try_recv().await? {
            Some(notification) => {
                // An error here ends the session rather than being logged and
                // stepped over: the cursor did not move, so the reconnect's
                // replay hands the row straight back.
                relay_notification(
                    pool,
                    bus,
                    origin,
                    relay_id,
                    notification.payload(),
                    &mut delivered_through,
                )
                .await?;
            }
            // The connection is gone with sqlx's own reconnect already done, so
            // start over from a fresh listener rather than trusting a session
            // that skipped whatever was published in between. The outer loop's
            // backoff then applies, and the next session replays from the cursor.
            None => {
                tracing::warn!(relay_id, "event outbox listener lost its connection");
                return Ok(());
            }
        }
    }
}

/// Deliver every outbox row this relay has not delivered yet, oldest first, then
/// persist the position. Runs on every (re)connect.
///
/// Returns the highest `seq` processed — this session's high-water mark — and
/// how many rows that covered. Because `seq` is commit-ordered and the rows are
/// walked in order, nothing at or below the mark is still owed, so a
/// notification for one of those rows is a duplicate of this replay rather than
/// news.
async fn replay(
    pool: &PgPool,
    bus: &EventBus,
    origin: &str,
    relay_id: &str,
) -> anyhow::Result<(i64, usize)> {
    let from = cursor_seq(pool, relay_id).await?;
    let rows = sqlx::query(
        r#"SELECT "id", "seq", "payload", "origin" FROM "EventOutbox"
           WHERE "seq" > $1 ORDER BY "seq" ASC"#,
    )
    .bind(from)
    .fetch_all(pool)
    .await?;

    let mut delivered_through = from;
    for row in &rows {
        delivered_through = row.get("seq");
        match decode_remote_event(row, origin) {
            Ok(Some(event)) => {
                bus.publish_local(event);
            }
            // This pod wrote it.
            Ok(None) => {}
            // A payload this build cannot read never will. Step over it — failing
            // here instead would wedge the relay on one poisoned row, since every
            // replay would hit it again.
            Err(e) => {
                let id: String = row.get("id");
                tracing::error!("event outbox row {id} cannot be decoded, skipping: {e}");
            }
        }
    }
    if delivered_through > from {
        advance_cursor(pool, relay_id, delivered_through).await?;
    }
    report_replay_gap(pool, relay_id, delivered_through, rows.len()).await;
    Ok((delivered_through, rows.len()))
}

async fn relay_notification(
    pool: &PgPool,
    bus: &EventBus,
    origin: &str,
    relay_id: &str,
    id: &str,
    delivered_through: &mut i64,
) -> anyhow::Result<()> {
    let Some(row) = fetch_row(pool, id).await? else {
        return Ok(());
    };
    let seq: i64 = row.get("seq");
    if seq <= *delivered_through {
        return Ok(());
    }
    if let Some(event) = decode_remote_event(&row, origin)? {
        bus.publish_local(event);
    }
    // Only after the event is safely on the local bus: a crash here replays the
    // row on restart (at-least-once, which the client handles), whereas moving
    // the cursor first would lose it outright.
    *delivered_through = seq;
    advance_cursor(pool, relay_id, seq).await
}

async fn fetch_row(pool: &PgPool, id: &str) -> anyhow::Result<Option<PgRow>> {
    Ok(
        sqlx::query(
            r#"SELECT "id", "seq", "payload", "origin" FROM "EventOutbox" WHERE "id" = $1"#,
        )
        .bind(id)
        .fetch_optional(pool)
        .await?,
    )
}

/// The event to republish locally, or `None` when this relay wrote it.
///
/// A pod must not re-deliver its own event: `publish` already put it on the
/// local bus, and re-entering the outbox through the replay would amplify.
fn decode_remote_event(row: &PgRow, origin: &str) -> anyhow::Result<Option<AppEvent>> {
    let event_origin: String = row.get("origin");
    if event_origin == origin {
        return Ok(None);
    }

    let payload: serde_json::Value = row.get("payload");
    Ok(Some(serde_json::from_value::<AppEvent>(payload)?))
}

async fn cursor_seq(pool: &PgPool, relay_id: &str) -> anyhow::Result<i64> {
    if let Some(seq) =
        sqlx::query_scalar::<_, i64>(r#"SELECT "seq" FROM "EventRelayCursor" WHERE "relayId" = $1"#)
            .bind(relay_id)
            .fetch_optional(pool)
            .await?
    {
        return Ok(seq);
    }

    // No row: this relay has never delivered anything, so it starts at the head
    // rather than at zero. Replaying the whole retention window here is the
    // "naive watermark" failure — a fresh subscriber would be handed events it
    // was never subscribed for, including a `GameCompleted` that navigates it to
    // the results page. Read after `LISTEN`, so nothing published from here on
    // can fall between the two.
    let head = head_seq(pool).await?;
    sqlx::query(
        r#"INSERT INTO "EventRelayCursor" ("relayId", "seq") VALUES ($1, $2)
           ON CONFLICT ("relayId") DO NOTHING"#,
    )
    .bind(relay_id)
    .bind(head)
    .execute(pool)
    .await?;
    // Re-read rather than trust `head`: another relay sharing this id may have
    // won the insert, and rewinding past its position would replay its rows.
    Ok(
        sqlx::query_scalar::<_, i64>(
            r#"SELECT "seq" FROM "EventRelayCursor" WHERE "relayId" = $1"#,
        )
        .bind(relay_id)
        .fetch_one(pool)
        .await?,
    )
}

async fn head_seq(pool: &PgPool) -> anyhow::Result<i64> {
    Ok(
        sqlx::query_scalar::<_, i64>(r#"SELECT "seq" FROM "EventOutboxSeq" WHERE "id" = 1"#)
            .fetch_one(pool)
            .await?,
    )
}

/// Move the durable cursor forward, never backward.
///
/// A notification can name an older row than one already processed, and
/// rewinding would replay it to every local subscriber.
async fn advance_cursor(pool: &PgPool, relay_id: &str, seq: i64) -> anyhow::Result<()> {
    sqlx::query(
        r#"UPDATE "EventRelayCursor" SET "seq" = $2, "updatedAt" = now()
           WHERE "relayId" = $1 AND "seq" < $2"#,
    )
    .bind(relay_id)
    .bind(seq)
    .execute(pool)
    .await?;
    Ok(())
}

/// Say when replay could not cover everything the cursor still owed.
///
/// The only thing that can create that gap is the reaper deleting rows the relay
/// had not reached yet, i.e. a pod that stayed away longer than
/// `OUTBOX_RETENTION`. The socket may still read "connected" — the failure to
/// avoid here is a silently missing `GamePresence` delta, not a visible
/// disconnect — so the affected clients have to reconcile over the websocket
/// (DEF-175). A pod whose cursor has simply not caught up yet is normal and
/// silent: every writer commits before it notifies, so the newest rows are
/// simply not committed yet.
async fn report_replay_gap(pool: &PgPool, relay_id: &str, delivered_through: i64, replayed: usize) {
    let head = match head_seq(pool).await {
        Ok(head) => head,
        Err(e) => {
            tracing::error!("could not read the outbox head: {e}");
            return;
        }
    };
    let oldest_still_here: i64 = match sqlx::query_scalar(
        r#"SELECT coalesce(min("seq"), $1) FROM "EventOutbox" WHERE "seq" > $1"#,
    )
    .bind(delivered_through)
    .fetch_one(pool)
    .await
    {
        Ok(oldest) => oldest,
        Err(e) => {
            tracing::error!("could not read the oldest undelivered outbox row: {e}");
            return;
        }
    };
    if oldest_still_here > delivered_through + 1 {
        tracing::warn!(
            relay_id,
            head,
            delivered_through,
            oldest_still_here,
            replayed,
            missing = oldest_still_here - delivered_through - 1,
            "event outbox replay left a gap: rows were pruned before this pod reconnected, \
             so the missed deltas are only recoverable over the websocket"
        );
    }
}

/// Deliver one outbox row to local subscribers, exactly as a live notification
/// would — minus the cursor. For tests that assert on delivery rather than on
/// the replay machinery.
pub async fn relay_event_for_test(pool: &PgPool, bus: &EventBus, origin: &str, id: &str) {
    if let Some(row) = fetch_row(pool, id).await.unwrap() {
        match decode_remote_event(&row, origin) {
            Ok(Some(event)) => {
                bus.publish_local(event);
            }
            Ok(None) => {}
            Err(e) => panic!("outbox row {id} should have decoded: {e}"),
        }
    }
}

/// What one replay did, so a test can assert on the events themselves.
///
/// The count alone is not enough: several tests share one database, so a replay
/// legitimately walks rows another test inserted, and asserting on the count
/// makes those tests fail spuriously.
#[derive(Debug, Clone, PartialEq)]
pub struct Replayed {
    /// Rows walked, including this pod's own and any skipped.
    pub rows: usize,
    /// The events actually handed to local subscribers, in `seq` order.
    pub delivered: Vec<AppEvent>,
}

/// One reconnect-and-replay cycle, for tests that drive the relay directly
/// instead of waiting on a reconnect.
pub async fn replay_for_test(
    pool: &PgPool,
    bus: &EventBus,
    origin: &str,
    relay_id: &str,
    rx: &mut broadcast::Receiver<AppEvent>,
) -> Replayed {
    let (_, rows) = replay(pool, bus, origin, relay_id).await.unwrap();
    let mut delivered = Vec::new();
    while let Ok(event) = rx.try_recv() {
        delivered.push(event);
    }
    Replayed { rows, delivered }
}

/// The relay loop, for tests that need the real listener (and so the real
/// `try_recv` disconnect handling) rather than a direct `replay` call.
///
/// The returned handle aborts the loop, which is how a test stops it; dropping
/// it does not, because in production it runs for the process lifetime.
pub fn spawn_relay_for_test(
    pool: PgPool,
    bus: EventBus,
    origin: &str,
    relay_id: &str,
) -> tokio::task::JoinHandle<()> {
    spawn_listener(pool, bus, origin.to_string(), relay_id.to_string())
}

/// Collect at most `limit` events, waiting for the first and then draining what
/// is already queued.
pub async fn collect_for_test(
    rx: &mut broadcast::Receiver<AppEvent>,
    limit: usize,
    within: Duration,
) -> Vec<AppEvent> {
    let mut out = Vec::new();
    let deadline = tokio::time::Instant::now() + within;
    while out.len() < limit {
        match tokio::time::timeout_at(deadline, rx.recv()).await {
            Ok(Ok(event)) => out.push(event),
            Ok(Err(_)) => break,
            Err(_) => break,
        }
    }
    out
}
