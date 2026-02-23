//! 用户交互相关handlers
//!
//! 处理用户输入、子代理、shell命令执行等交互操作。

use std::sync::Arc;
use tracing::debug;

use crate::guide_prompt::sub_agent_system_prompt;
use crate::model::Message;
use crate::protocol::{Event, Op, SubAgentMode, UserInputResponse};
use crate::session::{Session, TurnContext};

/// 处理用户输入或Turn操作（主要handler）
pub async fn handle_user_input_or_turn(
    sess: &Session,
    _sub_id: String,
    op: Op,
    previous_context: &mut Option<Arc<TurnContext>>,
) {
    debug!("Handling user input or turn");

    // 确保有 TurnContext
    if previous_context.is_none() {
        *previous_context = Some(sess.new_default_turn().await);
    }

    let ctx = previous_context.as_ref().unwrap().clone();

    match op {
        Op::UserTurn {
            items,
            cwd,
            approval_policy,
            sandbox_policy,
            model,
            effort,
            summary,
            final_output_json_schema,
            collaboration_mode,
            prompt_directives,
        } => {
            let (directive_developer_instructions, directive_user_instructions) =
                if let Some(directives) = prompt_directives {
                    (
                        directives.developer_instructions,
                        directives.user_instructions,
                    )
                } else {
                    (None, None)
                };

            // 更新上下文
            let base_ctx = &(*ctx);
            let new_ctx = TurnContext {
                model,
                cwd: if cwd != std::path::Path::new(".") {
                    Some(cwd.to_string_lossy().to_string())
                } else {
                    base_ctx.cwd.clone()
                },
                sub_id: base_ctx.sub_id.clone(),
                approval_policy: Some(approval_policy),
                sandbox_policy: Some(sandbox_policy),
                collaboration_mode: match collaboration_mode {
                    Some(mode) => Some(mode),
                    None => base_ctx.collaboration_mode,
                },
                reasoning_effort: match effort {
                    Some(effort) => Some(effort),
                    None => base_ctx.reasoning_effort,
                },
                reasoning_summary: Some(summary),
                user_instructions: directive_user_instructions
                    .or(base_ctx.user_instructions.clone()),
                developer_instructions: directive_developer_instructions
                    .or(base_ctx.developer_instructions.clone()),
                final_output_json_schema: final_output_json_schema
                    .or(base_ctx.final_output_json_schema.clone()),
                truncation_policy: base_ctx.truncation_policy.clone(),
                auto_compact_token_limit: base_ctx.auto_compact_token_limit,
                context_window: base_ctx.context_window,
                tool_output_max_size: base_ctx.tool_output_max_size,
            };

            *previous_context = Some(Arc::new(new_ctx.clone()));

            // 处理输入项 - 将文本添加到历史并启动模型调用
            let mut has_text_input = false;
            for item in items {
                match item {
                    crate::protocol::UserInputItem::Text { text } => {
                        // 修复 P0-1: 使用 push_message 直接写回历史
                        sess.push_message(crate::model::Message::user(text.clone()))
                            .await;
                        has_text_input = true;
                    }
                    crate::protocol::UserInputItem::Image { path } => {
                        debug!("Image input: {:?}", path);
                        sess.emit_event(Event::Warning {
                            message: format!("Image input received: {:?}", path),
                        })
                        .await;
                    }
                    crate::protocol::UserInputItem::File { path } => {
                        debug!("File input: {:?}", path);
                        sess.emit_event(Event::Warning {
                            message: format!("File input received: {:?}", path),
                        })
                        .await;
                    }
                    crate::protocol::UserInputItem::Command { command } => {
                        debug!("Command input: {}", command);
                        sess.emit_event(Event::RunUserShellCommand { command })
                            .await;
                    }
                }
            }

            // 如果有文本输入，启动 RegularTask 调用模型
            if has_text_input {
                use crate::tasks::RegularTask;
                sess.spawn_task(Arc::new(new_ctx), RegularTask).await;
            }
        }
        Op::UserInputLegacy {
            items,
            final_output_json_schema,
        } => {
            // 遗留格式支持
            let mut ctx_clone = (*ctx).clone();
            if let Some(schema) = final_output_json_schema {
                ctx_clone.final_output_json_schema = Some(schema);
            }
            *previous_context = Some(Arc::new(ctx_clone.clone()));

            let mut has_text_input = false;
            for item in items {
                if let crate::protocol::UserInputItem::Text { text } = item {
                    // 修复 P0-1: 使用 push_message 直接写回历史
                    sess.push_message(crate::model::Message::user(text)).await;
                    has_text_input = true;
                }
            }

            if has_text_input {
                use crate::tasks::RegularTask;
                sess.spawn_task(Arc::new(ctx_clone), RegularTask).await;
            }
        }
        _ => {
            // 忽略其他类型的操作
        }
    }
}

