//! Library target so `tests/` integration tests can reach the tRPC dispatch
//! (`routers::generator::try_handle`) and `Ctx` without going through the
//! Axum router. The binary keeps its `main.rs`; both share these modules.

pub mod assets;
pub mod auth_routes;
pub mod checkout;
pub mod ctx;
pub mod mailer;
pub mod origin;
pub mod routers;
pub mod scoring;
pub mod seo;
pub mod spa;
pub mod state;
pub mod webhook;
pub mod wire;

use crossword_db::AppEvent;
use serde_json::{json, Value};
use tokio::sync::{broadcast::error::RecvError, mpsc::UnboundedSender};

/// Forward bus events to one tRPC-over-WS subscription.
///
/// Lives here rather than inline in `main.rs` so it can be tested without a
/// socket, and so the `Lagged` rule is stated once. `broadcast::Receiver`
/// stays usable after `RecvError::Lagged` — it reports *dropped* frames, not a
/// disconnect — so the loop must continue past it. Treating it as terminal
/// silently killed every live subscription the moment the bus outran the task:
/// the server had already answered `{"type":"started"}`, nothing ever sent
/// `{"type":"stopped"}`, and the client's remote letters and presence rings
/// just stopped arriving with no error anywhere.
pub async fn forward_event_bus<M>(
    mut rx: tokio::sync::broadcast::Receiver<AppEvent>,
    out: UnboundedSender<String>,
    id: i64,
    mut data_for: M,
) where
    M: FnMut(&AppEvent) -> Option<Value>,
{
    loop {
        match rx.recv().await {
            Ok(ev) => {
                if let Some(data) = data_for(&ev) {
                    let _ = out.send(
                        json!({ "id": id, "result": { "type": "data", "data": data } }).to_string(),
                    );
                }
            }
            // Behind the ring buffer: `skipped` frames are gone, the next one
            // is the oldest still retained. The subscriber is still connected.
            Err(RecvError::Lagged(skipped)) => {
                tracing::warn!(
                    subscription = id,
                    skipped,
                    "event bus lagged; subscription continues"
                );
            }
            // Every sender is gone, so no further event can ever arrive.
            Err(RecvError::Closed) => break,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossword_events::EventBus;

    fn presence(game: &str) -> AppEvent {
        AppEvent::GamePresence {
            active_game_id: game.into(),
            user_id: "u-partner".into(),
            name: "Partner".into(),
            number: Some(3),
            direction: Some("ACROSS".into()),
        }
    }

    /// A burst deeper than the ring buffer must not disconnect the
    /// subscription. Before this, the forwarder exited on `Lagged`; because the
    /// server had already sent `{"type":"started"}` the client was never told,
    /// so a lagging board silently stopped receiving remote letters/presence.
    #[tokio::test]
    async fn lagged_forwarder_keeps_forwarding() {
        let bus = EventBus::new(2);
        let rx = bus.subscribe();
        let (tx, mut out) = tokio::sync::mpsc::unbounded_channel::<String>();

        // Overflow the 2-slot ring before anything reads from it.
        for i in 0..5 {
            bus.publish(AppEvent::GameCompleted {
                active_game_id: format!("ag-{i}"),
                completed_game_id: format!("cg-{i}"),
            });
        }

        let handle = tokio::spawn(forward_event_bus(rx, tx, 7, |ev| match ev {
            AppEvent::GamePresence {
                active_game_id,
                user_id,
                ..
            } => Some(json!({ "activeGameId": active_game_id, "userId": user_id })),
            _ => None,
        }));

        bus.publish(presence("ag-live"));
        let frame = out.recv().await.expect("a data frame after the lag");
        assert!(frame.contains("\"type\":\"data\""), "got {frame}");
        assert!(frame.contains("ag-live"), "got {frame}");
        assert!(frame.contains("\"id\":7"), "got {frame}");
        assert!(
            !handle.is_finished(),
            "forwarder exited on RecvError::Lagged"
        );
    }
}
