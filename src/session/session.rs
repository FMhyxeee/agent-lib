use std::sync::Arc;

use tokio::sync::{Mutex, mpsc};
use tokio_stream::StreamExt;
use tokio_stream::wrappers::ReceiverStream;

use crate::error::{AgentError, AgentResult};
use crate::protocol::{ApprovalPolicy, Event, EventQueue, Op, SubmissionQueue};
use crate::session::{ConversationHistory, SessionState};
use crate::tasks::{RunningTask, SessionTask};

/// TaskSession - 任务需要访问的 Session 接口
///
/// 这是一个简化的 Session 接口，只包含任务需要的功能。
#[async_trait::async_trait]
pub trait TaskSession: Send + Sync {
    async fn history(&self) -> ConversationHistory;
    async fn compact_history(&self, keep_recent: usize, summary: String);
    async fn emit_event(&self, event: Event);
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
}

/// Session 配置
#[derive(Debug, Clone)]
pub struct SessionConfig {
    pub queue_buffer: usize,
    pub event_buffer: usize,
    pub default_model: String,
    pub default_cwd: Option<String>,
    pub default_approval_policy: Option<ApprovalPolicy>,
}

impl Default for SessionConfig {
    fn default() -> Self {
        Self {
            queue_buffer: 64,
            event_buffer: 64,
            default_model: "default".to_string(),
            default_cwd: None,
            default_approval_policy: None,
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
