use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio_stream::wrappers::ReceiverStream;

use crate::error::AgentError;
use crate::model::TokenUsage;
use crate::tools::ToolResult;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Event {
    TurnStarted { turn_id: String },
    ModelStreaming { chunk: String },
    ModelComplete { content: String, usage: TokenUsage },
    ToolCallRequested { tool: String, args: Value },
    ToolCallResult { tool: String, result: ToolResult },
    ApprovalRequired { request_id: String, tool: String, args: Value },
    HandoffInitiated { from: String, to: String },
    TurnComplete { result: Value },
    Error { error: AgentError },
}

pub type EventStream = ReceiverStream<Event>;
