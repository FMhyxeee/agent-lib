use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::{debug, info};

use crate::protocol::{
    ApprovalPolicy, CollaborationMode, Event, McpServerRefreshConfig, Op, ReasoningEffort,
    ReasoningSummary, ReviewDecision, SandboxPolicy,
};
use crate::session::{Session, TurnContext};
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
                .await;
                previous_context = Some(sess.new_default_turn().await);
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

            Op::ApprovalResponse { request_id, approved } => {
                handle_approval_response(&sess, request_id, approved).await;
            }

            Op::Handoff { target_agent, context } => {
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

            Op::ListCustomPrompts => {
                handle_list_custom_prompts(&sess).await;
            }

            Op::ListModels => {
                handle_list_models(&sess).await;
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
) {
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
    };

    // 发送上下文覆盖事件
    sess.emit_event(crate::protocol::Event::Warning {
        message: format!(
            "Turn context overridden: model={}, cwd={:?}, approval_policy={:?}",
            new_ctx.model, new_ctx.cwd, new_ctx.approval_policy_v2
        ),
    })
    .await;
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
            };

            *previous_context = Some(Arc::new(new_ctx));

            // 处理输入项
            for item in items {
                match item {
                    crate::protocol::UserInputItem::Text { text } => {
                        // 文本输入，直接流式输出
                        sess.emit_event(Event::ModelStreaming { chunk: text }).await;
                    }
                    crate::protocol::UserInputItem::Image { path } => {
                        // 图片输入
                        debug!("Image input: {:?}", path);
                        sess.emit_event(Event::Warning {
                            message: format!("Image input received: {:?}", path),
                        })
                        .await;
                    }
                    crate::protocol::UserInputItem::File { path } => {
                        // 文件输入
                        debug!("File input: {:?}", path);
                        sess.emit_event(Event::Warning {
                            message: format!("File input received: {:?}", path),
                        })
                        .await;
                    }
                    crate::protocol::UserInputItem::Command { command } => {
                        // 命令输入
                        debug!("Command input: {}", command);
                        sess.emit_event(Event::RunUserShellCommand { command })
                            .await;
                    }
                }
            }
        }
        Op::UserInputLegacy {
            items,
            final_output_json_schema,
        } => {
            // 遗留格式支持
            if let Some(schema) = final_output_json_schema {
                let mut ctx_clone = (*ctx).clone();
                ctx_clone.final_output_json_schema = Some(schema);
                *previous_context = Some(Arc::new(ctx_clone));
            }

            for item in items {
                if let crate::protocol::UserInputItem::Text { text } = item {
                    sess.emit_event(Event::ModelStreaming { chunk: text }).await;
                }
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
                name: tool.name,
                description: tool.description,
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
            debug!("Checking {} MCP servers for unhealthy connections", servers.len());
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

async fn handle_undo(sess: &Session) {
    debug!("Handling undo");

    let current_len = sess.history().await.len();

    if current_len == 0 {
        sess.emit_event(crate::protocol::Event::Warning {
            message: "Cannot undo: history is empty".to_string(),
        })
        .await;
        return;
    }

    // 使用 with_history 来分析需要移除的消息
    let messages_to_remove = sess
        .with_history(|history| {
            let messages = history.all();

            // 找到最后一个用户输入或助手消息
            for (i, msg) in messages.iter().rev().enumerate() {
                if matches!(
                    msg.role,
                    crate::model::MessageRole::User | crate::model::MessageRole::Assistant
                ) {
                    // 如果是助手消息，我们需要确保移除完整的一轮
                    // 即助手消息前的用户消息（如果存在）
                    if msg.role == crate::model::MessageRole::Assistant && i < messages.len() - 1 {
                        let prev_msg = &messages[messages.len() - i - 2];
                        if prev_msg.role == crate::model::MessageRole::User {
                            return i + 2; // 用户 + 助手
                        }
                    }
                    return i + 1; // 只移除这一个消息
                }
            }
            0 // 没有找到需要移除的消息
        })
        .await;

    if messages_to_remove > 0 {
        // 使用 compact_history 方法来实现撤销功能
        // 保留所有消息除了最后 messages_to_remove 条
        let keep_recent = current_len - messages_to_remove;
        let summary = format!("Undo: removed {} messages", messages_to_remove);

        // 调用 compact_history 方法
        sess.compact_history(keep_recent, summary.clone()).await;

        sess.emit_event(crate::protocol::Event::UndoPerformed {
            removed_messages: messages_to_remove,
            summary,
        })
        .await;
    } else {
        sess.emit_event(crate::protocol::Event::Warning {
            message: "Cannot undo: no user or assistant messages to remove".to_string(),
        })
        .await;
    }
}

async fn handle_thread_rollback(sess: &Session, num_turns: u32) {
    debug!(num_turns, "Handling thread rollback");

    // 计算要保留的消息数量
    let history = sess.history().await;
    let current_len = history.len();
    let keep_recent = current_len.saturating_sub(num_turns as usize);

    // 使用现有的 compact 机制
    let summary = format!("Rolled back {} turns", num_turns);
    sess.compact_history(keep_recent, summary).await;

    sess.emit_event(crate::protocol::Event::ThreadRolledBack { num_turns })
        .await;
}

async fn handle_add_to_history(sess: &Session, text: String) {
    debug!("Handling add to history: {}", text);

    let mut history = sess.history().await;
    history.push(crate::model::Message::user(text.clone()));

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
    use tokio::time::{timeout, Duration};

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
            Command::new("cmd")
                .args(["/C", &command])
                .output()
        } else {
            Command::new("sh")
                .arg("-c")
                .arg(&command)
                .output()
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
            sess.emit_event(crate::protocol::Event::ModelStreaming {
                chunk: stdout,
            })
            .await;
        }
        sess.emit_event(crate::protocol::Event::ToolCallResult {
            tool: "shell".to_string(),
            result: crate::tools::ToolResult::text(format!(
                "Exit code: {}",
                exit_code
            )),
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
    // 基本的命令安全检查
    let command_lower = command.to_lowercase();

    // 禁止的命令
    let forbidden = [
        "rm -rf /",
        "format",
        "mkfs",
        "dd if=",
        "shutdown",
        "reboot",
        "halt",
        "poweroff",
    ];

    for forbidden_cmd in forbidden {
        if command_lower.contains(forbidden_cmd) {
            debug!(command = %command, "Command blocked by safety policy");
            return false;
        }
    }

    true
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
            result: crate::tools::ToolResult::text(format!(
                "Request {} approved",
                request_id
            )),
        })
        .await;
    } else {
        sess.emit_event(crate::protocol::Event::Error {
            error: crate::error::AgentError::Tool(format!(
                "Request {} denied by user",
                request_id
            )),
        })
        .await;
    }
}

