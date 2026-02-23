use std::path::PathBuf;

use serde_json::json;
use tokio::fs;

use agent_lib::tools::builtin::{CodeExecTool, FileSystemTool, ShellTool};
use agent_lib::tools::{Tool, ToolContext};

fn temp_dir() -> PathBuf {
    let mut dir = std::env::temp_dir();
    dir.push(format!("agent_lib_test_{}", uuid::Uuid::new_v4()));
    dir
}

#[tokio::test]
async fn test_filesystem_tool_read_write_delete() {
    let tool = FileSystemTool::new();
    let dir = temp_dir();
    fs::create_dir_all(&dir).await.unwrap();

    let ctx = ToolContext {
        cwd: Some(dir.to_string_lossy().to_string()),
        sandbox_root: None,
    };

    let path = "test.txt";
    let content = "hello";

    tool.execute(
        json!({"operation": "write", "path": path, "content": content}),
        &ctx,
    )
    .await
    .unwrap();

    let read = tool
        .execute(json!({"operation": "read", "path": path}), &ctx)
        .await
        .unwrap();
    assert_eq!(read.output.as_str().unwrap(), content);

    tool.execute(json!({"operation": "delete", "path": path}), &ctx)
        .await
        .unwrap();
}

#[tokio::test]
async fn test_filesystem_tool_edit_success() {
    let tool = FileSystemTool::new();
    let dir = temp_dir();
    fs::create_dir_all(&dir).await.unwrap();

    let ctx = ToolContext {
        cwd: Some(dir.to_string_lossy().to_string()),
        sandbox_root: None,
    };

    let path = "edit.txt";
    let initial = "alpha\nbeta\ngamma\n";
    fs::write(dir.join(path), initial).await.unwrap();

    let result = tool
        .execute(
            json!({
                "operation": "edit",
                "path": path,
                "edits": [
                    { "find": "beta", "replace": "BETA" }
                ]
            }),
            &ctx,
        )
        .await
        .unwrap();

    assert_eq!(
        result.output.get("replacements").and_then(|v| v.as_u64()),
        Some(1)
    );
    assert!(
        result
            .output
            .get("changed_line_count")
            .and_then(|v| v.as_u64())
            .unwrap_or(0)
            >= 1
    );

    let after = fs::read_to_string(dir.join(path)).await.unwrap();
    assert_eq!(after, "alpha\nBETA\ngamma\n");
}

#[tokio::test]
async fn test_filesystem_tool_edit_target_not_found() {
    let tool = FileSystemTool::new();
    let dir = temp_dir();
    fs::create_dir_all(&dir).await.unwrap();

    let ctx = ToolContext {
        cwd: Some(dir.to_string_lossy().to_string()),
        sandbox_root: None,
    };

    let path = "missing-target.txt";
    fs::write(dir.join(path), "hello").await.unwrap();

    let err = tool
        .execute(
            json!({
                "operation": "edit",
                "path": path,
                "edits": [
                    { "find": "not-there", "replace": "x" }
                ]
            }),
            &ctx,
        )
        .await
        .unwrap_err();

    assert!(format!("{err}").contains("not found"));
}

#[tokio::test]
async fn test_filesystem_tool_edit_dry_run() {
    let tool = FileSystemTool::new();
    let dir = temp_dir();
    fs::create_dir_all(&dir).await.unwrap();

    let ctx = ToolContext {
        cwd: Some(dir.to_string_lossy().to_string()),
        sandbox_root: None,
    };

    let path = "dry-run.txt";
    fs::write(dir.join(path), "hello world").await.unwrap();

    let result = tool
        .execute(
            json!({
                "operation": "edit",
                "path": path,
                "dry_run": true,
                "edits": [
                    { "find": "world", "replace": "agent" }
                ]
            }),
            &ctx,
        )
        .await
        .unwrap();

    assert_eq!(
        result.output.get("dry_run").and_then(|v| v.as_bool()),
        Some(true)
    );
    let after = fs::read_to_string(dir.join(path)).await.unwrap();
    assert_eq!(after, "hello world");
}

#[tokio::test]
async fn test_filesystem_tool_edit_rejects_sandbox_escape() {
    let tool = FileSystemTool::new();
    let dir = temp_dir();
    fs::create_dir_all(&dir).await.unwrap();

    let ctx = ToolContext {
        cwd: Some(dir.to_string_lossy().to_string()),
        sandbox_root: Some(dir.to_string_lossy().to_string()),
    };

    let err = tool
        .execute(
            json!({
                "operation": "edit",
                "path": "../outside.txt",
                "edits": [
                    { "find": "a", "replace": "b" }
                ]
            }),
            &ctx,
        )
        .await
        .unwrap_err();

    let msg = format!("{err}");
    assert!(msg.contains("parent dir") || msg.contains("escapes sandbox"));
}

#[tokio::test]
async fn test_filesystem_tool_edit_diff_truncation() {
    let tool = FileSystemTool::new();
    let dir = temp_dir();
    fs::create_dir_all(&dir).await.unwrap();

    let ctx = ToolContext {
        cwd: Some(dir.to_string_lossy().to_string()),
        sandbox_root: None,
    };

    let path = "diff.txt";
    let mut large = String::new();
    for _ in 0..200 {
        large.push_str("aaaaabbbbbcccccdddddeeeee\n");
    }
    fs::write(dir.join(path), &large).await.unwrap();

    let result = tool
        .execute(
            json!({
                "operation": "edit",
                "path": path,
                "return_diff": true,
                "max_diff_chars": 120,
                "edits": [
                    { "find": "aaaaa", "replace": "XXXXX", "replace_all": true }
                ]
            }),
            &ctx,
        )
        .await
        .unwrap();

    assert_eq!(
        result
            .output
            .get("diff_truncated")
            .and_then(|v| v.as_bool()),
        Some(true)
    );
    let diff = result
        .output
        .get("diff")
        .and_then(|v| v.as_str())
        .unwrap_or_default();
    assert!(diff.chars().count() <= 120);
}

#[tokio::test]
async fn test_shell_tool_exec() {
    let tool = ShellTool::new();
    let ctx = ToolContext {
        cwd: None,
        sandbox_root: None,
    };

    let command = if cfg!(windows) {
        "echo hello"
    } else {
        "echo hello"
    };

    let result = tool
        .execute(json!({"command": command}), &ctx)
        .await
        .unwrap();
    let stdout = result
        .output
        .get("stdout")
        .and_then(|v| v.as_str())
        .unwrap();
    assert!(stdout.contains("hello"));
}

#[tokio::test]
async fn test_code_exec_tool() {
    let tool = CodeExecTool::new();
    let ctx = ToolContext {
        cwd: None,
        sandbox_root: None,
    };

    let (language, code) = if cfg!(windows) {
        ("powershell", "Write-Output 42")
    } else {
        ("sh", "echo 42")
    };

    let result = tool
        .execute(json!({"language": language, "code": code}), &ctx)
        .await
        .unwrap();
    let stdout = result
        .output
        .get("stdout")
        .and_then(|v| v.as_str())
        .unwrap();
    assert!(stdout.contains("42"));
}
