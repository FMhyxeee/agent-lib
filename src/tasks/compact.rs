use async_trait::async_trait;
use std::sync::Arc;

use crate::model::Message;
use crate::session::{TaskSession, TurnContext};
use crate::tasks::{SessionTask, TaskKind};
use tokio_util::sync::CancellationToken;

/// 历史压缩任务
///
/// 当对话历史超过 token 限制时，自动压缩历史记录。
#[derive(Clone, Copy, Default)]
pub struct CompactTask;

#[async_trait]
impl SessionTask for CompactTask {
    fn kind(&self) -> TaskKind {
        TaskKind::Compact
    }

    async fn run(
        self: Arc<Self>,
        session: Arc<dyn TaskSession>,
        ctx: Arc<TurnContext>,
        _cancellation_token: CancellationToken,
    ) -> Option<String> {
        // 1. 获取当前历史
        let history = session.history().await;
        let total_tokens = history.total_tokens();

        // 2. 检查是否需要压缩
        let limit = ctx.auto_compact_token_limit.unwrap_or(100000) as usize;
        if total_tokens < limit {
            return None;
        }

        let transcript = build_transcript(&history);

        // 3. 生成摘要（调用 LLM，失败时回退到简单摘要）
        let summary = generate_summary(session.as_ref(), ctx.as_ref(), transcript)
            .await
            .unwrap_or_else(|| {
                format!(
                    "[Compacted {} tokens of conversation history]",
                    total_tokens / 2
                )
            });

        // 4. 压缩历史
        let keep_recent = 10; // 保留最近 10 条消息
        session.compact_history(keep_recent, summary).await;

        Some(format!("Compacted history from {} tokens", total_tokens))
    }
}

fn build_transcript(history: &crate::session::ConversationHistory) -> String {
    let messages = history.for_prompt();
    let mut transcript = String::new();
    for msg in messages {
        let role = match msg.role {
            crate::model::MessageRole::System => "system",
            crate::model::MessageRole::User => "user",
            crate::model::MessageRole::Assistant => "assistant",
            crate::model::MessageRole::Tool => "tool",
        };
        transcript.push_str(&format!("[{role}] {}\n", msg.content.trim()));
    }
    transcript
}

async fn generate_summary(
    session: &dyn TaskSession,
    ctx: &TurnContext,
    transcript: String,
) -> Option<String> {
    if transcript.trim().is_empty() {
        return None;
    }

    let summary_prompt = format!(
        "请用简洁要点总结以下对话，保留关键决策、约束和未完成事项。不要添加额外解释。\n\n{}",
        transcript
    );

    let summary_messages = vec![
        Message::system(format!(
            "你是对话摘要助手，输出简洁要点。上下文窗口: {}",
            ctx.context_window
        )),
        Message::user(summary_prompt),
    ];

    match session.chat_model(summary_messages, Vec::new()).await {
        Ok(resp) if !resp.content.trim().is_empty() => Some(resp.content.trim().to_string()),
        _ => None,
    }
}