/// 处理用户输入响应
pub async fn handle_user_input_answer(
    sess: &Session,
    id: String,
    response: UserInputResponse,
) {
    debug!(id = %id, "Handling user input answer");

    match response {
        UserInputResponse::Text(text) => {
            // 添加到历史
            sess.emit_event(Event::ModelStreaming {
                chunk: format!("User answered: {}", text),
            })
            .await;
        }
        UserInputResponse::Cancel => {
            sess.emit_event(Event::Warning {
                message: "User cancelled the input".to_string(),
            })
            .await;
        }
    }
}

/// 处理简单用户输入
pub async fn handle_user_input(sess: &Session, content: String) {
    debug!(content = %content, "Handling user input");

    sess.emit_event(Event::ModelStreaming { chunk: content })
        .await;
}

/// 检查命令是否允许执行
fn is_command_allowed(command: &str) -> bool {
    let command_lower = command.to_lowercase();

    // === 黑名单：危险命令 ===
    let blocklist = [
        // 系统破坏
        "rm -rf /",
        "rm -rf /*",
        "rm -rf ~",
        "format",
        "mkfs",
        "dd if=/dev/zero",
        "dd if=/dev/random",
        // 系统控制
        "shutdown",
        "reboot",
        "halt",
        "poweroff",
        "init 0",
        // Windows 危险命令
        "format c:",
        "del /q",
        "rmdir /s /q c:\\",
        "shutdown /s",
        "shutdown /r",
        // 密码/密钥相关
        "passwd",
        "chpasswd",
        // 数据库删除
        "drop database",
        "truncate table",
        "delete from",
    ];

    for blocked in blocklist {
        if command_lower.contains(blocked) {
            debug!(command = %command, blocked = %blocked, "Command blocked by blocklist");
            return false;
        }
    }

    // === 白名单：允许的安全命令 ===
    // 这些命令模式是明确允许的
    let allowlist_patterns = [
        // 信息查看
        "ls",
        "dir",
        "cat",
        "type",
        "head",
        "tail",
        "grep",
        "findstr",
        "echo",
        "pwd",
        "cd",
        "which",
        "where",
        // Git 操作
        "git status",
        "git log",
        "git diff",
        "git show",
        "git branch",
        // 构建工具
        "cargo build",
        "cargo test",
        "cargo check",
        "cargo fmt",
        "cargo clippy",
        "npm run",
        "npm test",
        "npm build",
        "make",
        "cmake",
        // 文件操作（限制范围）
        "mkdir",
        "touch",
        "cp ", // 注意空格，避免匹配到其他命令
        "mv ",
        "copy ",
        "move ",
        "rm ", // 允许 rm 但不是 rm -rf /
        "del ",
    ];

    // 检查是否在白名单中
    for pattern in &allowlist_patterns {
        if command_lower.starts_with(pattern) {
            debug!(command = %command, "Command allowed by allowlist");
            return true;
        }
    }

    // 默认策略：不在白名单中的命令一律拒绝
    debug!(command = %command, "Command not in allowlist, rejecting by default");
    false
}

