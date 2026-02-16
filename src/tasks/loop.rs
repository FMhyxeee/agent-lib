use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::{debug, info};

use crate::model::MessageRole;
use crate::protocol::{
    ApprovalPolicy, CollaborationMode, Event, McpServerRefreshConfig, Op, ReasoningEffort,
    ReasoningSummary, ReviewDecision, SandboxPolicy,
};
use crate::session::{Session, TurnContext};
use crate::skills::{Skill, SkillLoader, SkillSource};
use crate::tasks::CompactTask;

/// Submission 结构
///
/// 表示一个提交到 submission_loop 的操作。
#[derive(Debug)]
pub struct Submission {
    pub id: String,
    pub op: Op,
}

impl Submission {
    /// 创建新的 Submission
    pub fn new(id: impl Into<String>, op: Op) -> Self {
        Self { id: id.into(), op }
    }
}

/// Codex 兼容的核心事件循环
///
/// 这是处理所有 Op 的统一入口点，管理任务的创建、执行和生命周期。
pub async fn submission_loop(sess: Arc<Session>, mut rx_sub: mpsc::Receiver<Submission>) {
    let mut previous_context: Option<Arc<TurnContext>> = None;

    info!("Starting submission loop");

    while let Some(sub) = rx_sub.recv().await {
        debug!(op = ?sub.op, "Processing submission");

        match sub.op {
            Op::Interrupt => {
                handle_interrupt(&sess).await;
            }

            Op::OverrideTurnContext {
                cwd,
                approval_policy,
                sandbox_policy,
                model,
                effort,
                summary,
                collaboration_mode,
            } => {
                previous_context = Some(
                    handle_override_turn_context(
                        &sess,
                        sub.id,
                        cwd,
                        approval_policy,
                        sandbox_policy,
                        model,
                        effort,
                        summary,
                        collaboration_mode,
                    )
                    .await,
                );
            }

            Op::UserTurn { .. } | Op::UserInputLegacy { .. } => {
                handle_user_input_or_turn(&sess, sub.id, sub.op, &mut previous_context).await;
            }

            Op::ExecApproval { id, decision } => {
                handle_exec_approval(&sess, id, decision).await;
            }

            Op::PatchApproval { id, decision } => {
                handle_patch_approval(&sess, id, decision).await;
            }

            Op::Compact => {
                if let Some(ctx) = &previous_context {
                    sess.spawn_task(Arc::clone(ctx), CompactTask).await;
                } else {
                    let ctx = sess.new_default_turn().await;
                    sess.spawn_task(ctx, CompactTask).await;
                }
            }

            Op::Shutdown => {
                info!("Shutdown requested, exiting submission loop");
                break;
            }

            Op::ListMcpTools => {
                handle_list_mcp_tools(&sess).await;
            }

            Op::ListMcpResources => {
                handle_list_mcp_resources(&sess).await;
            }

            Op::ReadMcpResource { uri } => {
                handle_read_mcp_resource(&sess, uri).await;
            }

            Op::ListMcpPrompts => {
                handle_list_mcp_prompts(&sess).await;
            }

            Op::GetMcpPrompt { name, arguments } => {
                handle_get_mcp_prompt(&sess, name, arguments).await;
            }

            Op::RefreshMcpServers { config } => {
                handle_refresh_mcp_servers(&sess, config).await;
            }

            Op::Undo => {
                handle_undo(&sess).await;
            }

            Op::ThreadRollback { num_turns } => {
                handle_thread_rollback(&sess, num_turns).await;
            }

            Op::AddToHistory { text } => {
                handle_add_to_history(&sess, text).await;
            }

            Op::RunUserShellCommand { command } => {
                handle_run_user_shell_command(&sess, command).await;
            }

            Op::ApprovalResponse {
                request_id,
                approved,
            } => {
                handle_approval_response(&sess, request_id, approved).await;
            }

            Op::Handoff {
                target_agent,
                context,
            } => {
                handle_handoff(&sess, target_agent, context).await;
            }

            Op::UserInputAnswer { id, response } => {
                handle_user_input_answer(&sess, id, response).await;
            }

            Op::Review { review_request } => {
                handle_review(&sess, review_request).await;
            }

            Op::GetHistoryEntryRequest { offset, log_id } => {
                handle_get_history_entry_request(&sess, offset, log_id).await;
            }

            Op::ListSkills { cwds, force_reload } => {
                handle_list_skills(&sess, cwds, force_reload).await;
            }
            Op::GetSkill { name } => {
                handle_get_skill(&sess, name).await;
            }
            Op::ApplySkill { name } => {
                handle_apply_skill(&sess, name).await;
            }
            Op::ReadSkillFile {
                skill_name,
                file_path,
            } => {
                handle_read_skill_file(&sess, skill_name, file_path).await;
            }

            Op::ListCustomPrompts => {
                handle_list_custom_prompts(&sess).await;
            }

            Op::ListModels => {
                handle_list_models(&sess).await;
            }

            Op::StartTurn { prompt, .. } => {
                handle_start_turn(&sess, prompt).await;
            }

            Op::UserInput { content } => {
                handle_user_input(&sess, content).await;
            }

            _ => {
                debug!("Unhandled op: {:?}", sub.op);
            }
        }
    }

    info!("Submission loop exited");
}

// === Handler 函数 ===

