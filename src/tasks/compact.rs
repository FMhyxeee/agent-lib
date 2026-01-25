use async_trait::async_trait;
use std::sync::Arc;

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

        // 3. 生成摘要（TODO: 实际应该调用 LLM 生成摘要）
        let summary = format!(
            "[Compacted {} tokens of conversation history]",
            total_tokens / 2
        );

        // 4. 压缩历史
        let keep_recent = 10; // 保留最近 10 条消息
        session.compact_history(keep_recent, summary).await;

        Some(format!("Compacted history from {} tokens", total_tokens))
    }
}
