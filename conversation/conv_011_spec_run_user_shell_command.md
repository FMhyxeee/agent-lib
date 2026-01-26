# Project Observation Record #011

**Date**: 2025-01-26
**Observer**: Claude (Agent)
**Status:** 📋 Specification Ready
**Topic:** RunUserShellCommand 实现规范

---

## 任务概述

**优先级:** 🔴 高
**文件:** `src/tasks/loop.rs`
**当前状态:** 只发送 Event，不执行命令

---

## 当前代码

```rust
// src/tasks/loop.rs:278-283
crate::protocol::UserInputItem::Command { command } => {
    // 命令输入
    debug!("Command input: {}", command);
    sess.emit_event(Event::RunUserShellCommand { command })
        .await;
}
```

---

## 需要实现的功能

### 1. 处理器签名

```rust
async fn handle_run_user_shell_command(sess: &Session, command: String) {
    debug!("Handling run user shell command: {}", command);

    // 1. 权限检查
    // 2. 沙箱执行
    // 3. 输出捕获
    // 4. 发送结果 Event
}
```

### 2. submission_loop 中添加匹配

```rust
// src/tasks/loop.rs match 分支中添加
Op::RunUserShellCommand { command } => {
    handle_run_user_shell_command(&sess, command).await;
}
```

### 3. Event 扩展 (如需要)

当前 Event 已经存在，但可能需要扩展：
```rust
// src/protocol/event.rs
Event::RunUserShellCommand {
    command: String,
    // 可能需要添加:
    // output: String,
    // exit_code: i32,
}
```

---

## 安全考虑

1. **命令验证** - 检查命令是否允许执行
2. **沙箱** - 在受限环境中执行
3. **超时** - 防止命令挂起
4. **资源限制** - 限制 CPU/内存使用

---

## 伪代码

```rust
async fn handle_run_user_shell_command(sess: &Session, command: String) {
    use tokio::process::Command;
    use tokio::time::{timeout, Duration};

    debug!("Executing command: {}", command);

    // 1. 权限检查
    let is_allowed = check_command_allowed(&command).await;
    if !is_allowed {
        sess.emit_event(Event::Error {
            error: AgentError::PermissionDenied(format!("Command not allowed: {}", command)),
        }).await;
        return;
    }

    // 2. 执行命令 (带超时)
    let result = match timeout(
        Duration::from_secs(30),
        Command::new("sh")
            .arg("-c")
            .arg(&command)
            .output()
    ).await {
        Ok(Ok(output)) => output,
        Ok(Err(e)) => {
            sess.emit_event(Event::Error {
                error: AgentError::Tool(format!("Command failed: {}", e)),
            }).await;
            return;
        }
        Err(_) => {
            sess.emit_event(Event::Error {
                error: AgentError::Timeout("Command timed out".to_string()),
            }).await;
            return;
        }
    };

    // 3. 发送结果
    let stdout = String::from_utf8_lossy(&result.stdout).to_string();
    let stderr = String::from_utf8_lossy(&result.stderr).to_string();
    let exit_code = result.status.code().unwrap_or(-1);

    sess.emit_event(Event::RunUserShellCommand {
        command: command.clone(),
    }).await;

    if exit_code == 0 {
        sess.emit_event(Event::ModelStreaming { chunk: stdout }).await;
    } else {
        sess.emit_event(Event::Error {
            error: AgentError::Tool(format!("Command exited with {}: {}", exit_code, stderr)),
        }).await;
    }
}

async fn check_command_allowed(command: &str) -> bool {
    // TODO: 实现命令白名单/黑名单检查
    true
}
```

---

## 测试用例

```rust
#[tokio::test]
async fn test_run_shell_command_echo() {
    let (session, handle) = Session::new(64);
    handle.submit(Op::RunUserShellCommand {
        command: "echo hello".to_string(),
    }).await;

    // 验证 Event
    let event = handle.next_event().await;
    assert!(matches!(event, Event::RunUserShellCommand { .. }));
}

#[tokio::test]
async fn test_run_shell_command_timeout() {
    // 测试超时命令
    handle.submit(Op::RunUserShellCommand {
        command: "sleep 100".to_string(),
    }).await;

    // 应该返回超时错误
}
```

---

## 相关文件

- `src/tasks/loop.rs` - 添加处理器
- `src/protocol/event.rs` - 可能需要扩展 Event
- `src/protocol/op.rs` - Op 已存在

---

**规范完成，等待实现。**

---