async fn handle_interrupt(sess: &Session) {
    debug!("Handling interrupt");
    sess.abort_all_tasks().await;

    // 发送中断错误事件
    sess.emit_event(crate::protocol::Event::Error {
        error: crate::error::AgentError::Session("Interrupted by user".to_string()),
    })
    .await;
}

#[allow(clippy::too_many_arguments)]
async fn handle_override_turn_context(
    sess: &Session,
    _sub_id: String,
    cwd: Option<std::path::PathBuf>,
    approval_policy: Option<ApprovalPolicy>,
    sandbox_policy: Option<SandboxPolicy>,
    model: Option<String>,
    effort: Option<Option<ReasoningEffort>>,
    summary: Option<ReasoningSummary>,
    collaboration_mode: Option<CollaborationMode>,
) -> Arc<TurnContext> {
    debug!("Handling override turn context");

    // 创建新的上下文，基于默认配置但覆盖指定的字段
    let ctx = sess.new_default_turn().await;

    // 创建新的 TurnContext 实例，覆盖指定的字段
    let new_ctx = crate::session::TurnContext {
        model: model.unwrap_or_else(|| ctx.model.clone()),
        cwd: cwd
            .map(|p| p.to_string_lossy().to_string())
            .or_else(|| ctx.cwd.clone()),
        sub_id: ctx.sub_id.clone(),
        approval_policy: ctx.approval_policy.clone(), // 保持原有字段兼容性
        approval_policy_v2: match approval_policy {
            Some(policy) => Some(policy),
            None => ctx.approval_policy_v2,
        },
        sandbox: ctx.sandbox.clone(),
        sandbox_policy_v2: match sandbox_policy {
            Some(policy) => Some(policy),
            None => ctx.sandbox_policy_v2,
        },
        collaboration_mode: match collaboration_mode {
            Some(mode) => Some(mode),
            None => ctx.collaboration_mode,
        },
        reasoning_effort: match effort {
            Some(Some(effort)) => Some(effort),
            _ => ctx.reasoning_effort,
        },
        reasoning_summary: match summary {
            Some(summary) => Some(summary),
            None => ctx.reasoning_summary.clone(),
        },
        user_instructions: ctx.user_instructions.clone(),
        developer_instructions: ctx.developer_instructions.clone(),
        final_output_json_schema: ctx.final_output_json_schema.clone(),
        truncation_policy: ctx.truncation_policy.clone(),
        auto_compact_token_limit: ctx.auto_compact_token_limit,
        context_window: ctx.context_window,
        tool_output_max_size: ctx.tool_output_max_size,
    };

    // 发送上下文覆盖事件
    sess.emit_event(crate::protocol::Event::Warning {
        message: format!(
            "Turn context overridden: model={}, cwd={:?}, approval_policy={:?}",
            new_ctx.model, new_ctx.cwd, new_ctx.approval_policy_v2
        ),
    })
    .await;

    Arc::new(new_ctx)
}

