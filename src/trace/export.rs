use serde::{Deserialize, Serialize};

use crate::trace::TraceEvent;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceExport {
    pub events: Vec<TraceEvent>,
}
