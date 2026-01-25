use std::process::Stdio;

use async_trait::async_trait;
use serde_json::Value;
use serde_json::json;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;

use crate::error::{AgentError, AgentResult};
use crate::tools::{Tool, ToolContext, ToolDef, ToolResult};

#[derive(Debug, Default)]
pub struct CodeExecTool;

impl CodeExecTool {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Tool for CodeExecTool {
    fn definition(&self) -> ToolDef {
        ToolDef {
            name: "code_exec".to_string(),
            description: "Execute code snippets in a sandbox".to_string(),
            schema: json!({
                "type": "object",
                "properties": {
                    "language": { "type": "string" },
                    "code": { "type": "string" }
                },
                "required": ["language", "code"]
            }),
        }
    }

    async fn execute(&self, _args: Value, _ctx: &ToolContext) -> AgentResult<ToolResult> {
        let language = _args
            .get("language")
            .and_then(|value| value.as_str())
            .ok_or_else(|| AgentError::Tool("missing language".to_string()))?;
        let code = _args
            .get("code")
            .and_then(|value| value.as_str())
            .ok_or_else(|| AgentError::Tool("missing code".to_string()))?;

        let output = match language {
            "python" => run_with_stdin("python", &["-"], code).await?,
            "bash" | "sh" if !cfg!(windows) => Command::new("sh")
                .arg("-c")
                .arg(code)
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .output()
                .await
                .map_err(|err| AgentError::Tool(format!("exec failed: {err}")))?,
            "powershell" if cfg!(windows) => Command::new("powershell")
                .arg("-Command")
                .arg(code)
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .output()
                .await
                .map_err(|err| AgentError::Tool(format!("exec failed: {err}")))?,
            other => return Err(AgentError::Tool(format!("unsupported language: {other}"))),
        };

        Ok(ToolResult {
            output: json!({
                "stdout": String::from_utf8_lossy(&output.stdout),
                "stderr": String::from_utf8_lossy(&output.stderr),
                "status": output.status.code(),
            }),
        })
    }
}

async fn run_with_stdin(
    program: &str,
    args: &[&str],
    input: &str,
) -> AgentResult<std::process::Output> {
    let mut child = Command::new(program)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|err| AgentError::Tool(format!("spawn failed: {err}")))?;

    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(input.as_bytes())
            .await
            .map_err(|err| AgentError::Tool(format!("stdin write failed: {err}")))?;
    }

    child
        .wait_with_output()
        .await
        .map_err(|err| AgentError::Tool(format!("exec failed: {err}")))
}
