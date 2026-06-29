//! Typed pub-sub bus replacing the process-local Node EventEmitter.

use crossword_db::AppEvent;
use tokio::sync::broadcast;

#[derive(Debug, Clone)]
pub struct EventBus {
    tx: broadcast::Sender<AppEvent>,
}

impl Default for EventBus {
    fn default() -> Self {
        Self::new(256)
    }
}

impl EventBus {
    pub fn new(capacity: usize) -> Self {
        let (tx, _) = broadcast::channel(capacity);
        Self { tx }
    }

    pub fn publish(&self, event: AppEvent) -> usize {
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
            bus.publish(AppEvent::Message {
                text: "hello".into()
            }),
            1
        );
        assert!(matches!(rx.recv().await.unwrap(), AppEvent::Message { text } if text == "hello"));
    }
}
