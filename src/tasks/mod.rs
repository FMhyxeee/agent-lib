mod compact;
mod handlers;
mod r#loop;
mod regular;

pub use compact::CompactTask;
pub use r#loop::{Submission, submission_loop};
pub use regular::RegularTask;

use async_trait::async_trait;
use std::sync::Arc;
use tokio_util::sync::CancellationToken;

use crate::session::{TaskSession, TurnContext};

/// Task 类型标识
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskKind {
    /// 常规 Turn 任务
    Regular,
    /// 历史压缩任务
    Compact,
    /// 审查任务
    Review,
    /// 撤销任务
    Undo,
    /// 用户 Shell 命令任务
    UserShell,
}

/// SessionTask trait - 所有 Task 必须实现
///
/// 定义了在 Session 中运行的任务的基本接口。
#[async_trait]
pub trait SessionTask: Send + Sync + 'static {
    /// 获取任务类型
    fn kind(&self) -> TaskKind;

    /// 运行任务
    ///
    /// # 参数
    /// * `session` - TaskSession 实例（简化的 Session 接口）
    /// * `ctx` - Turn 上下文
    /// * `cancellation_token` - 用于取消任务的令牌
    ///
    /// # 返回
    /// 可选的任务结果字符串
    async fn run(
        self: Arc<Self>,
        session: Arc<dyn TaskSession>,
        ctx: Arc<TurnContext>,
        cancellation_token: CancellationToken,
    ) -> Option<String>;
}

/// RunningTask - 运行中的任务
///
/// 表示当前正在运行的任务实例。
#[derive(Debug)]
pub struct RunningTask {
    pub kind: TaskKind,
    pub cancellation_token: CancellationToken,
    pub turn_context: Arc<TurnContext>,
    pub done: Arc<tokio::sync::Notify>,
}

impl RunningTask {
    /// 创建新的运行中任务
    pub fn new(
        kind: TaskKind,
        cancellation_token: CancellationToken,
        turn_context: Arc<TurnContext>,
    ) -> Self {
        Self {
            kind,
            cancellation_token,
            turn_context,
            done: Arc::new(tokio::sync::Notify::new()),
        }
    }

    /// 标记任务完成
    pub fn mark_done(&self) {
        self.done.notify_one();
    }

    /// 等待任务完成
    pub async fn wait_done(&self) {
        self.done.notified().await;
    }

    /// 取消任务
    pub fn cancel(&self) {
        self.cancellation_token.cancel();
    }

    /// 检查任务是否被取消
    pub fn is_cancelled(&self) -> bool {
        self.cancellation_token.is_cancelled()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::sync::Mutex;

    // Mock TaskSession for testing
    struct MockTaskSession {
        history: Arc<Mutex<crate::session::ConversationHistory>>,
        _event_sender: tokio::sync::mpsc::Sender<crate::protocol::Event>,
    }

    #[async_trait::async_trait]
    impl TaskSession for MockTaskSession {
        async fn history(&self) -> crate::session::ConversationHistory {
            self.history.lock().await.clone()
        }

        async fn compact_history(&self, _keep_recent: usize, _summary: String) {
            // Mock implementation
        }

        async fn emit_event(&self, _event: crate::protocol::Event) {
            // Mock implementation
        }

        async fn push_message(&self, message: crate::model::Message) {
            // Mock implementation - add to history
            let mut history = self.history.lock().await;
            history.push(message);
        }

        async fn undo_last_messages(&self, _num_messages: usize) {
            // Mock implementation
        }
    }

    #[test]
    fn test_task_kind_equality() {
        assert_eq!(TaskKind::Regular, TaskKind::Regular);
        assert_ne!(TaskKind::Regular, TaskKind::Compact);
    }

    #[test]
    fn test_regular_task_kind() {
        let task = RegularTask;
        assert_eq!(task.kind(), TaskKind::Regular);
    }

    #[test]
    fn test_compact_task_kind() {
        let task = CompactTask;
        assert_eq!(task.kind(), TaskKind::Compact);
    }

    #[tokio::test]
    async fn test_running_task_creation() {
        let ctx = Arc::new(TurnContext::default());
        let token = CancellationToken::new();
        let running = RunningTask::new(TaskKind::Regular, token, ctx);

        assert_eq!(running.kind, TaskKind::Regular);
        assert!(!running.is_cancelled());
    }

    #[tokio::test]
    async fn test_running_task_cancel() {
        let ctx = Arc::new(TurnContext::default());
        let token = CancellationToken::new();
        let running = RunningTask::new(TaskKind::Compact, token.clone(), ctx);

        assert!(!running.is_cancelled());
        running.cancel();
        assert!(running.is_cancelled());
    }

    #[tokio::test]
    async fn test_running_task_mark_done() {
        let ctx = Arc::new(TurnContext::default());
        let token = CancellationToken::new();
        let running = RunningTask::new(TaskKind::Undo, token, ctx);

        // Mark done in background
        let done_notify = running.done.clone();
        tokio::spawn(async move {
            done_notify.notify_one();
        });

        // Wait should complete
        running.wait_done().await;
    }

    #[test]
    fn test_submission_creation() {
        let submission = Submission::new("test-id", crate::protocol::Op::Shutdown);
        assert_eq!(submission.id, "test-id");
        assert!(matches!(submission.op, crate::protocol::Op::Shutdown));
    }

    #[tokio::test]
    async fn test_compact_task_run_under_limit() {
        let (tx, _rx) = tokio::sync::mpsc::channel(10);
        let session: Arc<dyn TaskSession> = Arc::new(MockTaskSession {
            history: Arc::new(Mutex::new(crate::session::ConversationHistory::new())),
            _event_sender: tx,
        });
        let ctx = Arc::new(TurnContext {
            auto_compact_token_limit: Some(100000),
            ..Default::default()
        });
        let token = CancellationToken::new();

        let task = Arc::new(CompactTask);
        let result = task.run(session, ctx, token).await;

        // Should return None because token count is under limit
        assert!(result.is_none());
    }
}
