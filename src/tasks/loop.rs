use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::{debug, info};

use crate::protocol::{
    ApprovalPolicy, CollaborationMode, McpServerRefreshConfig, Op, ReasoningEffort,
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
        Self {
            id: id.into(),
            op,
        }
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

    // 更新当前上下文
    // TODO: 实现上下文覆盖逻辑
    let _ = (cwd, approval_policy, sandbox_policy, model, effort, summary, collaboration_mode);

    sess.emit_event(crate::protocol::Event::Warning {
        message: "Turn context overridden".to_string(),
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
            // TODO: 处理用户 Turn
            let _ = (
                items, cwd, approval_policy, sandbox_policy, model, effort,
                summary, final_output_json_schema, collaboration_mode,
            );
        }
        Op::UserInputLegacy {
            items,
            final_output_json_schema,
        } => {
            // TODO: 处理遗留用户输入
            let _ = (items, final_output_json_schema);
        }
        _ => {}
    }
}

async fn handle_exec_approval(sess: &Session, id: String, decision: ReviewDecision) {
    debug!(id, ?decision, "Handling exec approval");
    // TODO: 实现执行批准逻辑
    sess.emit_event(crate::protocol::Event::Warning {
        message: format!("Exec approval: {} {:?}", id, decision),
    })
    .await;
}

async fn handle_patch_approval(sess: &Session, id: String, decision: ReviewDecision) {
    debug!(id, ?decision, "Handling patch approval");
    // TODO: 实现补丁批准逻辑
    sess.emit_event(crate::protocol::Event::Warning {
        message: format!("Patch approval: {} {:?}", id, decision),
    })
    .await;
}

async fn handle_list_mcp_tools(sess: &Session) {
    debug!("Handling list MCP tools");
    // TODO: 实现列出 MCP 工具逻辑
    sess.emit_event(crate::protocol::Event::McpListToolsResponse { tools: vec![] })
        .await;
}

async fn handle_refresh_mcp_servers(_sess: &Session, config: McpServerRefreshConfig) {
    debug!(force = config.force_reload, "Handling refresh MCP servers");
    // TODO: 实现刷新 MCP 服务器逻辑
}

async fn handle_undo(sess: &Session) {
    debug!("Handling undo");
    // TODO: 实现撤销逻辑
    sess.emit_event(crate::protocol::Event::Warning {
        message: "Undo operation requested".to_string(),
    })
    .await;
}

async fn handle_thread_rollback(sess: &Session, num_turns: u32) {
    debug!(num_turns, "Handling thread rollback");
    // TODO: 实现线程回滚逻辑
    sess.emit_event(crate::protocol::Event::ThreadRolledBack { num_turns })
        .await;
}

async fn handle_add_to_history(_sess: &Session, _text: String) {
    debug!("Handling add to history");
    // TODO: 实现添加到历史逻辑
}
