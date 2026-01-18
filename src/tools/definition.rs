use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::error::{AgentError, AgentResult};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDef {
    pub name: String,
    pub description: String,
    pub schema: Value,
}

#[derive(Debug, Clone)]
pub struct ToolContext {
    pub cwd: Option<String>,
    pub sandbox_root: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResult {
    pub output: Value,
}

impl ToolResult {
    pub fn text(text: impl Into<String>) -> Self {
        Self {
            output: Value::String(text.into()),
        }
    }
}

#[async_trait]
pub trait Tool: Send + Sync {
    fn definition(&self) -> ToolDef;

    async fn execute(&self, args: Value, ctx: &ToolContext) -> AgentResult<ToolResult>;
}

pub fn not_implemented_tool(name: &str) -> AgentError {
    AgentError::NotImplemented(format!("tool {name} not implemented"))
}
