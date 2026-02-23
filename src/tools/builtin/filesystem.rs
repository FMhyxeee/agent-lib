use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::Value;
use serde_json::json;
use sha2::{Digest, Sha256};
use tokio::fs;
use uuid::Uuid;

use crate::error::{AgentError, AgentResult};
use crate::tools::{Tool, ToolContext, ToolDef, ToolResult, needs_truncation, truncate_output};

const DEFAULT_MAX_DIFF_CHARS: usize = 8 * 1024;
const MAX_ALLOWED_DIFF_CHARS: usize = 64 * 1024;
const MAX_EDITS_PER_REQUEST: usize = 32;

#[derive(Debug, Default)]
pub struct FileSystemTool;

#[derive(Debug, Deserialize)]
struct EditInstruction {
    find: String,
    #[serde(default)]
    replace: String,
    #[serde(default)]
    replace_all: bool,
    #[serde(default)]
    expected_occurrences: Option<usize>,
}

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
            description: "Read, write, edit, or delete files".to_string(),
            schema: json!({
                "type": "object",
                "properties": {
                    "operation": { "type": "string", "enum": ["read", "write", "delete", "edit"] },
                    "path": { "type": "string" },
                    "content": { "type": "string" },
                    "edits": {
                        "type": "array",
                        "maxItems": MAX_EDITS_PER_REQUEST,
                        "items": {
                            "type": "object",
                            "properties": {
                                "find": { "type": "string" },
                                "replace": { "type": "string" },
                                "replace_all": { "type": "boolean" },
                                "expected_occurrences": { "type": "integer", "minimum": 0 }
                            },
                            "required": ["find"]
                        }
                    },
                    "dry_run": { "type": "boolean" },
                    "return_diff": { "type": "boolean" },
                    "max_diff_chars": { "type": "integer", "minimum": 64 }
                },
                "required": ["operation", "path"]
            }),
        }
    }

    async fn execute(&self, args: Value, ctx: &ToolContext) -> AgentResult<ToolResult> {
        let operation = args
            .get("operation")
            .and_then(|value| value.as_str())
            .ok_or_else(|| AgentError::Tool("missing operation".to_string()))?;
        let path = args
            .get("path")
            .and_then(|value| value.as_str())
            .ok_or_else(|| AgentError::Tool("missing path".to_string()))?;

        let resolved = resolve_path(ctx.cwd.as_deref(), ctx.sandbox_root.as_deref(), path)?;

        match operation {
            "read" => {
                let contents = fs::read_to_string(&resolved)
                    .await
                    .map_err(|err| AgentError::Tool(format!("read failed: {err}")))?;
                Ok(ToolResult::text(contents))
            }
            "write" => {
                let content = args
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
            "edit" => execute_edit(&args, path, &resolved).await,
            other => Err(AgentError::Tool(format!("unsupported operation: {other}"))),
        }
    }
}

