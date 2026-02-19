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
