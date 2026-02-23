use std::path::PathBuf;
use std::process::Stdio;
use std::time::Duration;

use async_trait::async_trait;
use serde_json::Value;
use serde_json::json;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;
use tokio::time::timeout;

use crate::error::{AgentError, AgentResult};
use crate::tools::{Tool, ToolContext, ToolDef, ToolResult};

#[derive(Debug, Default)]
pub struct CodeExecTool;

const DEFAULT_EXEC_TIMEOUT_SECS: u64 = 30;

impl CodeExecTool {
    pub fn new() -> Self {
        Self
    }

    async fn execute_with_timeout(
        &self,
        args: Value,
        ctx: &ToolContext,
        timeout_secs: u64,
    ) -> AgentResult<ToolResult> {
        let language = args
            .get("language")
            .and_then(|value| value.as_str())
            .ok_or_else(|| AgentError::Tool("missing language".to_string()))?;
        let code = args
            .get("code")
            .and_then(|value| value.as_str())
            .ok_or_else(|| AgentError::Tool("missing code".to_string()))?;
        let cwd = normalized_cwd(ctx.cwd.as_deref());

        let output = match language.to_ascii_lowercase().as_str() {
            "python" => run_with_stdin("python", &["-"], code, cwd.as_ref(), timeout_secs).await?,
            "bash" | "sh" if !cfg!(windows) => {
                let mut command = Command::new("sh");
                command
                    .arg("-c")
                    .arg(code)
                    .stdout(Stdio::piped())
                    .stderr(Stdio::piped());
                configure_current_dir(&mut command, cwd.as_ref());
                run_command_with_timeout(command, timeout_secs).await?
            }
            "powershell" if cfg!(windows) => {
                let mut command = Command::new("powershell");
                command
                    .arg("-Command")
                    .arg(code)
                    .stdout(Stdio::piped())
                    .stderr(Stdio::piped());
                configure_current_dir(&mut command, cwd.as_ref());
                run_command_with_timeout(command, timeout_secs).await?
            }
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

#[async_trait]
impl Tool for CodeExecTool {
    fn definition(&self) -> ToolDef {
        ToolDef {
            name: "code_exec".to_string(),
            description: "Execute code snippets with timeout and cwd constraints".to_string(),
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

    async fn execute(&self, args: Value, ctx: &ToolContext) -> AgentResult<ToolResult> {
        self.execute_with_timeout(args, ctx, DEFAULT_EXEC_TIMEOUT_SECS)
            .await
    }
}

fn normalized_cwd(cwd: Option<&str>) -> Option<PathBuf> {
    let cwd = cwd?.trim();
    if cwd.is_empty() {
        return None;
    }
    Some(PathBuf::from(cwd))
}

fn configure_current_dir(command: &mut Command, cwd: Option<&PathBuf>) {
    if let Some(path) = cwd {
        command.current_dir(path);
    }
}

async fn run_command_with_timeout(
    mut command: Command,
    timeout_secs: u64,
) -> AgentResult<std::process::Output> {
    match timeout(Duration::from_secs(timeout_secs), command.output()).await {
        Ok(Ok(output)) => Ok(output),
        Ok(Err(err)) => Err(AgentError::Tool(format!("exec failed: {err}"))),
        Err(_) => Err(AgentError::Tool(format!(
            "execution timed out after {timeout_secs} seconds"
        ))),
    }
}

async fn run_with_stdin(
    program: &str,
    args: &[&str],
    input: &str,
    cwd: Option<&PathBuf>,
    timeout_secs: u64,
) -> AgentResult<std::process::Output> {
    let mut command = Command::new(program);
    command
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    configure_current_dir(&mut command, cwd);

    let mut child = command
        .spawn()
        .map_err(|err| AgentError::Tool(format!("spawn failed: {err}")))?;

    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(input.as_bytes())
            .await
            .map_err(|err| AgentError::Tool(format!("stdin write failed: {err}")))?;
    }

    match timeout(Duration::from_secs(timeout_secs), child.wait_with_output()).await {
        Ok(Ok(output)) => Ok(output),
        Ok(Err(err)) => Err(AgentError::Tool(format!("exec failed: {err}"))),
        Err(_) => Err(AgentError::Tool(format!(
            "execution timed out after {timeout_secs} seconds"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    fn default_ctx() -> ToolContext {
        ToolContext {
            cwd: None,
            sandbox_root: None,
        }
    }

    #[tokio::test]
    async fn execute_fails_when_language_missing() {
        let tool = CodeExecTool::new();
        let result = tool
            .execute(json!({"code": "print(1)"}), &default_ctx())
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn execute_fails_when_code_missing() {
        let tool = CodeExecTool::new();
        let result = tool
            .execute(json!({"language": "python"}), &default_ctx())
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn execute_fails_for_unsupported_language() {
        let tool = CodeExecTool::new();
        let err = tool
            .execute(
                json!({
                    "language": "ruby",
                    "code": "puts 1"
                }),
                &default_ctx(),
            )
            .await
            .expect_err("unsupported language should fail");
        assert!(err.to_string().contains("unsupported language"));
    }

    #[tokio::test]
    async fn execute_times_out_for_long_running_script() {
        let tool = CodeExecTool::new();
        let args = if cfg!(windows) {
            json!({
                "language": "powershell",
                "code": "Start-Sleep -Seconds 2"
            })
        } else {
            json!({
                "language": "sh",
                "code": "sleep 2"
            })
        };

        let err = tool
            .execute_with_timeout(args, &default_ctx(), 1)
            .await
            .expect_err("long-running script should time out");
        assert!(err.to_string().contains("timed out"));
    }

    #[tokio::test]
    async fn execute_respects_tool_context_cwd() {
        let temp_dir = std::env::temp_dir().join(format!("code-exec-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&temp_dir).expect("temp dir should be created");
        let tool = CodeExecTool::new();

        let ctx = ToolContext {
            cwd: Some(temp_dir.to_string_lossy().to_string()),
            sandbox_root: None,
        };
        let args = if cfg!(windows) {
            json!({
                "language": "powershell",
                "code": "(Get-Location).Path"
            })
        } else {
            json!({
                "language": "sh",
                "code": "pwd"
            })
        };

        let result = tool
            .execute_with_timeout(args, &ctx, 5)
            .await
            .expect("code exec should succeed");

        let stdout = result
            .output
            .get("stdout")
            .and_then(|value| value.as_str())
            .unwrap_or_default();
        assert!(stdout.contains(temp_dir.to_string_lossy().as_ref()));

        let _ = std::fs::remove_dir_all(&temp_dir);
    }
}
