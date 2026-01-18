use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ApprovalDecision {
    Approve,
    Deny { reason: String },
    Ask,
}

#[async_trait]
pub trait ApprovalHook: Send + Sync {
    async fn check(&self, tool: &str, args: &Value) -> ApprovalDecision;
}
