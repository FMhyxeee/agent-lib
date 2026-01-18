use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::session::TurnContext;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Op {
    StartTurn { prompt: String, context: TurnContext },
    UserInput { content: String },
    ApprovalResponse { request_id: String, approved: bool },
    Interrupt,
    Handoff { target_agent: String, context: Value },
}
