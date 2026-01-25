use std::path::{Path, PathBuf};

use async_trait::async_trait;
use serde_json::Value;
use serde_json::json;
use tokio::fs;

use crate::error::{AgentError, AgentResult};
use crate::tools::{Tool, ToolContext, ToolDef, ToolResult};

#[derive(Debug, Default)]
pub struct FileSystemTool;

impl FileSystemTool {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Tool for FileSystemTool {
    fn definition(&self) -> ToolDef {
        ToolDef {
            name: "filesystem".to_string(),
            description: "Read or write files".to_string(),
            schema: json!({
                "type": "object",
                "properties": {
                    "operation": { "type": "string", "enum": ["read", "write", "delete"] },
                    "path": { "type": "string" },
                    "content": { "type": "string" }
                },
                "required": ["operation", "path"]
            }),
        }
    }

    async fn execute(&self, _args: Value, _ctx: &ToolContext) -> AgentResult<ToolResult> {
        let operation = _args
            .get("operation")
            .and_then(|value| value.as_str())
            .ok_or_else(|| AgentError::Tool("missing operation".to_string()))?;
        let path = _args
            .get("path")
            .and_then(|value| value.as_str())
            .ok_or_else(|| AgentError::Tool("missing path".to_string()))?;

        let resolved = resolve_path(_ctx.cwd.as_deref(), _ctx.sandbox_root.as_deref(), path)?;

        match operation {
            "read" => {
                let contents = fs::read_to_string(&resolved)
                    .await
                    .map_err(|err| AgentError::Tool(format!("read failed: {err}")))?;
                Ok(ToolResult::text(contents))
            }
            "write" => {
                let content = _args
                    .get("content")
                    .and_then(|value| value.as_str())
                    .ok_or_else(|| AgentError::Tool("missing content".to_string()))?;
                fs::write(&resolved, content)
                    .await
                    .map_err(|err| AgentError::Tool(format!("write failed: {err}")))?;
                Ok(ToolResult::text("ok"))
            }
            "delete" => {
                fs::remove_file(&resolved)
                    .await
                    .map_err(|err| AgentError::Tool(format!("delete failed: {err}")))?;
                Ok(ToolResult::text("ok"))
            }
            other => Err(AgentError::Tool(format!("unsupported operation: {other}"))),
        }
    }
}

fn resolve_path(cwd: Option<&str>, sandbox_root: Option<&str>, path: &str) -> AgentResult<PathBuf> {
    let base = cwd.map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."));
    let target = Path::new(path);
    let resolved = if target.is_absolute() {
        target.to_path_buf()
    } else {
        base.join(target)
    };

    if let Some(root) = sandbox_root {
        let root_path = normalize_path(&PathBuf::from(root));
        if resolved
            .components()
            .any(|c| matches!(c, std::path::Component::ParentDir))
        {
            return Err(AgentError::Tool(
                "parent dir segments not allowed in sandbox".to_string(),
            ));
        }
        let resolved_norm = normalize_path(&resolved);
        if !resolved_norm.starts_with(&root_path) {
            return Err(AgentError::Tool("path escapes sandbox".to_string()));
        }
    }
    Ok(resolved)
}

fn normalize_path(path: &Path) -> PathBuf {
    use std::path::Component;
    let mut result = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Prefix(prefix) => result.push(prefix.as_os_str()),
            Component::RootDir => result.push(component.as_os_str()),
            Component::CurDir => {}
            Component::ParentDir => {
                result.pop();
            }
            Component::Normal(part) => result.push(part),
        }
    }
    result
}