/// 处理用户 Shell 命令执行
pub async fn handle_run_user_shell_command(sess: &Session, command: String) {
    use tokio::process::Command;
    use tokio::time::{timeout, Duration};

    debug!(command = %command, "Handling run user shell command");

    // 检查命令是否允许执行
    if !is_command_allowed(&command) {
        sess.emit_event(Event::Error {
            error: crate::error::AgentError::Tool(format!(
                "Command not allowed by policy: {}",
                command
            )),
        })
        .await;
        return;
    }

    // 执行命令（带超时）
    let result = match timeout(
        Duration::from_secs(30),
        if cfg!(windows) {
            Command::new("cmd").args(["/C", &command]).output()
        } else {
            Command::new("sh").arg("-c").arg(&command).output()
        },
    )
    .await
    {
        Ok(Ok(output)) => output,
        Ok(Err(e)) => {
            let error_msg = format!("Command failed: {}", e);
            sess.emit_event(Event::Error {
                error: crate::error::AgentError::Tool(error_msg),
            })
            .await;
            return;
        }
        Err(_) => {
            sess.emit_event(Event::Error {
                error: crate::error::AgentError::Tool("Command timed out".to_string()),
            })
            .await;
            return;
        }
    };

    // 解析输出
    let stdout = String::from_utf8_lossy(&result.stdout).to_string();
    let stderr = String::from_utf8_lossy(&result.stderr).to_string();
    let exit_code = result.status.code().unwrap_or(-1);

    // 发送命令执行事件
    sess.emit_event(Event::RunUserShellCommand {
        command: command.clone(),
    })
    .await;

    // 发送执行结果
    if exit_code == 0 {
        // 成功 - 发送输出
        if !stdout.is_empty() {
            sess.emit_event(Event::ModelStreaming { chunk: stdout })
                .await;
        }
        sess.emit_event(Event::ToolCallResult {
            tool: "shell".to_string(),
            result: crate::tools::ToolResult::text(format!("Exit code: {}", exit_code)),
        })
        .await;
    } else {
        // 失败 - 发送错误信息
        let error_msg = if !stderr.is_empty() {
            format!("Command exited with {}: {}", exit_code, stderr)
        } else {
            format!("Command exited with {}", exit_code)
        };
        sess.emit_event(Event::Error {
            error: crate::error::AgentError::Tool(error_msg),
        })
        .await;
    }
}

/// 处理运行子代理
pub async fn handle_run_sub_agent(sess: &Session, mode: SubAgentMode, input: String) {
    debug!(mode = %mode.as_str(), "Handling run sub-agent");

    let trimmed_input = input.trim();
    if trimmed_input.is_empty() {
        sess.emit_event(Event::SubAgentFailed {
            mode,
            error: "sub-agent input cannot be empty".to_string(),
        })
        .await;
        sess.emit_event(Event::Error {
            error: crate::error::AgentError::Tool("sub-agent input cannot be empty".to_string()),
        })
        .await;
        return;
    }

    let stored_user_prompt = format!("[sub-agent:{}]\n{}", mode.as_str(), trimmed_input);
    sess.push_message(Message::user(stored_user_prompt)).await;

    sess.emit_event(Event::SubAgentStarted {
        mode,
        input: trimmed_input.to_string(),
    })
    .await;
    sess.emit_event(Event::SubAgentProgress {
        mode,
        message: "building sub-agent prompt".to_string(),
    })
    .await;

    let history = sess.history().await;
    let mut messages = history.for_prompt();
    messages.insert(0, Message::system(sub_agent_system_prompt(mode)));

    let response = match sess.chat_model(messages, vec![]).await {
        Ok(response) => response,
        Err(error) => {
            sess.emit_event(Event::SubAgentFailed {
                mode,
                error: error.to_string(),
            })
            .await;
            sess.emit_event(Event::Error { error }).await;
            return;
        }
    };

    let final_content = response.content.trim().to_string();
    sess.push_message(Message::assistant(final_content.clone()))
        .await;

    sess.emit_event(Event::SubAgentProgress {
        mode,
        message: "sub-agent response ready".to_string(),
    })
    .await;

    let mut chunk = String::new();
    let chunk_size = 20;
    for ch in final_content.chars() {
        chunk.push(ch);
        if chunk.chars().count() >= chunk_size {
            sess.emit_event(Event::ModelStreaming {
                chunk: chunk.clone(),
            })
            .await;
            chunk.clear();
        }
    }

    if !chunk.is_empty() {
        sess.emit_event(Event::ModelStreaming { chunk }).await;
    }

    sess.emit_event(Event::ModelComplete {
        content: final_content.clone(),
        usage: response.usage.clone(),
    })
    .await;

    sess.emit_event(Event::SubAgentCompleted {
        mode,
        output: final_content,
    })
    .await;
}
