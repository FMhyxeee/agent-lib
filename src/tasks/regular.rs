use std::sync::Arc;
use async_trait::async_trait;

use crate::session::{TaskSession, TurnContext};
use crate::tasks::{SessionTask, TaskKind};
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
        _session: Arc<dyn TaskSession>,
        _ctx: Arc<TurnContext>,
        _cancellation_token: CancellationToken,
    ) -> Option<String> {
        // TODO: 实现 run_turn 逻辑
        // 1. 获取对话历史
        // 2. 调用模型
        // 3. 处理工具调用
        // 4. 返回结果
        None
    }
}
