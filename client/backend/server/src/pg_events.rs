use crossword_db::AppEvent;
use crossword_events::EventBus;
use sqlx::{postgres::PgListener, PgPool, Row};
use std::time::Duration;
use tokio::sync::mpsc;

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
    spawn_listener(pool, bus.clone(), origin);
    bus
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
    sqlx::query(r#"INSERT INTO "EventOutbox" ("id", "payload", "origin") VALUES ($1, $2, $3)"#)
        .bind(&id)
        .bind(payload)
        .bind(origin)
        .execute(pool)
        .await?;
    sqlx::query("SELECT pg_notify($1, $2)")
        .bind(CHANNEL)
        .bind(&id)
        .execute(pool)
        .await?;
    Ok(())
}

fn spawn_listener(pool: PgPool, bus: EventBus, origin: String) {
    tokio::spawn(async move {
        loop {
            match listen_once(&pool, &bus, &origin).await {
                Ok(()) => tracing::warn!("event outbox listener stopped; reconnecting"),
                Err(e) => tracing::error!("event outbox listener failed: {e}"),
            }
            tokio::time::sleep(RETRY_DELAY).await;
        }
    });
}

async fn listen_once(pool: &PgPool, bus: &EventBus, origin: &str) -> anyhow::Result<()> {
    let mut listener = PgListener::connect_with(pool).await?;
    listener.listen(CHANNEL).await?;
    loop {
        let notification = listener.recv().await?;
        relay_event(pool, bus, origin, notification.payload()).await;
    }
}

async fn relay_event(pool: &PgPool, bus: &EventBus, origin: &str, id: &str) {
    match load_remote_event(pool, origin, id).await {
        Ok(Some(event)) => {
            bus.publish_local(event);
        }
        Ok(None) => {}
        Err(e) => tracing::error!("event outbox relay failed for {id}: {e}"),
    }
}

async fn load_remote_event(
    pool: &PgPool,
    origin: &str,
    id: &str,
) -> anyhow::Result<Option<AppEvent>> {
    let Some(row) = sqlx::query(r#"SELECT "payload", "origin" FROM "EventOutbox" WHERE "id" = $1"#)
        .bind(id)
        .fetch_optional(pool)
        .await?
    else {
        return Ok(None);
    };

    let event_origin: String = row.get("origin");
    if event_origin == origin {
        return Ok(None);
    }

    let payload: serde_json::Value = row.get("payload");
    Ok(Some(serde_json::from_value::<AppEvent>(payload)?))
}

pub async fn relay_event_for_test(pool: &PgPool, bus: &EventBus, origin: &str, id: &str) {
    if let Some(event) = load_remote_event(pool, origin, id).await.unwrap() {
        bus.publish_local(event);
    }
}