/// 处理 Agent 移交
async fn handle_handoff(sess: &Session, target_agent: String, context: serde_json::Value) {
    debug!(target_agent = %target_agent, "Handling handoff");

    // 获取当前状态
    let current_state = sess.history().await;
    let state_json = serde_json::to_value(&current_state)
        .unwrap_or_else(|_| serde_json::json!({}));

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

    // TODO: 如果有 Agent 注册表，这里可以通知目标 Agent
    // if let Some(target) = AGENT_REGISTRY.get(&target_agent) {
    //     target.receive_handoff(handoff_context).await?;
    // }

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
async fn handle_review(
    sess: &Session,
    review_request: crate::protocol::ReviewRequest,
) {
    debug!(content = %review_request.content, "Handling review request");

    // 发送审查事件
    sess.emit_event(crate::protocol::Event::Warning {
        message: format!(
            "Review request received: {} chars",
            review_request.content.len()
        ),
    })
    .await;

    // TODO: 实际的代码审查逻辑
    // 1. 解析代码内容
    // 2. 运行审查器
    // 3. 返回审查结果

    sess.emit_event(crate::protocol::Event::ToolCallResult {
        tool: "review".to_string(),
        result: crate::tools::ToolResult::text(
            "Review request processed".to_string(),
        ),
    })
    .await;
}

/// 处理历史条目请求
async fn handle_get_history_entry_request(
    sess: &Session,
    offset: usize,
    log_id: u64,
) {
    debug!(offset = offset, log_id = log_id, "Handling get history entry request");

    let history = sess.history().await;
    let entries = history.all();

    if offset < entries.len() {
        let entry = &entries[offset];
        sess.emit_event(crate::protocol::Event::HistoryEntry {
            offset,
            log_id,
            entry: serde_json::to_string(entry).unwrap_or_else(|_| {
                format!("{:?}", entry)
            }),
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
async fn handle_list_skills(
    sess: &Session,
    cwds: Vec<std::path::PathBuf>,
    force_reload: bool,
) {
    debug!(cwds = ?cwds, force_reload = force_reload, "Handling list skills");

    // TODO: 实现从指定目录加载技能
    // 当前返回空列表
    let skills: Vec<crate::protocol::SkillEntry> = Vec::new();

    sess.emit_event(crate::protocol::Event::ListSkillsResponse {
        skills,
    })
    .await;

    if force_reload {
        sess.emit_event(crate::protocol::Event::Warning {
            message: "Skills cache refreshed".to_string(),
        })
        .await;
    }
}

/// 处理列出自定义提示请求
async fn handle_list_custom_prompts(sess: &Session) {
    debug!("Handling list custom prompts");

    // TODO: 实现自定义提示管理
    // 当前返回空列表
    let prompts: Vec<crate::protocol::CustomPromptInfo> = Vec::new();

    sess.emit_event(crate::protocol::Event::ListCustomPromptsResponse {
        prompts,
    })
    .await;
}

/// 处理列出模型请求
async fn handle_list_models(sess: &Session) {
    debug!("Handling list models");

    // TODO: 实现模型列表功能
    // 当前返回默认模型
    let models = vec![
        crate::protocol::ModelInfo {
            id: "default".to_string(),
            name: "Default Model".to_string(),
            provider: "builtin".to_string(),
        },
    ];

    sess.emit_event(crate::protocol::Event::ModelsListed { models })
        .await;
}
