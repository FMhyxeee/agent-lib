use std::collections::VecDeque;
use std::sync::Arc;

use tokio::sync::{Mutex, mpsc};
use tokio_stream::StreamExt;
use tokio_stream::wrappers::ReceiverStream;

use crate::error::{AgentError, AgentResult};
use crate::mcp::McpManager;
use crate::protocol::{ApprovalPolicy, Event, EventQueue, Op, SubmissionQueue};
use crate::session::{ConversationHistory, SessionState};
use crate::tasks::{RunningTask, SessionTask};
use chrono;

/// TaskSession - 任务需要访问的 Session 接口
///
/// 这是一个简化的 Session 接口，只包含任务需要的功能。
#[async_trait::async_trait]
pub trait TaskSession: Send + Sync + 'static {
    async fn history(&self) -> ConversationHistory;
    async fn compact_history(&self, keep_recent: usize, summary: String);
    async fn emit_event(&self, event: Event);

    /// 高效的 token 访问，避免完整克隆
    ///
    /// 这个方法比调用 `history().total_tokens()` 更高效，
    /// 因为它不需要克隆整个 ConversationHistory。
    async fn token_count(&self) -> usize {
        let history = self.history().await;
        history.total_tokens()
    }

    /// 检查是否需要压缩历史
    ///
    /// # 参数
    /// * `limit` - token 限制，超过这个值就需要压缩
    ///
    /// # 返回
    /// 如果当前 token 数超过限制返回 true，否则返回 false
    async fn should_compact(&self, limit: usize) -> bool {
        self.token_count().await > limit
    }

    /// 撤销最后几条消息
    async fn undo_last_messages(&self, num_messages: usize);
}

/// SessionArc - 实现 TaskSession 的 Arc 包装器
#[derive(Clone)]
struct SessionArc {
    history: Arc<Mutex<ConversationHistory>>,
    event_sender: mpsc::Sender<Event>,
}

#[async_trait::async_trait]
impl TaskSession for SessionArc {
    async fn history(&self) -> ConversationHistory {
        self.history.lock().await.clone()
    }

    async fn compact_history(&self, keep_recent: usize, summary: String) {
        let mut history = self.history.lock().await;
        history.compact(keep_recent, summary);

        let _ = self
            .event_sender
            .send(crate::protocol::Event::ContextCompacted {
                compacted_items: vec![],
            })
            .await;
    }

    async fn emit_event(&self, event: Event) {
        let _ = self.event_sender.send(event).await;
    }

    /// 撤销最后几条消息
    async fn undo_last_messages(&self, num_messages: usize) {
        let mut history = self.history.lock().await;
        if num_messages > 0 && history.len() > num_messages {
            // 创建新的历史记录，移除最后 num_messages 条消息
            let new_messages: Vec<crate::model::Message> = history
                .all()
                .iter()
                .take(history.len() - num_messages)
                .cloned()
                .collect();

            // 清空历史并重新添加消息
            history.clear();
            for msg in new_messages {
                history.push(msg);
            }
        }
    }
}

/// Session 配置
#[derive(Debug, Clone)]
pub struct SessionConfig {
    pub queue_buffer: usize,
    pub event_buffer: usize,
    pub default_model: String,
    pub default_cwd: Option<String>,
    pub default_approval_policy: Option<ApprovalPolicy>,
    pub mcp_manager: Option<Arc<McpManager>>,
    pub max_undo_steps: usize,
}

impl Default for SessionConfig {
    fn default() -> Self {
        Self {
            queue_buffer: 64,
            event_buffer: 64,
            default_model: "default".to_string(),
            default_cwd: None,
            default_approval_policy: None,
            mcp_manager: None,
            max_undo_steps: 10,
        }
    }
}

/// ActiveTurn - 当前活动 Turn 的状态
#[derive(Debug)]
struct ActiveTurn {
    tasks: Vec<RunningTask>,
}

#[derive(Debug)]
pub struct Session {
    history: Arc<Mutex<ConversationHistory>>,
    state: Arc<Mutex<SessionState>>,
    submission: SubmissionQueue,
    event_sender: mpsc::Sender<Event>,
    config: SessionConfig,
    active_turn: Arc<Mutex<Option<ActiveTurn>>>,
    undo_stack: Arc<Mutex<VecDeque<UndoSnapshot>>>,
}

#[derive(Clone, Debug)]
#[allow(dead_code)]
struct UndoSnapshot {
    history: ConversationHistory,
    turn_id: String,
    timestamp: i64,
}

#[derive(Clone, Debug)]
pub struct SessionHandle {
    submission: SubmissionQueue,
    event_stream: Arc<Mutex<ReceiverStream<Event>>>,
}

impl Session {
    pub fn new(buffer: usize) -> (Self, SessionHandle) {
        Self::with_config(buffer, SessionConfig::default())
    }

