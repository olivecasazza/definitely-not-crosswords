mod common;

use crossword_db::AppEvent;
use crossword_events::EventBus;
use crossword_server::pg_events;
use sqlx::Row;

#[tokio::test]
#[ignore = "requires live DB; run with --ignored and DATABASE_URL"]
async fn outbox_event_reaches_separate_bus() {
    let pool = common::pool().await;
    let sender_origin = "outbox-test-a";
    let receiver_origin = "outbox-test-b";
    let event = AppEvent::GamePresence {
        active_game_id: "ag-outbox".into(),
        user_id: "u-1".into(),
        name: "Ada".into(),
        number: Some(3),
        direction: Some("across".into()),
    };
    let id = uuid::Uuid::new_v4().to_string();
    sqlx::query(r#"INSERT INTO "EventOutbox" ("id", "payload", "origin") VALUES ($1, $2, $3)"#)
        .bind(&id)
        .bind(serde_json::to_value(&event).unwrap())
        .bind(sender_origin)
        .execute(&pool)
        .await
        .unwrap();

    let receiver = EventBus::default();
    let mut rx = receiver.subscribe();
    pg_events::relay_event_for_test(&pool, &receiver, receiver_origin, &id).await;

    assert_eq!(rx.recv().await.unwrap(), event);
}

#[tokio::test]
#[ignore = "requires live DB; run with --ignored and DATABASE_URL"]
async fn outbox_drops_events_from_same_origin() {
    let pool = common::pool().await;
    let origin = "outbox-test-same-origin";
    let event = AppEvent::GameCompleted {
        active_game_id: "ag-outbox".into(),
        completed_game_id: "cg-outbox".into(),
    };
    let id = uuid::Uuid::new_v4().to_string();
    sqlx::query(r#"INSERT INTO "EventOutbox" ("id", "payload", "origin") VALUES ($1, $2, $3)"#)
        .bind(&id)
        .bind(serde_json::to_value(&event).unwrap())
        .bind(origin)
        .execute(&pool)
        .await
        .unwrap();

    let receiver = EventBus::default();
    let mut rx = receiver.subscribe();
    pg_events::relay_event_for_test(&pool, &receiver, origin, &id).await;

    assert!(rx.try_recv().is_err());
}

#[tokio::test]
#[ignore = "requires live DB; run with --ignored and DATABASE_URL"]
async fn outbox_prune_deletes_old_rows() {
    let pool = common::pool().await;
    let id = uuid::Uuid::new_v4().to_string();
    sqlx::query(
        r#"INSERT INTO "EventOutbox" ("id", "payload", "origin", "createdAt") VALUES ($1, $2, $3, now() - interval '20 minutes')"#,
    )
    .bind(&id)
    .bind(serde_json::to_value(AppEvent::GameCompleted {
        active_game_id: "ag-outbox".into(),
        completed_game_id: "cg-outbox".into(),
    }).unwrap())
    .bind("outbox-test-prune")
    .execute(&pool)
    .await
    .unwrap();

    pg_events::prune_outbox(&pool).await;

    let row = sqlx::query(r#"SELECT count(*) AS count FROM "EventOutbox" WHERE "id" = $1"#)
        .bind(&id)
        .fetch_one(&pool)
        .await
        .unwrap();
    let count: i64 = row.get("count");
    assert_eq!(count, 0);
}
