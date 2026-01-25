use async_trait::async_trait;
use serde_json::Value;
use serde_json::json;

use std::process::Stdio;

use tokio::process::Command;

use crate::error::{AgentError, AgentResult};
use crate::tools::{Tool, ToolContext, ToolDef, ToolResult};

#[derive(Debug, Default)]
pub struct ShellTool;

impl ShellTool {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Tool for ShellTool {
    fn definition(&self) -> ToolDef {
        ToolDef {
            name: "shell".to_string(),
            description: "Execute a shell command".to_string(),
            schema: json!({
                "type": "object",
                "properties": {
                    "command": { "type": "string" }
                },
                "required": ["command"]
            }),
        }
    }

    async fn execute(&self, _args: Value, _ctx: &ToolContext) -> AgentResult<ToolResult> {
        let command = _args
            .get("command")
            .and_then(|value| value.as_str())
            .ok_or_else(|| AgentError::Tool("missing command".to_string()))?;

        let output = if cfg!(windows) {
            Command::new("powershell")
                .arg("-Command")
                .arg(command)
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .output()
                .await
        } else {
            Command::new("sh")
                .arg("-c")
                .arg(command)
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .output()
                .await
        }
        .map_err(|err| AgentError::Tool(format!("shell exec failed: {err}")))?;

        Ok(ToolResult {
            output: json!({
                "stdout": String::from_utf8_lossy(&output.stdout),
                "stderr": String::from_utf8_lossy(&output.stderr),
                "status": output.status.code(),
            }),
        })
    }
}
