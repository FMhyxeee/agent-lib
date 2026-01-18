use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};

use crate::protocol::Event;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceEvent {
    pub event: Event,
}

#[derive(Default, Clone)]
pub struct TraceRecorder {
    events: Arc<Mutex<Vec<TraceEvent>>>,
}

impl TraceRecorder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn record(&self, event: Event) {
        let mut guard = self
            .events
            .lock()
            .expect("trace recorder poisoned");
        guard.push(TraceEvent { event });
    }

    pub fn events(&self) -> Vec<TraceEvent> {
        self.events
            .lock()
            .expect("trace recorder poisoned")
            .clone()
    }
}
