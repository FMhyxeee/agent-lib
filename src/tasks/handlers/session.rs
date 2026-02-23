//! Session管理相关handlers
//!
//! 处理会话中断、上下文覆盖、撤销、回滚等操作。

use std::sync::Arc;
use tracing::debug;

use crate::model::{Message, MessageRole};
use crate::protocol::{
    ApprovalPolicy, CollaborationMode, ReasoningEffort, ReasoningSummary,
    SandboxPolicy,
};
use crate::session::{Session, TurnContext};

/// 处理会话中断
pub async fn handle_interrupt(sess: &Session) {
    debug!("Handling interrupt");
    sess.abort_all_tasks().await;

    // 发送中断错误事件
    sess.emit_event(crate::protocol::Event::Error {
        error: crate::error::AgentError::Session("Interrupted by user".to_string()),
    })
    .await;
}

/// 处理覆盖Turn上下文
#[allow(clippy::too_many_arguments)]
pub async fn handle_override_turn_context(
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
        approval_policy: match approval_policy {
            Some(policy) => Some(policy),
            None => ctx.approval_policy,
        },
        sandbox_policy: match sandbox_policy {
            Some(policy) => Some(policy),
            None => ctx.sandbox_policy,
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
            new_ctx.model, new_ctx.cwd, new_ctx.approval_policy
        ),
    })
    .await;

    Arc::new(new_ctx)
}

/// 计算需要移除的消息数量（用于撤销操作）
pub fn count_messages_to_remove_for_undo(messages: &[Message]) -> usize {
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

/// 计算需要移除的消息数量（用于回滚操作）
pub fn count_messages_to_remove_for_rollback(messages: &[Message], num_turns: u32) -> usize {
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

/// 处理撤销操作
pub async fn handle_undo(sess: &Session) {
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

/// 处理线程回滚操作
pub async fn handle_thread_rollback(sess: &Session, num_turns: u32) {
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

/// 处理添加到历史记录
pub async fn handle_add_to_history(sess: &Session, text: String) {
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