async fn execute_edit(
    args: &Value,
    original_path: &str,
    resolved: &Path,
) -> AgentResult<ToolResult> {
    let edits_json = args
        .get("edits")
        .and_then(|value| value.as_array())
        .ok_or_else(|| AgentError::Tool("missing edits for operation=edit".to_string()))?;

    if edits_json.is_empty() {
        return Err(AgentError::Tool("edits must not be empty".to_string()));
    }

    if edits_json.len() > MAX_EDITS_PER_REQUEST {
        return Err(AgentError::Tool(format!(
            "too many edits: {} (max {})",
            edits_json.len(),
            MAX_EDITS_PER_REQUEST
        )));
    }

    let mut edits = Vec::with_capacity(edits_json.len());
    for edit in edits_json {
        let instruction: EditInstruction = serde_json::from_value(edit.clone())
            .map_err(|err| AgentError::Tool(format!("invalid edit item: {err}")))?;
        if instruction.find.is_empty() {
            return Err(AgentError::Tool("edit.find must not be empty".to_string()));
        }
        edits.push(instruction);
    }

    let dry_run = args
        .get("dry_run")
        .and_then(|value| value.as_bool())
        .unwrap_or(false);
    let return_diff = args
        .get("return_diff")
        .and_then(|value| value.as_bool())
        .unwrap_or(false);
    let max_diff_chars = args
        .get("max_diff_chars")
        .and_then(|value| value.as_u64())
        .map(|value| value as usize)
        .unwrap_or(DEFAULT_MAX_DIFF_CHARS)
        .min(MAX_ALLOWED_DIFF_CHARS)
        .max(64);

    let before = fs::read_to_string(resolved)
        .await
        .map_err(|err| AgentError::Tool(format!("edit read failed: {err}")))?;

    let before_hash = hash_content(&before);
    let bytes_before = before.as_bytes().len();

    let mut edited = before.clone();
    let mut replacements = 0usize;
    let mut applied = Vec::with_capacity(edits.len());

    for (index, instruction) in edits.iter().enumerate() {
        let occurrences = edited.matches(&instruction.find).count();

        if occurrences == 0 {
            return Err(AgentError::Tool(format!(
                "edit[{index}] find target not found"
            )));
        }

        if let Some(expected) = instruction.expected_occurrences {
            if occurrences != expected {
                return Err(AgentError::Tool(format!(
                    "edit[{index}] occurrences mismatch: expected {expected}, got {occurrences}"
                )));
            }
        }

        let replaced_count = if instruction.replace_all {
            edited = edited.replace(&instruction.find, &instruction.replace);
            occurrences
        } else {
            edited = edited.replacen(&instruction.find, &instruction.replace, 1);
            1
        };

        replacements += replaced_count;
        applied.push(json!({
            "index": index,
            "replacements": replaced_count,
        }));
    }

    let bytes_after = edited.as_bytes().len();
    let after_hash = hash_content(&edited);
    let changed_line_count = count_changed_lines(&before, &edited);

    if !dry_run && before != edited {
        atomic_write(resolved, &edited).await?;
    }

    let mut output = json!({
        "operation": "edit",
        "path": original_path,
        "dry_run": dry_run,
        "replacements": replacements,
        "changed_line_count": changed_line_count,
        "before_hash": before_hash,
        "after_hash": after_hash,
        "bytes_before": bytes_before,
        "bytes_after": bytes_after,
        "applied_edits": applied,
    });

    if return_diff {
        let raw_diff = build_line_diff(&before, &edited);
        let truncated = needs_truncation(&raw_diff, max_diff_chars);
        let diff = if truncated {
            truncate_output(&raw_diff, max_diff_chars)
        } else {
            raw_diff
        };

        output["diff"] = Value::String(diff);
        output["diff_truncated"] = Value::Bool(truncated);
    }

    Ok(ToolResult { output })
}

async fn atomic_write(path: &Path, content: &str) -> AgentResult<()> {
    let parent = path.parent().ok_or_else(|| {
        AgentError::Tool(format!(
            "edit write failed: path has no parent: {}",
            path.display()
        ))
    })?;

    let temp_path = parent.join(format!(".agent-lib-edit-{}.tmp", Uuid::new_v4()));
    fs::write(&temp_path, content)
        .await
        .map_err(|err| AgentError::Tool(format!("edit temp write failed: {err}")))?;

    match fs::rename(&temp_path, path).await {
        Ok(()) => Ok(()),
        Err(err)
            if matches!(
                err.kind(),
                ErrorKind::AlreadyExists | ErrorKind::PermissionDenied
            ) =>
        {
            fs::remove_file(path).await.map_err(|remove_err| {
                AgentError::Tool(format!("edit replace failed: {remove_err}"))
            })?;
            fs::rename(&temp_path, path)
                .await
                .map_err(|rename_err| AgentError::Tool(format!("edit rename failed: {rename_err}")))
        }
        Err(err) => {
            let _ = fs::remove_file(&temp_path).await;
            Err(AgentError::Tool(format!("edit rename failed: {err}")))
        }
    }
}

fn hash_content(content: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(content.as_bytes());
    format!("{:x}", hasher.finalize())
}

fn count_changed_lines(before: &str, after: &str) -> usize {
    let before_lines: Vec<&str> = before.lines().collect();
    let after_lines: Vec<&str> = after.lines().collect();
    let max_lines = before_lines.len().max(after_lines.len());

    let mut changed = 0usize;
    for index in 0..max_lines {
        if before_lines.get(index) != after_lines.get(index) {
            changed += 1;
        }
    }
    changed
}

fn build_line_diff(before: &str, after: &str) -> String {
    let before_lines: Vec<&str> = before.lines().collect();
    let after_lines: Vec<&str> = after.lines().collect();
    let max_lines = before_lines.len().max(after_lines.len());

    let mut diff = String::from("--- before\n+++ after\n");

    for index in 0..max_lines {
        let before_line = before_lines.get(index);
        let after_line = after_lines.get(index);

        if before_line == after_line {
            continue;
        }

        diff.push_str(&format!("@@ line {} @@\n", index + 1));
        if let Some(line) = before_line {
            diff.push('-');
            diff.push_str(line);
            diff.push('\n');
        }
        if let Some(line) = after_line {
            diff.push('+');
            diff.push_str(line);
            diff.push('\n');
        }
    }

    diff
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