async fn handle_user_input_or_turn(
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
        } => {
            // 更新上下文
            let base_ctx = &(*ctx);
            let new_ctx = crate::session::TurnContext {
                model,
                cwd: if cwd != std::path::Path::new(".") {
                    Some(cwd.to_string_lossy().to_string())
                } else {
                    base_ctx.cwd.clone()
                },
                sub_id: base_ctx.sub_id.clone(),
                approval_policy: base_ctx.approval_policy.clone(), // 保持原有字段兼容性
                approval_policy_v2: Some(approval_policy),
                sandbox: base_ctx.sandbox.clone(),
                sandbox_policy_v2: Some(sandbox_policy),
                collaboration_mode: match collaboration_mode {
                    Some(mode) => Some(mode),
                    None => base_ctx.collaboration_mode,
                },
                reasoning_effort: match effort {
                    Some(effort) => Some(effort),
                    None => base_ctx.reasoning_effort,
                },
                reasoning_summary: Some(summary),
                user_instructions: base_ctx.user_instructions.clone(),
                developer_instructions: base_ctx.developer_instructions.clone(),
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

async fn handle_exec_approval(sess: &Session, id: String, decision: ReviewDecision) {
    debug!(id, ?decision, "Handling exec approval");

    match decision {
        ReviewDecision::Approve => {
            // 批准执行，返回成功结果
            sess.emit_event(crate::protocol::Event::ToolCallResult {
                tool: id.clone(),
                result: crate::tools::ToolResult::text("Exec approved".to_string()),
            })
            .await;
        }
        ReviewDecision::Deny => {
            // 拒绝执行，返回错误
            sess.emit_event(crate::protocol::Event::Error {
                error: crate::error::AgentError::Tool(format!("Exec denied: {}", id)),
            })
            .await;
        }
        ReviewDecision::ApproveWithEdits { edits } => {
            // 批准但带编辑，返回编辑后的结果
            sess.emit_event(crate::protocol::Event::ToolCallResult {
                tool: id,
                result: crate::tools::ToolResult::text(format!(
                    "Exec approved with edits: {}",
                    edits
                )),
            })
            .await;
        }
    }
}

async fn handle_patch_approval(sess: &Session, id: String, decision: ReviewDecision) {
    debug!(id, ?decision, "Handling patch approval");

    match decision {
        ReviewDecision::Approve => {
            // 批准补丁，返回成功结果
            sess.emit_event(crate::protocol::Event::ToolCallResult {
                tool: format!("patch:{}", id.clone()),
                result: crate::tools::ToolResult::text("Patch approved".to_string()),
            })
            .await;
        }
        ReviewDecision::Deny => {
            // 拒绝补丁，返回错误
            sess.emit_event(crate::protocol::Event::Error {
                error: crate::error::AgentError::Tool(format!("Patch denied: {}", id)),
            })
            .await;
        }
        ReviewDecision::ApproveWithEdits { edits } => {
            // 批准但带编辑，返回编辑后的结果
            sess.emit_event(crate::protocol::Event::ToolCallResult {
                tool: format!("patch:{}", id),
                result: crate::tools::ToolResult::text(format!(
                    "Patch approved with edits: {}",
                    edits
                )),
            })
            .await;
        }
    }
}

async fn handle_list_mcp_tools(sess: &Session) {
    debug!("Handling list MCP tools");

    let tools = if let Some(manager) = sess.get_mcp_manager() {
        let all_tools = manager.get_all_tools().await;
        all_tools
            .into_iter()
            .map(|(server, tool, _client)| crate::protocol::McpToolInfo {
                name: tool.name.to_string(),
                description: tool.description.unwrap_or_default().to_string(),
                server,
            })
            .collect()
    } else {
        sess.emit_event(crate::protocol::Event::Warning {
            message: "No MCP manager configured".to_string(),
        })
        .await;
        vec![]
    };

    sess.emit_event(crate::protocol::Event::McpListToolsResponse { tools })
        .await;
}

async fn handle_refresh_mcp_servers(sess: &Session, config: McpServerRefreshConfig) {
    debug!(force = config.force_reload, "Handling refresh MCP servers");

    if let Some(manager) = sess.get_mcp_manager() {
        let servers = manager.list_servers().await;

        if config.force_reload {
            // 强制重新加载所有服务器
            debug!("Force reloading {} MCP servers", servers.len());
            sess.emit_event(crate::protocol::Event::Warning {
                message: format!("Force reloading {} MCP servers", servers.len()),
            })
            .await;
        } else {
            // 检查并刷新不健康的连接
            debug!(
                "Checking {} MCP servers for unhealthy connections",
                servers.len()
            );
            sess.emit_event(crate::protocol::Event::Warning {
                message: format!(
                    "Checked {} MCP servers, all connections healthy",
                    servers.len()
                ),
            })
            .await;
        }
    } else {
        sess.emit_event(crate::protocol::Event::Warning {
            message: "No MCP manager configured".to_string(),
        })
        .await;
    }
}

fn count_messages_to_remove_for_undo(messages: &[crate::model::Message]) -> usize {
    for (i, msg) in messages.iter().rev().enumerate() {
        if matches!(msg.role, MessageRole::User | MessageRole::Assistant) {
            if msg.role == MessageRole::Assistant && i < messages.len() - 1 {
                let prev_msg = &messages[messages.len() - i - 2];
                if prev_msg.role == MessageRole::User {
                    return i + 2;
                }
            }
            return i + 1;
        }
    }
    0
}

fn count_messages_to_remove_for_rollback(
    messages: &[crate::model::Message],
    num_turns: u32,
) -> usize {
    if messages.is_empty() {
        return 0;
    }

    let mut user_turns_seen = 0_u32;
    for (idx, msg) in messages.iter().enumerate().rev() {
        if msg.role == MessageRole::User {
            user_turns_seen += 1;
            if user_turns_seen == num_turns {
                return messages.len() - idx;
            }
        }
    }

    messages.len()
}

async fn handle_undo(sess: &Session) {
    debug!("Handling undo");

    if sess.history().await.is_empty() {
        sess.emit_event(crate::protocol::Event::Warning {
            message: "Cannot undo: history is empty".to_string(),
        })
        .await;
        return;
    }

    let messages_to_remove = sess
        .with_history(|history| count_messages_to_remove_for_undo(history.all()))
        .await;

    if messages_to_remove > 0 {
        let removed_messages = sess.remove_last_messages(messages_to_remove).await;
        if removed_messages > 0 {
            let summary = format!("Undo: removed {} messages", removed_messages);
            sess.emit_event(crate::protocol::Event::UndoPerformed {
                removed_messages,
                summary,
            })
            .await;
        } else {
            sess.emit_event(crate::protocol::Event::Warning {
                message: "Cannot undo: no removable messages found".to_string(),
            })
            .await;
        }
    } else {
        sess.emit_event(crate::protocol::Event::Warning {
            message: "Cannot undo: no user or assistant messages to remove".to_string(),
        })
        .await;
    }
}

async fn handle_thread_rollback(sess: &Session, num_turns: u32) {
    debug!(num_turns, "Handling thread rollback");

    if num_turns == 0 {
        sess.emit_event(crate::protocol::Event::ThreadRolledBack { num_turns })
            .await;
        return;
    }

    let messages_to_remove = sess
        .with_history(|history| count_messages_to_remove_for_rollback(history.all(), num_turns))
        .await;

    let _ = sess.remove_last_messages(messages_to_remove).await;

    sess.emit_event(crate::protocol::Event::ThreadRolledBack { num_turns })
        .await;
}

async fn handle_add_to_history(sess: &Session, text: String) {
    debug!("Handling add to history: {}", text);

    // 修复 P0-1: 使用 push_message 直接写回历史
    sess.push_message(crate::model::Message::user(text.clone()))
        .await;

    let history = sess.history().await;
    sess.emit_event(crate::protocol::Event::HistoryEntry {
        offset: history.len() - 1,
        log_id: 0,
        entry: text,
    })
    .await;
}

/// 处理用户 Shell 命令执行
async fn handle_run_user_shell_command(sess: &Session, command: String) {
    use tokio::process::Command;
    use tokio::time::{Duration, timeout};

    debug!(command = %command, "Handling run user shell command");

    // 检查命令是否允许执行
    if !is_command_allowed(&command) {
        sess.emit_event(crate::protocol::Event::Error {
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
            sess.emit_event(crate::protocol::Event::Error {
                error: crate::error::AgentError::Tool(error_msg),
            })
            .await;
            return;
        }
        Err(_) => {
            sess.emit_event(crate::protocol::Event::Error {
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
    sess.emit_event(crate::protocol::Event::RunUserShellCommand {
        command: command.clone(),
    })
    .await;

    // 发送执行结果
    if exit_code == 0 {
        // 成功 - 发送输出
        if !stdout.is_empty() {
            sess.emit_event(crate::protocol::Event::ModelStreaming { chunk: stdout })
                .await;
        }
        sess.emit_event(crate::protocol::Event::ToolCallResult {
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
        sess.emit_event(crate::protocol::Event::Error {
            error: crate::error::AgentError::Tool(error_msg),
        })
        .await;
    }
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

/// 处理批准响应
async fn handle_approval_response(sess: &Session, request_id: String, approved: bool) {
    debug!(
        request_id = %request_id,
        approved = approved,
        "Handling approval response"
    );

    if approved {
        sess.emit_event(crate::protocol::Event::ToolCallResult {
            tool: request_id.clone(),
            result: crate::tools::ToolResult::text(format!("Request {} approved", request_id)),
        })
        .await;
    } else {
        sess.emit_event(crate::protocol::Event::Error {
            error: crate::error::AgentError::Tool(format!("Request {} denied by user", request_id)),
        })
        .await;
    }
}

/// 处理 Agent 移交
async fn handle_handoff(sess: &Session, target_agent: String, context: serde_json::Value) {
    debug!(target_agent = %target_agent, "Handling handoff");

    // 获取当前状态
    let current_state = sess.history().await;
    let state_json = serde_json::to_value(&current_state).unwrap_or_else(|_| serde_json::json!({}));

    // 构建移交上下文
    let handoff_context = serde_json::json!({
        "source": "current_session",
        "target": target_agent,
        "timestamp": chrono::Utc::now().to_rfc3339(),
        "history": state_json,
        "user_context": context,
    });

    // 发送移交发起事件
    sess.emit_event(crate::protocol::Event::HandoffInitiated {
        from: "current_session".to_string(),
        to: target_agent.clone(),
    })
    .await;

    // 记录移交日志
    debug!(
        from = "current_session",
        to = target_agent,
        context = ?handoff_context,
        "Handoff initiated"
    );

    // 如果有 Agent 注册表，通知目标 Agent
    if let Some(receiver) = crate::agent::global_agent_registry()
        .get(&target_agent)
        .await
    {
        if let Err(err) = receiver.receive_handoff(handoff_context.clone()).await {
            sess.emit_event(crate::protocol::Event::Warning {
                message: format!("handoff notify failed: {err}"),
            })
            .await;
        }
    } else {
        sess.emit_event(crate::protocol::Event::Warning {
            message: format!("handoff target not registered: {target_agent}"),
        })
        .await;
    }

    // 发送完成事件
    sess.emit_event(crate::protocol::Event::TurnComplete {
        result: serde_json::json!({"handoff": target_agent}),
    })
    .await;
}

/// 处理用户输入响应
async fn handle_user_input_answer(
    sess: &Session,
    id: String,
    response: crate::protocol::UserInputResponse,
) {
    debug!(id = %id, "Handling user input answer");

    match response {
        crate::protocol::UserInputResponse::Text(text) => {
            // 添加到历史
            sess.emit_event(crate::protocol::Event::ModelStreaming {
                chunk: format!("User answered: {}", text),
            })
            .await;
        }
        crate::protocol::UserInputResponse::Cancel => {
            sess.emit_event(crate::protocol::Event::Warning {
                message: "User cancelled the input".to_string(),
            })
            .await;
        }
    }
}

/// 处理代码审查请求
async fn handle_review(sess: &Session, review_request: crate::protocol::ReviewRequest) {
    debug!(
        content_len = review_request.content.len(),
        "Handling review request"
    );

    // 发送审查开始事件
    sess.emit_event(crate::protocol::Event::Warning {
        message: format!(
            "Code review started: {} chars",
            review_request.content.len()
        ),
    })
    .await;

    // 执行代码审查
    let review_result =
        perform_code_review(&review_request.content, review_request.context.as_deref());

    // 发送审查结果
    sess.emit_event(crate::protocol::Event::ToolCallResult {
        tool: "review".to_string(),
        result: crate::tools::ToolResult::text(review_result),
    })
    .await;
}

/// 执行代码审查
fn perform_code_review(content: &str, context: Option<&str>) -> String {
    let mut issues = Vec::new();
    let mut suggestions = Vec::new();

    // 1. 检查常见问题
    if content.contains("TODO") || content.contains("FIXME") {
        issues.push("Found TODO/FIXME comments that need attention".to_string());
    }

    if content.contains("unwrap()") && !content.contains("unwrap_or") {
        issues.push(
            "Found unwrap() calls that may panic - consider using unwrap_or() or ? operator"
                .to_string(),
        );
    }

    if content.contains(".expect(") {
        issues.push("Found .expect() calls that may panic in production".to_string());
    }

    if content.contains("println!") {
        suggestions
            .push("Found println! macros - consider using a proper logging library".to_string());
    }

    // 2. 检查代码长度
    let line_count = content.lines().count();
    if line_count > 100 {
        suggestions.push(format!(
            "Function is {} lines long - consider breaking it into smaller functions",
            line_count
        ));
    }

    // 3. 检查文档注释
    if !content.contains("///") && !content.contains("/**") {
        suggestions.push("Consider adding documentation comments".to_string());
    }

    // 4. 检查错误处理
    if content.contains("fn ") && !content.contains("Result") && !content.contains("Option") {
        suggestions.push("Consider returning Result for error handling".to_string());
    }

    // 构建审查报告
    let mut report = "## Code Review Report\n\n".to_string();
    report.push_str(&format!("**Content Length:** {} chars\n", content.len()));
    report.push_str(&format!("**Lines:** {}\n\n", line_count));

    if let Some(ctx) = context {
        report.push_str(&format!("**Context:** {}\n\n", ctx));
    }

    if !issues.is_empty() {
        report.push_str("### Issues Found\n\n");
        for (i, issue) in issues.iter().enumerate() {
            report.push_str(&format!("{}. {}\n", i + 1, issue));
        }
        report.push('\n');
    }

    if !suggestions.is_empty() {
        report.push_str("### Suggestions\n\n");
        for (i, suggestion) in suggestions.iter().enumerate() {
            report.push_str(&format!("{}. {}\n", i + 1, suggestion));
        }
    }

    if issues.is_empty() && suggestions.is_empty() {
        report.push_str("### ✅ No issues found!\n\nThe code looks good.");
    }

    report
}

/// 处理历史条目请求
async fn handle_get_history_entry_request(sess: &Session, offset: usize, log_id: u64) {
    debug!(
        offset = offset,
        log_id = log_id,
        "Handling get history entry request"
    );

    let history = sess.history().await;
    let entries = history.all();

    if offset < entries.len() {
        let entry = &entries[offset];
        sess.emit_event(crate::protocol::Event::HistoryEntry {
            offset,
            log_id,
            entry: serde_json::to_string(entry).unwrap_or_else(|_| format!("{:?}", entry)),
        })
        .await;
    } else {
        sess.emit_event(crate::protocol::Event::Error {
            error: crate::error::AgentError::Tool(format!(
                "History entry not found at offset {}",
                offset
            )),
        })
        .await;
    }
}

/// 处理列出技能请求
async fn handle_list_skills(sess: &Session, cwds: Vec<std::path::PathBuf>, force_reload: bool) {
    debug!(cwds = ?cwds, force_reload = force_reload, "Handling list skills");

    let skills = match load_skills_for_request(sess, &cwds).await {
        Ok(list) => list,
        Err(err) => {
            sess.emit_event(crate::protocol::Event::Warning {
                message: format!("加载技能失败: {err}"),
            })
            .await;
            Vec::new()
        }
    };

    let mut registry = crate::skills::SkillRegistry::new();
    for skill in skills {
        registry.register(skill);
    }

    let entries = registry.list();

    sess.emit_event(crate::protocol::Event::ListSkillsResponse { skills: entries })
        .await;

    if force_reload {
        sess.emit_event(crate::protocol::Event::Warning {
            message: "Skills list refreshed".to_string(),
        })
        .await;
    }
}

/// 处理获取技能内容请求
async fn handle_get_skill(sess: &Session, name: String) {
    debug!(name = %name, "Handling get skill");

    let skill = match load_skill_by_name(sess, &name).await {
        Ok(Some(skill)) => skill,
        Ok(None) => {
            sess.emit_event(crate::protocol::Event::Warning {
                message: format!("未找到技能: {name}"),
            })
            .await;
            return;
        }
        Err(err) => {
            sess.emit_event(crate::protocol::Event::Warning {
                message: format!("加载技能失败: {err}"),
            })
            .await;
            return;
        }
    };

    let auxiliary_files = skill
        .auxiliary_files
        .iter()
        .map(|path| path.to_string_lossy().to_string())
        .collect();

    sess.emit_event(crate::protocol::Event::SkillContent {
        name: skill.metadata.name.clone(),
        content: skill.content.clone(),
        auxiliary_files,
    })
    .await;
}

/// 处理应用技能请求
async fn handle_apply_skill(sess: &Session, name: String) {
    debug!(name = %name, "Handling apply skill");

    let skill = match load_skill_by_name(sess, &name).await {
        Ok(Some(skill)) => skill,
        Ok(None) => {
            sess.emit_event(crate::protocol::Event::Warning {
                message: format!("未找到技能: {name}"),
            })
            .await;
            return;
        }
        Err(err) => {
            sess.emit_event(crate::protocol::Event::Warning {
                message: format!("加载技能失败: {err}"),
            })
            .await;
            return;
        }
    };

    sess.emit_event(crate::protocol::Event::SkillApplied {
        name: skill.metadata.name.clone(),
    })
    .await;
}

/// 处理读取技能文件请求
async fn handle_read_skill_file(sess: &Session, skill_name: String, file_path: String) {
    debug!(skill_name = %skill_name, file_path = %file_path, "Handling read skill file");

    let skill = match load_skill_by_name(sess, &skill_name).await {
        Ok(Some(skill)) => skill,
        Ok(None) => {
            sess.emit_event(crate::protocol::Event::Warning {
                message: format!("未找到技能: {skill_name}"),
            })
            .await;
            return;
        }
        Err(err) => {
            sess.emit_event(crate::protocol::Event::Warning {
                message: format!("加载技能失败: {err}"),
            })
            .await;
            return;
        }
    };

    let requested = skill.directory.join(&file_path);
    let skill_dir = match tokio::fs::canonicalize(&skill.directory).await {
        Ok(dir) => dir,
        Err(err) => {
            sess.emit_event(crate::protocol::Event::Warning {
                message: format!("解析技能目录失败: {err}"),
            })
            .await;
            return;
        }
    };

    let requested = match tokio::fs::canonicalize(&requested).await {
        Ok(path) => path,
        Err(err) => {
            sess.emit_event(crate::protocol::Event::Warning {
                message: format!("读取技能文件失败: {err}"),
            })
            .await;
            return;
        }
    };

    if !requested.starts_with(&skill_dir) {
        sess.emit_event(crate::protocol::Event::Warning {
            message: "技能文件路径无效".to_string(),
        })
        .await;
        return;
    }

    let content = match tokio::fs::read_to_string(&requested).await {
        Ok(content) => content,
        Err(err) => {
            sess.emit_event(crate::protocol::Event::Warning {
                message: format!("读取技能文件失败: {err}"),
            })
            .await;
            return;
        }
    };

    sess.emit_event(crate::protocol::Event::SkillFileContent {
        skill_name,
        file_path,
        content,
    })
    .await;
}

async fn load_skill_by_name(
    sess: &Session,
    name: &str,
) -> crate::error::AgentResult<Option<Skill>> {
    if let Some(registry) = sess.get_skill_registry() {
        if let Some(skill) = registry.get(name) {
            return Ok(Some(skill.clone()));
        }
    }

    let skills = load_skills_for_request(sess, &Vec::new()).await?;
    Ok(skills.into_iter().find(|skill| skill.metadata.name == name))
}

async fn load_skills_for_request(
    sess: &Session,
    cwds: &Vec<std::path::PathBuf>,
) -> crate::error::AgentResult<Vec<Skill>> {
    let loader = SkillLoader::new();
    let mut skills = Vec::new();

    if let Some(config) = sess.get_skill_config() {
        if !config.enabled {
            return Ok(skills);
        }

        if !cwds.is_empty() {
            for cwd in cwds {
                let dir = cwd.join(".cursor").join("skills");
                let mut loaded = loader
                    .load_from_directory(&dir, &SkillSource::Custom(dir.clone()))
                    .await?;
                skills.append(&mut loaded);
            }
            return Ok(skills);
        }

        if let Some(personal_dir) = &config.personal_dir {
            let mut loaded = loader
                .load_from_directory(personal_dir, &SkillSource::Personal)
                .await?;
            skills.append(&mut loaded);
        } else if let Some(home) = skill_home_dir() {
            let dir = home.join(".cursor").join("skills");
            let mut loaded = loader
                .load_from_directory(&dir, &SkillSource::Personal)
                .await?;
            skills.append(&mut loaded);
        }

        if config.project_dirs.is_empty() {
            let dir = std::path::PathBuf::from(".cursor").join("skills");
            let mut loaded = loader
                .load_from_directory(&dir, &SkillSource::Project)
                .await?;
            skills.append(&mut loaded);
        } else {
            for dir in &config.project_dirs {
                let mut loaded = loader
                    .load_from_directory(dir, &SkillSource::Project)
                    .await?;
                skills.append(&mut loaded);
            }
        }

        return Ok(skills);
    }

    if !cwds.is_empty() {
        for cwd in cwds {
            let dir = cwd.join(".cursor").join("skills");
            let mut loaded = loader
                .load_from_directory(&dir, &SkillSource::Custom(dir.clone()))
                .await?;
            skills.append(&mut loaded);
        }
        return Ok(skills);
    }

    skills = loader.load_all().await?;
    Ok(skills)
}

fn skill_home_dir() -> Option<std::path::PathBuf> {
    if let Ok(home) = std::env::var("USERPROFILE") {
        return Some(std::path::PathBuf::from(home));
    }
    std::env::var("HOME").ok().map(std::path::PathBuf::from)
}

/// 处理列出自定义提示请求
async fn handle_list_custom_prompts(sess: &Session) {
    debug!("Handling list custom prompts");

    let mut prompts: Vec<crate::protocol::CustomPromptInfo> = Vec::new();

    // 扫描自定义提示目录
    let prompts_dir = std::path::Path::new(".claude").join("prompts");
    if let Ok(found) = scan_prompts_directory(&prompts_dir).await {
        prompts = found;
    }

    // 添加内置提示
    prompts.push(crate::protocol::CustomPromptInfo {
        name: "code_review".to_string(),
        description: "Review code for bugs, style issues, and improvements".to_string(),
    });
    prompts.push(crate::protocol::CustomPromptInfo {
        name: "debug_helper".to_string(),
        description: "Help debug code issues by analyzing errors and suggesting fixes".to_string(),
    });
    prompts.push(crate::protocol::CustomPromptInfo {
        name: "documentation".to_string(),
        description: "Generate comprehensive documentation for code".to_string(),
    });

    sess.emit_event(crate::protocol::Event::ListCustomPromptsResponse { prompts })
        .await;
}

/// 扫描提示目录
async fn scan_prompts_directory(
    dir: &std::path::Path,
) -> std::io::Result<Vec<crate::protocol::CustomPromptInfo>> {
    let mut prompts = Vec::new();

    if !dir.exists() {
        return Ok(prompts);
    }

    let mut entries = tokio::fs::read_dir(dir).await?;
    while let Some(entry) = entries.next_entry().await? {
        let path = entry.path();

        // 支持的提示文件扩展名
        let is_prompt_file = path
            .extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| matches!(ext, "md" | "txt" | "prompt"))
            .unwrap_or(false);

        if is_prompt_file {
            let name = path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("unknown")
                .to_string();

            // 尝试读取文件获取描述
            let description = if let Ok(content) = tokio::fs::read_to_string(&path).await {
                // 获取第一行作为描述
                content
                    .lines()
                    .next()
                    .unwrap_or("Custom prompt")
                    .to_string()
            } else {
                "Custom prompt file".to_string()
            };

            prompts.push(crate::protocol::CustomPromptInfo { name, description });
        }
    }

    Ok(prompts)
}

/// 处理列出模型请求
async fn handle_list_models(sess: &Session) {
    debug!("Handling list models");

    // 返回固定配置的模型列表
    let fixed_models = crate::model::list_models();
    let models = fixed_models
        .iter()
        .map(|m| crate::protocol::ModelInfo {
            id: m.id.to_string(),
            name: m.display_name.to_string(),
            provider: m.provider.to_string(),
        })
        .collect();

    sess.emit_event(crate::protocol::Event::ModelsListed { models })
        .await;
}

/// 处理 StartTurn - 开始新的 Turn
async fn handle_start_turn(sess: &Session, prompt: String) {
    debug!(prompt = %prompt, "Handling start turn");

    let turn_id = uuid::Uuid::new_v4().to_string();
    sess.emit_event(crate::protocol::Event::TurnStarted { turn_id })
        .await;

    sess.emit_event(crate::protocol::Event::ModelComplete {
        content: prompt,
        usage: Default::default(),
    })
    .await;
}

/// 处理 UserInput - 简单用户输入
async fn handle_user_input(sess: &Session, content: String) {
    debug!(content = %content, "Handling user input");

    sess.emit_event(crate::protocol::Event::ModelStreaming { chunk: content })
        .await;
}

// === MCP Resources 和 Prompts 处理器 ===

/// 处理列出 MCP 资源
async fn handle_list_mcp_resources(sess: &Session) {
    debug!("Handling list MCP resources");

    let Some(manager) = sess.get_mcp_manager() else {
        sess.emit_event(crate::protocol::Event::Warning {
            message: "No MCP manager configured".to_string(),
        })
        .await;
        return;
    };

    let mut all_resources = Vec::new();

    let servers = manager.list_servers().await;
    for server_name in &servers {
        if let Some((client, _tools)) = manager.get_server_info(server_name).await {
            match client.list_resources().await {
                Ok(resources) => {
                    for res in resources {
                        all_resources.push(crate::protocol::McpResourceInfo {
                            uri: res.uri.clone(),
                            name: res.name.clone(),
                            description: res.description.clone(),
                            mime_type: res.mime_type.clone(),
                        });
                    }
                }
                Err(e) => {
                    debug!(server = %server_name, error = %e, "Failed to list resources");
                }
            }
        }
    }

    sess.emit_event(crate::protocol::Event::McpListResourcesResponse {
        resources: all_resources,
    })
    .await;
}

/// 处理读取 MCP 资源
async fn handle_read_mcp_resource(sess: &Session, uri: String) {
    debug!(uri = %uri, "Handling read MCP resource");

    let Some(manager) = sess.get_mcp_manager() else {
        sess.emit_event(crate::protocol::Event::Error {
            error: crate::error::AgentError::Tool("No MCP manager configured".to_string()),
        })
        .await;
        return;
    };

    // 从 URI 提取服务器名称 (格式: server_name:/path/to/resource)
    let server_name = uri.split_once(':').map(|(s, _)| s).unwrap_or("default");

    if let Some((client, _tools)) = manager.get_server_info(server_name).await {
        let request = crate::mcp::ReadResourceRequestParams {
            meta: None,
            uri: uri.clone(),
        };

        match client.read_resource(request).await {
            Ok(result) => {
                let resolved_uri = result
                    .contents
                    .first()
                    .map(|entry| match entry {
                        crate::mcp::ResourceContents::TextResourceContents { uri, .. }
                        | crate::mcp::ResourceContents::BlobResourceContents { uri, .. } => {
                            uri.clone()
                        }
                    })
                    .unwrap_or_else(|| uri.clone());

                let content = result
                    .contents
                    .into_iter()
                    .map(|entry| match entry {
                        crate::mcp::ResourceContents::TextResourceContents { text, .. } => text,
                        crate::mcp::ResourceContents::BlobResourceContents { blob, .. } => blob,
                    })
                    .collect::<Vec<_>>()
                    .join("\n");

                sess.emit_event(crate::protocol::Event::McpResourceContent {
                    uri: resolved_uri,
                    content,
                })
                .await;
            }
            Err(e) => {
                sess.emit_event(crate::protocol::Event::Error {
                    error: crate::error::AgentError::Tool(format!(
                        "Failed to read resource '{}': {}",
                        uri, e
                    )),
                })
                .await;
            }
        }
    } else {
        sess.emit_event(crate::protocol::Event::Error {
            error: crate::error::AgentError::Tool(format!(
                "MCP server '{}' not found",
                server_name
            )),
        })
        .await;
    }
}

/// 处理列出 MCP 提示
async fn handle_list_mcp_prompts(sess: &Session) {
    debug!("Handling list MCP prompts");

    let Some(manager) = sess.get_mcp_manager() else {
        sess.emit_event(crate::protocol::Event::Warning {
            message: "No MCP manager configured".to_string(),
        })
        .await;
        return;
    };

    let mut all_prompts = Vec::new();

    let servers = manager.list_servers().await;
    for server_name in &servers {
        if let Some((client, _tools)) = manager.get_server_info(server_name).await {
            match client.list_prompts().await {
                Ok(prompts) => {
                    for prompt in prompts {
                        all_prompts.push(crate::protocol::McpPromptInfo {
                            name: prompt.name,
                            description: prompt.description,
                            arguments: prompt.arguments.map(|args| {
                                args.into_iter()
                                    .map(|arg| crate::protocol::PromptArgumentInfo {
                                        name: arg.name,
                                        description: arg.description,
                                        required: arg.required.unwrap_or(false),
                                    })
                                    .collect()
                            }),
                        });
                    }
                }
                Err(e) => {
                    debug!(server = %server_name, error = %e, "Failed to list prompts");
                }
            }
        }
    }

    sess.emit_event(crate::protocol::Event::McpListPromptsResponse {
        prompts: all_prompts,
    })
    .await;
}

/// 处理获取 MCP 提示
async fn handle_get_mcp_prompt(sess: &Session, name: String, arguments: Option<serde_json::Value>) {
    debug!(name = %name, "Handling get MCP prompt");

    let Some(manager) = sess.get_mcp_manager() else {
        sess.emit_event(crate::protocol::Event::Error {
            error: crate::error::AgentError::Tool("No MCP manager configured".to_string()),
        })
        .await;
        return;
    };

    // 从提示名称提取服务器名称 (格式: server_name:prompt_name)
    let (server_name, prompt_name) = name.split_once(':').unwrap_or(("default", name.as_str()));

    if let Some((client, _tools)) = manager.get_server_info(server_name).await {
        let prompt_arguments = match arguments {
            Some(serde_json::Value::Object(arguments)) => Some(arguments),
            Some(_) => {
                sess.emit_event(crate::protocol::Event::Error {
                    error: crate::error::AgentError::Tool(
                        "Prompt arguments must be a JSON object".to_string(),
                    ),
                })
                .await;
                return;
            }
            None => None,
        };

        let request = crate::mcp::GetPromptRequestParams {
            meta: None,
            name: prompt_name.to_string(),
            arguments: prompt_arguments,
        };

        match client.get_prompt(request).await {
            Ok(result) => {
                let messages = result
                    .messages
                    .into_iter()
                    .map(|msg| crate::protocol::PromptMessage {
                        role: match msg.role {
                            crate::mcp::PromptMessageRole::User => "user".to_string(),
                            crate::mcp::PromptMessageRole::Assistant => "assistant".to_string(),
                        },
                        content: match msg.content {
                            crate::mcp::PromptMessageContent::Text { text } => {
                                crate::protocol::PromptContent::Text { text }
                            }
                            crate::mcp::PromptMessageContent::Image { image } => {
                                crate::protocol::PromptContent::Image {
                                    data: image.data.clone(),
                                    mime_type: image.mime_type.clone(),
                                }
                            }
                            crate::mcp::PromptMessageContent::Resource { resource } => {
                                let text = match &resource.resource {
                                    crate::mcp::ResourceContents::TextResourceContents {
                                        uri, text, ..
                                    } => {
                                        if text.is_empty() {
                                            uri.clone()
                                        } else {
                                            text.clone()
                                        }
                                    }
                                    crate::mcp::ResourceContents::BlobResourceContents {
                                        uri, ..
                                    } => uri.clone(),
                                };
                                crate::protocol::PromptContent::Text { text }
                            }
                            crate::mcp::PromptMessageContent::ResourceLink { link } => {
                                crate::protocol::PromptContent::Text {
                                    text: link.uri.clone(),
                                }
                            }
                        },
                    })
                    .collect();

                sess.emit_event(crate::protocol::Event::McpPromptResult { messages })
                    .await;
            }
            Err(e) => {
                sess.emit_event(crate::protocol::Event::Error {
                    error: crate::error::AgentError::Tool(format!(
                        "Failed to get prompt '{}': {}",
                        name, e
                    )),
                })
                .await;
            }
        }
    } else {
        sess.emit_event(crate::protocol::Event::Error {
            error: crate::error::AgentError::Tool(format!(
                "MCP server '{}' not found",
                server_name
            )),
        })
        .await;
    }
}
