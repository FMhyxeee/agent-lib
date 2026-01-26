use async_trait::async_trait;
use std::sync::Arc;
use tracing::{debug, info};

use crate::protocol::{Event, TurnAbortReason};
use crate::session::{TaskSession, TurnContext};
use crate::tasks::{SessionTask, TaskKind};
use crate::tools::ToolDef;
use tokio_util::sync::CancellationToken;

/// 常规 Turn 任务
///
/// 处理标准的用户输入和模型响应循环。
#[derive(Clone, Copy, Default)]
pub struct RegularTask;

#[async_trait]
impl SessionTask for RegularTask {
    fn kind(&self) -> TaskKind {
        TaskKind::Regular
    }

    async fn run(
        self: Arc<Self>,
        session: Arc<dyn TaskSession>,
        ctx: Arc<TurnContext>,
        cancellation_token: CancellationToken,
    ) -> Option<String> {
        let turn_id = ctx.sub_id.clone();
        info!("[{}] Starting RegularTask", turn_id);

        // 检查是否被取消
        if cancellation_token.is_cancelled() {
            debug!("[{}] Task cancelled before start", turn_id);
            session
                .emit_event(Event::TurnAborted {
                    reason: TurnAbortReason::Error("cancelled before start".to_string()),
                })
                .await;
            return None;
        }

        // 1. 获取对话历史
        let history = session.history().await;
        let total_tokens = history.total_tokens();
        debug!("[{}] Current history: {} tokens", turn_id, total_tokens);

        // 2. 检查是否需要压缩
        let context_window = ctx.context_window;
        if total_tokens > context_window {
            let keep_recent = ((context_window as f32) * 0.7) as usize;
            let summary = format!(
                "[Compacted {} tokens of conversation history before turn {}]",
                total_tokens.saturating_sub(keep_recent),
                turn_id
            );
            debug!(
                "[{}] Compacting history ({} > {} tokens), keeping {} recent messages",
                turn_id, total_tokens, context_window, keep_recent
            );
            session.compact_history(keep_recent, summary).await;
        }

        // 3. 准备模型调用
        let messages = history.for_prompt();

        if messages.is_empty() {
            debug!("[{}] No messages to send to model", turn_id);
            session
                .emit_event(Event::Warning {
                    message: "No messages in history to process".to_string(),
                })
                .await;
            return None;
        }

        // 4. 发送 ModelStreaming 事件（开始）
        session
            .emit_event(Event::ModelStreaming {
                chunk: format!("[{}] Thinking...\n", turn_id),
            })
            .await;

        // 5. 调用模型
        let response = match session.chat_model(messages, Vec::<ToolDef>::new()).await {
            Ok(resp) => resp,
            Err(e) => {
                debug!("[{}] Model call failed: {:?}", turn_id, e);
                session.emit_event(Event::Error { error: e }).await;
                return None;
            }
        };

        // 6. 发送流式响应内容（分块发送）
        let content = &response.content;
        let chunk_size = 20;
        for i in (0..content.len()).step_by(chunk_size) {
            if cancellation_token.is_cancelled() {
                session
                    .emit_event(Event::TurnAborted {
                        reason: TurnAbortReason::Error("cancelled during response".to_string()),
                    })
                    .await;
                return None;
            }
            let end = (i + chunk_size).min(content.len());
            let chunk = &content[i..end];
            session
                .emit_event(Event::ModelStreaming {
                    chunk: chunk.to_string(),
                })
                .await;
        }

        // 7. 发送完成事件
        session
            .emit_event(Event::ModelComplete {
                content: response.content.clone(),
                usage: response.usage,
            })
            .await;

        info!("[{}] RegularTask completed", turn_id);
        Some(format!(
            "Turn {}: {} tokens processed, response length: {}",
            turn_id,
            total_tokens,
            response.content.len()
        ))
    }
}