    pub fn with_config(_buffer: usize, config: SessionConfig) -> (Self, SessionHandle) {
        let (op_sender, op_receiver) = mpsc::channel(config.queue_buffer);
        let submission = SubmissionQueue::new(op_sender);

        let (event_sender, event_queue) = EventQueue::new(config.event_buffer);
        let event_stream = event_queue.stream();

        let session = Self {
            history: Arc::new(Mutex::new(ConversationHistory::new())),
            state: Arc::new(Mutex::new(SessionState::Idle)),
            submission,
            event_sender,
            config,
            active_turn: Arc::new(Mutex::new(None)),
            undo_stack: Arc::new(Mutex::new(VecDeque::new())),
        };

        let handle = SessionHandle {
            submission: session.submission.clone(),
            event_stream: Arc::new(Mutex::new(event_stream)),
        };

        tokio::spawn(session_loop(op_receiver, session.event_sender.clone()));

        (session, handle)
    }

    /// 获取对话历史
    pub async fn history(&self) -> ConversationHistory {
        self.history.lock().await.clone()
    }

    /// 高效的 token 访问，避免完整克隆
    ///
    /// 这个方法比调用 `history().total_tokens()` 更高效，
    /// 因为它不需要克隆整个 ConversationHistory。
    pub async fn token_count(&self) -> usize {
        let history = self.history.lock().await;
        history.total_tokens()
    }

    /// 带作用域的只读访问，避免不必要的克隆
    ///
    /// 这个方法允许在不克隆整个 ConversationHistory 的情况下
    /// 对历史执行只读操作，性能更优。
    pub async fn with_history<F, R>(&self, f: F) -> R
    where
        F: FnOnce(&ConversationHistory) -> R,
    {
        let history = self.history.lock().await;
        f(&history)
    }

    /// 检查是否需要压缩历史
    ///
    /// # 参数
    /// * `limit` - token 限制，超过这个值就需要压缩
    ///
    /// # 返回
    /// 如果当前 token 数超过限制返回 true，否则返回 false
    pub async fn should_compact(&self, limit: usize) -> bool {
        self.token_count().await > limit
    }

    /// 获取会话状态
    pub async fn state(&self) -> SessionState {
        self.state.lock().await.clone()
    }

    /// 创建默认的 TurnContext
    pub async fn new_default_turn(&self) -> Arc<crate::session::TurnContext> {
        Arc::new(crate::session::TurnContext {
            sub_id: uuid::Uuid::new_v4().to_string(),
            model: self.config.default_model.clone(),
            cwd: self.config.default_cwd.clone(),
            approval_policy_v2: self.config.default_approval_policy,
            ..Default::default()
        })
    }

    /// 启动新 Task
    ///
    /// 注意：由于 Session 未实现 Clone，此方法会克隆内部字段而不是整个 Session。
    pub async fn spawn_task<T: SessionTask>(
        &self,
        turn_context: Arc<crate::session::TurnContext>,
        task: T,
    ) {
        let cancellation_token = tokio_util::sync::CancellationToken::new();
        let task = Arc::new(task);

        let running_task = RunningTask::new(
            task.kind(),
            cancellation_token.clone(),
            Arc::clone(&turn_context),
        );

        // 添加到活动任务列表
        {
            let mut active = self.active_turn.lock().await;
            if active.is_none() {
                *active = Some(ActiveTurn { tasks: vec![] });
            }
            if let Some(ref mut turn) = *active {
                turn.tasks.push(running_task);
            }
        }

        // 克隆需要的内部字段
        let session_arc: Arc<dyn TaskSession> = Arc::new(SessionArc {
            history: Arc::clone(&self.history),
            event_sender: self.event_sender.clone(),
        });

        tokio::spawn({
            let ctx = Arc::clone(&turn_context);
            let token = cancellation_token.clone();
            let active_turn = Arc::clone(&self.active_turn);
            async move {
                let result = task.run(session_arc, ctx, token).await;

                // 标记任务完成
                {
                    let mut active = active_turn.lock().await;
                    if let Some(ref mut turn) = *active {
                        turn.tasks.retain(|t| !t.cancellation_token.is_cancelled());
                    }
                }

                if let Some(msg) = result {
                    tracing::debug!("Task completed: {}", msg);
                }
            }
        });
    }

    /// 中断所有任务
    pub async fn abort_all_tasks(&self) {
        let mut active = self.active_turn.lock().await;
        if let Some(mut turn) = active.take() {
            for task in turn.tasks.drain(..) {
                task.cancel();
            }
        }
    }

    /// 压缩历史
    pub async fn compact_history(&self, keep_recent: usize, summary: String) {
        let mut history = self.history.lock().await;
        history.compact(keep_recent, summary);

        let _ = self
            .event_sender
            .send(crate::protocol::Event::ContextCompacted {
                compacted_items: vec![],
            })
            .await;
    }

