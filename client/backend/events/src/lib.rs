//! Typed pub-sub bus replacing the process-local Node EventEmitter.

use crossword_db::AppEvent;
use std::sync::Arc;
use tokio::sync::broadcast;

#[derive(Clone)]
pub struct EventBus {
    tx: broadcast::Sender<AppEvent>,
    outbox: Option<Arc<dyn Fn(AppEvent) + Send + Sync>>,
}

impl Default for EventBus {
    fn default() -> Self {
        Self::new(256)
    }
}

impl EventBus {
    pub fn new(capacity: usize) -> Self {
        let (tx, _) = broadcast::channel(capacity);
        Self { tx, outbox: None }
    }

    pub fn with_outbox(capacity: usize, outbox: impl Fn(AppEvent) + Send + Sync + 'static) -> Self {
        let (tx, _) = broadcast::channel(capacity);
        Self {
            tx,
            outbox: Some(Arc::new(outbox)),
        }
    }

    pub fn publish(&self, event: AppEvent) -> usize {
        let receivers = self.publish_local(event.clone());
        if let Some(outbox) = &self.outbox {
            outbox(event);
        }
        receivers
    }

    pub fn publish_local(&self, event: AppEvent) -> usize {
        self.tx.send(event).unwrap_or(0)
    }

    pub fn subscribe(&self) -> broadcast::Receiver<AppEvent> {
        self.tx.subscribe()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn subscribers_receive_events() {
        let bus = EventBus::new(8);
        let mut rx = bus.subscribe();
        assert_eq!(
            bus.publish(AppEvent::GameCompleted {
                active_game_id: "ag-1".into(),
                completed_game_id: "cg-1".into(),
            }),
            1
        );
        assert!(matches!(
            rx.recv().await.unwrap(),
            AppEvent::GameCompleted { completed_game_id, .. } if completed_game_id == "cg-1"
        ));
    }

    #[tokio::test]
    async fn local_publish_does_not_forward_to_outbox() {
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let bus = EventBus::with_outbox(8, move |event| {
            tx.send(event).unwrap();
        });

        bus.publish_local(AppEvent::GameCompleted {
            active_game_id: "ag-1".into(),
            completed_game_id: "cg-1".into(),
        });

        assert!(rx.try_recv().is_err());
    }
}
