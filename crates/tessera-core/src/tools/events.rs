//! Server-sent events (`GET /api/events`): progress of long tool jobs, so the
//! UI can show progress bars in the desktop window and in browsers alike.

use serde::Serialize;
use tokio::sync::broadcast;

/// Progress notification (formerly the `tessera://progress` Tauri event).
#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Progress {
    pub task: &'static str,
    pub done: usize,
    pub total: usize,
    pub current: Option<String>,
}

/// Fan-out of events to every connected UI.
pub struct EventHub {
    tx: broadcast::Sender<String>,
}

impl Default for EventHub {
    fn default() -> Self {
        Self::new()
    }
}

impl EventHub {
    pub fn new() -> EventHub {
        let (tx, _) = broadcast::channel(1024);
        EventHub { tx }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<String> {
        self.tx.subscribe()
    }

    /// Send `{"type": kind, "data": payload}` to every subscriber.
    pub fn send<T: Serialize>(&self, kind: &str, payload: &T) {
        if self.tx.receiver_count() == 0 {
            return;
        }
        if let Ok(data) = serde_json::to_value(payload) {
            let msg = serde_json::json!({ "type": kind, "data": data }).to_string();
            let _ = self.tx.send(msg);
        }
    }

    pub fn progress(&self, p: Progress) {
        self.send("progress", &p);
    }
}