    /// 发送事件
    pub async fn emit_event(&self, event: Event) {
        let _ = self.event_sender.send(event).await;
    }

    /// 获取事件发送器
    pub fn event_sender(&self) -> mpsc::Sender<Event> {
        self.event_sender.clone()
    }
}

impl SessionHandle {
    pub async fn submit(&self, op: Op) -> AgentResult<()> {
        self.submission
            .submit(op)
            .await
            .map_err(|err| AgentError::Session(err.to_string()))
    }

    pub async fn next_event(&self) -> Option<Event> {
        self.event_stream.lock().await.next().await
    }
}

// === Session Builder ===

/// Session 构建器
pub struct SessionBuilder {
    config: SessionConfig,
}

impl SessionBuilder {
    /// 创建一个新的 Session 构建器
    pub fn new() -> Self {
        Self {
            config: SessionConfig::default(),
        }
    }

    /// 设置队列缓冲区大小
    pub fn with_queue_buffer(mut self, buffer: usize) -> Self {
        self.config.queue_buffer = buffer;
        self
    }

    /// 设置事件缓冲区大小
    pub fn with_event_buffer(mut self, buffer: usize) -> Self {
        self.config.event_buffer = buffer;
        self
    }

    /// 设置默认模型
    pub fn with_default_model(mut self, model: impl Into<String>) -> Self {
        self.config.default_model = model.into();
        self
    }

    /// 设置默认工作目录
    pub fn with_default_cwd(mut self, cwd: impl Into<String>) -> Self {
        self.config.default_cwd = Some(cwd.into());
        self
    }

    /// 设置默认批准策略
    pub fn with_default_approval_policy(mut self, policy: ApprovalPolicy) -> Self {
        self.config.default_approval_policy = Some(policy);
        self
    }

    /// 设置 MCP Manager
    pub fn with_mcp_manager(mut self, manager: Arc<McpManager>) -> Self {
        self.config.mcp_manager = Some(manager);
        self
    }

    /// 设置最大撤销步数
    pub fn with_max_undo_steps(mut self, steps: usize) -> Self {
        self.config.max_undo_steps = steps;
        self
    }

    /// 构建 Session
    pub fn build(self) -> (Session, SessionHandle) {
        Session::with_config(0, self.config)
    }
}

impl Default for SessionBuilder {
    fn default() -> Self {
        Self::new()
    }
}

// === Session 方法增强 ===

impl Session {
    /// 获取 MCP Manager
    pub fn get_mcp_manager(&self) -> Option<Arc<McpManager>> {
        self.config.mcp_manager.clone()
    }

    /// 创建快照
    pub async fn create_snapshot(&self) {
        let history = self.history().await;
        let snapshot = UndoSnapshot {
            history,
            turn_id: uuid::Uuid::new_v4().to_string(),
            timestamp: chrono::Utc::now().timestamp(),
        };

        let mut stack = self.undo_stack.lock().await;
        stack.push_back(snapshot);
        if stack.len() > self.config.max_undo_steps {
            stack.pop_front();
        }
    }

    /// 撤销操作
    pub async fn undo(&self) -> AgentResult<()> {
        let mut stack = self.undo_stack.lock().await;
        if let Some(snapshot) = stack.pop_back() {
            let mut history = self.history.lock().await;
            *history = snapshot.history;
            Ok(())
        } else {
            Err(AgentError::Session("No undo history available".to_string()))
        }
    }
}

async fn session_loop(mut op_receiver: mpsc::Receiver<Op>, event_sender: mpsc::Sender<Event>) {
    while let Some(op) = op_receiver.recv().await {
        let _ = event_sender
            .send(Event::TurnStarted {
                turn_id: uuid::Uuid::new_v4().to_string(),
            })
            .await;

        match op {
            Op::StartTurn { prompt, .. } => {
                let _ = event_sender
                    .send(Event::ModelComplete {
                        content: prompt,
                        usage: Default::default(),
                    })
                    .await;
            }
            Op::UserInput { content } => {
                let _ = event_sender
                    .send(Event::ModelStreaming { chunk: content })
                    .await;
            }
            Op::ApprovalResponse { request_id, .. } => {
                let _ = event_sender
                    .send(Event::ToolCallResult {
                        tool: "approval".to_string(),
                        result: crate::tools::ToolResult::text(format!(
                            "approval response: {request_id}"
                        )),
                    })
                    .await;
            }
            Op::Interrupt => {
                let _ = event_sender
                    .send(Event::Error {
                        error: AgentError::Session("session interrupted".to_string()),
                    })
                    .await;
            }
            Op::Handoff { target_agent, .. } => {
                let _ = event_sender
                    .send(Event::HandoffInitiated {
                        from: "session".to_string(),
                        to: target_agent,
                    })
                    .await;
            }
            _ => {
                // 忽略其他 Op
            }
        }
    }
}
