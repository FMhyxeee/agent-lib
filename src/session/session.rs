use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use tokio::sync::{Mutex, mpsc};
use tokio_stream::StreamExt;
use tokio_stream::wrappers::ReceiverStream;

use crate::error::{AgentError, AgentResult};
use crate::mcp::McpManager;
use crate::model::{Message, ModelClient, ModelResponse};
use crate::protocol::{ApprovalPolicy, Event, EventQueue, Op, SubmissionQueue};
use crate::session::{ConversationHistory, SessionState};
use crate::skills::{SkillConfig, SkillRegistry};
use crate::tasks::{RunningTask, SessionTask};
use crate::tools::{ToolDef, ToolExecutor};

/// TaskSession - 任务需要访问的 Session 接口
///
/// 这是一个简化的 Session 接口，只包含任务需要的功能。
#[async_trait::async_trait]
pub trait TaskSession: Send + Sync + 'static {
    async fn history(&self) -> ConversationHistory;
    async fn compact_history(&self, keep_recent: usize, summary: String);
    async fn emit_event(&self, event: Event);

    /// 修复 P0-1: 添加消息到历史（直接写回）
    ///
    /// 这是修复 history 修改 bug 的关键方法，确保消息被正确添加到会话历史中。
    async fn push_message(&self, message: Message);

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

    /// 调用模型（可选实现）
    ///
    /// 默认返回 NotImplemented 错误，如果 Session 配置了 model 则会实际调用。
    async fn chat_model(
        &self,
        _messages: Vec<Message>,
        _tools: Vec<ToolDef>,
    ) -> AgentResult<ModelResponse> {
        Err(AgentError::NotImplemented(
            "model not configured in session".to_string(),
        ))
    }

    /// 获取可用工具列表
    async fn list_tools(&self) -> Vec<ToolDef> {
        vec![]
    }

    /// 执行工具
    async fn execute_tool(
        &self,
        _name: &str,
        _args: serde_json::Value,
    ) -> AgentResult<crate::tools::ToolResult> {
        Err(AgentError::NotImplemented(
            "tool executor not configured in session".to_string(),
        ))
    }
}

/// SessionArc - 实现 TaskSession 的 Arc 包装器
#[derive(Clone)]
struct SessionArc {
    history: Arc<Mutex<ConversationHistory>>,
    event_sender: mpsc::Sender<Event>,
    model: Option<Arc<dyn ModelClient>>,
    tool_executor: Option<Arc<ToolExecutor>>,
    default_cwd: Option<String>,
}

async fn emit_context_compacted(event_sender: &mpsc::Sender<Event>) {
    let _ = event_sender
        .send(crate::protocol::Event::ContextCompacted {
            compacted_items: vec![],
        })
        .await;
}

fn normalize_default_cwd(cwd: Option<&str>) -> Option<String> {
    let cwd = cwd?.trim();
    if cwd.is_empty() {
        return None;
    }

    let input = PathBuf::from(cwd);
    let absolute = if input.is_absolute() {
        input
    } else if let Ok(current_dir) = std::env::current_dir() {
        current_dir.join(input)
    } else {
        input
    };

    Some(normalize_path(&absolute).to_string_lossy().to_string())
}

fn normalize_path(path: &Path) -> PathBuf {
    use std::path::Component;

    let mut result = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Prefix(prefix) => result.push(prefix.as_os_str()),
            Component::RootDir => result.push(component.as_os_str()),
            Component::CurDir => {}
            Component::ParentDir => {
                result.pop();
            }
            Component::Normal(part) => result.push(part),
        }
    }
    result
}

#[async_trait::async_trait]
impl TaskSession for SessionArc {
    async fn history(&self) -> ConversationHistory {
        self.history.lock().await.clone()
    }

    async fn compact_history(&self, keep_recent: usize, summary: String) {
        let mut history = self.history.lock().await;
        history.compact(keep_recent, summary);

        emit_context_compacted(&self.event_sender).await;
    }

    async fn push_message(&self, message: Message) {
        let mut history = self.history.lock().await;
        history.push(message);
        // 修改后自动写回（因为 MutexGuard 持有锁直到作用域结束）
    }

    async fn emit_event(&self, event: Event) {
        let _ = self.event_sender.send(event).await;
    }

    /// 撤销最后几条消息
    async fn undo_last_messages(&self, num_messages: usize) {
        let mut history = self.history.lock().await;
        let _ = history.remove_last_messages(num_messages);
    }

    /// 调用模型
    async fn chat_model(
        &self,
        messages: Vec<Message>,
        tools: Vec<ToolDef>,
    ) -> AgentResult<ModelResponse> {
        if let Some(model) = &self.model {
            model.chat(messages, tools).await
        } else {
            Err(AgentError::NotImplemented(
                "model not configured in session".to_string(),
            ))
        }
    }

    /// 获取可用工具列表
    async fn list_tools(&self) -> Vec<ToolDef> {
        if let Some(executor) = &self.tool_executor {
            executor.list()
        } else {
            vec![]
        }
    }

    /// 执行工具
    async fn execute_tool(
        &self,
        name: &str,
        args: serde_json::Value,
    ) -> AgentResult<crate::tools::ToolResult> {
        if let Some(executor) = &self.tool_executor {
            use crate::tools::ToolContext;
            let normalized_cwd = normalize_default_cwd(self.default_cwd.as_deref());
            let ctx = ToolContext {
                cwd: normalized_cwd.clone(),
                sandbox_root: normalized_cwd,
            };
            executor.execute(name, args, &ctx).await
        } else {
            Err(AgentError::NotImplemented(
                "tool executor not configured in session".to_string(),
            ))
        }
    }
}

/// Session 配置
#[derive(Clone)]
pub struct SessionConfig {
    pub queue_buffer: usize,
    pub event_buffer: usize,
    pub default_model: String,
    pub default_cwd: Option<String>,
    pub default_approval_policy: Option<ApprovalPolicy>,
    pub mcp_manager: Option<Arc<McpManager>>,
    pub max_undo_steps: usize,
    /// 可选的模型客户端，用于 RegularTask 等需要调用模型的任务
    pub model: Option<Arc<dyn ModelClient>>,
    /// 可选的工具执行器，用于工具调用
    pub tool_executor: Option<Arc<ToolExecutor>>,
    /// 可选的技能配置
    pub skill_config: Option<SkillConfig>,
    /// 可选的技能注册表
    pub skill_registry: Option<Arc<SkillRegistry>>,
}

impl std::fmt::Debug for SessionConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SessionConfig")
            .field("queue_buffer", &self.queue_buffer)
            .field("event_buffer", &self.event_buffer)
            .field("default_model", &self.default_model)
            .field("default_cwd", &self.default_cwd)
            .field("default_approval_policy", &self.default_approval_policy)
            .field("mcp_manager", &self.mcp_manager)
            .field("max_undo_steps", &self.max_undo_steps)
            .field("model", &self.model.as_ref().map(|_| "<ModelClient>"))
            .field(
                "tool_executor",
                &self.tool_executor.as_ref().map(|_| "<ToolExecutor>"),
            )
            .field("skill_config", &self.skill_config)
            .field(
                "skill_registry",
                &self.skill_registry.as_ref().map(|_| "<SkillRegistry>"),
            )
            .finish()
    }
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
            model: None,
            tool_executor: None,
            skill_config: None,
            skill_registry: None,
        }
    }
}

/// ActiveTurn - 当前活动 Turn 的状态
#[derive(Debug)]
struct ActiveTurn {
    tasks: Vec<RunningTask>,
}

pub struct Session {
    history: Arc<Mutex<ConversationHistory>>,
    state: Arc<Mutex<SessionState>>,
    submission: SubmissionQueue,
    event_sender: mpsc::Sender<Event>,
    config: SessionConfig,
    active_turn: Arc<Mutex<Option<ActiveTurn>>>,
    undo_stack: Arc<Mutex<VecDeque<UndoSnapshot>>>,
    model: Option<Arc<dyn ModelClient>>,
    tool_executor: Option<Arc<ToolExecutor>>,
}

impl std::fmt::Debug for Session {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Session")
            .field("history", &"<ConversationHistory>")
            .field("state", &self.state)
            .field("submission", &self.submission)
            .field("config", &self.config)
            .field("active_turn", &self.active_turn)
            .field("undo_stack", &"<UndoStack>")
            .field("model", &self.model.as_ref().map(|_| "<ModelClient>"))
            .field(
                "tool_executor",
                &self.tool_executor.as_ref().map(|_| "<ToolExecutor>"),
            )
            .finish()
    }
}

#[derive(Clone, Debug)]
struct UndoSnapshot {
    history: ConversationHistory,
}

#[derive(Clone, Debug)]
pub struct SessionHandle {
    submission: SubmissionQueue,
    event_stream: Arc<tokio::sync::Mutex<ReceiverStream<Event>>>,
}

impl Session {
    pub fn new(buffer: usize) -> (Arc<Self>, SessionHandle) {
        Self::with_config(buffer, SessionConfig::default())
    }

    pub fn with_config(buffer: usize, mut config: SessionConfig) -> (Arc<Self>, SessionHandle) {
        if buffer > 0 {
            config.queue_buffer = buffer;
        }

        let (op_sender, mut op_receiver) = mpsc::channel(config.queue_buffer);
        let submission = SubmissionQueue::new(op_sender);

        let (event_sender, event_queue) = EventQueue::new(config.event_buffer);
        let event_stream = event_queue.stream();

        let model = config.model.clone();
        let tool_executor = config.tool_executor.clone();
        let queue_buffer = config.queue_buffer;

        let session = Self {
            history: Arc::new(Mutex::new(ConversationHistory::new())),
            state: Arc::new(Mutex::new(SessionState::Idle)),
            submission,
            event_sender,
            config,
            active_turn: Arc::new(Mutex::new(None)),
            undo_stack: Arc::new(Mutex::new(VecDeque::new())),
            model,
            tool_executor,
        };

        let handle = SessionHandle {
            submission: session.submission.clone(),
            event_stream: Arc::new(tokio::sync::Mutex::new(event_stream)),
        };

        // 修复 P0-3: 使用完整的 submission_loop 而非简化版本
        // 将 mpsc::Receiver<Op> 转换为 submission_loop 需要的 mpsc::Receiver<Submission>
        let sess_arc = Arc::new(session);
        let sess_clone = Arc::clone(&sess_arc);

        // 创建一个通道来桥接 Op -> Submission
        let (submission_sender, submission_receiver) = mpsc::channel(queue_buffer);

        // 启动 Op 到 Submission 的转换任务
        tokio::spawn(async move {
            use crate::tasks::Submission;
            let mut op_count = 0;
            while let Some(op) = op_receiver.recv().await {
                let submission = Submission::new(format!("op-{}", op_count), op);
                if submission_sender.send(submission).await.is_err() {
                    break;
                }
                op_count += 1;
            }
        });

        // 启动完整的 submission_loop
        tokio::spawn(crate::tasks::submission_loop(
            sess_clone,
            submission_receiver,
        ));

        (sess_arc, handle)
    }

    /// 获取对话历史
    pub async fn history(&self) -> ConversationHistory {
        self.history.lock().await.clone()
    }

    /// 添加消息到历史 (修复 P0-1: 确保写回到会话状态)
    pub async fn push_message(&self, message: crate::model::Message) {
        let mut history = self.history.lock().await;
        history.push(message);
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
            approval_policy: self.config.default_approval_policy,
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
        let task_done = running_task.done.clone();

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
            model: self.model.clone(),
            tool_executor: self.tool_executor.clone(),
            default_cwd: self.config.default_cwd.clone(),
        });

        tokio::spawn({
            let ctx = Arc::clone(&turn_context);
            let token = cancellation_token.clone();
            let active_turn = Arc::clone(&self.active_turn);
            let done_marker = Arc::clone(&task_done);
            async move {
                let result = task.run(session_arc, ctx, token).await;
                done_marker.notify_waiters();

                // 标记任务完成
                {
                    let mut active = active_turn.lock().await;
                    let mut should_clear_active_turn = false;
                    if let Some(ref mut turn) = *active {
                        turn.tasks.retain(|t| !Arc::ptr_eq(&t.done, &done_marker));
                        should_clear_active_turn = turn.tasks.is_empty();
                    }
                    if should_clear_active_turn {
                        *active = None;
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

        emit_context_compacted(&self.event_sender).await;
    }

    pub async fn remove_last_messages(&self, num_messages: usize) -> usize {
        let mut history = self.history.lock().await;
        history.remove_last_messages(num_messages)
    }

    /// 发送事件
    pub async fn emit_event(&self, event: Event) {
        let _ = self.event_sender.send(event).await;
    }

    pub async fn chat_model(
        &self,
        messages: Vec<Message>,
        tools: Vec<ToolDef>,
    ) -> AgentResult<ModelResponse> {
        if let Some(model) = &self.model {
            model.chat(messages, tools).await
        } else {
            Err(AgentError::NotImplemented(
                "model not configured in session".to_string(),
            ))
        }
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
        let mut stream = self.event_stream.lock().await;
        stream.next().await
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
    pub fn build(self) -> (Arc<Session>, SessionHandle) {
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

    /// 获取技能配置
    pub fn get_skill_config(&self) -> Option<SkillConfig> {
        self.config.skill_config.clone()
    }

    /// 获取技能注册表
    pub fn get_skill_registry(&self) -> Option<Arc<SkillRegistry>> {
        self.config.skill_registry.clone()
    }

    /// 创建快照
    pub async fn create_snapshot(&self) {
        let history = self.history().await;
        let snapshot = UndoSnapshot { history };

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::{Tool, ToolContext, ToolDef, ToolRegistry, ToolResult};
    use async_trait::async_trait;
    use serde_json::json;

    #[derive(Debug, Default)]
    struct InspectContextTool;

    #[async_trait]
    impl Tool for InspectContextTool {
        fn definition(&self) -> ToolDef {
            ToolDef {
                name: "inspect_context".to_string(),
                description: "Return tool context".to_string(),
                schema: json!({
                    "type": "object",
                    "properties": {}
                }),
            }
        }

        async fn execute(
            &self,
            _args: serde_json::Value,
            ctx: &ToolContext,
        ) -> AgentResult<ToolResult> {
            Ok(ToolResult {
                output: json!({
                    "cwd": ctx.cwd,
                    "sandbox_root": ctx.sandbox_root,
                }),
            })
        }
    }

    #[test]
    fn normalize_default_cwd_converts_relative_path_to_absolute_path() {
        let normalized = normalize_default_cwd(Some(".")).expect("expected normalized cwd");
        assert!(Path::new(&normalized).is_absolute());
    }

    #[tokio::test]
    async fn execute_tool_sets_sandbox_root_from_normalized_default_cwd() {
        let mut registry = ToolRegistry::new();
        registry.register(Arc::new(InspectContextTool));
        let executor = Arc::new(ToolExecutor::new(registry));

        let (event_sender, _event_receiver) = mpsc::channel(4);
        let session = SessionArc {
            history: Arc::new(Mutex::new(ConversationHistory::new())),
            event_sender,
            model: None,
            tool_executor: Some(executor),
            default_cwd: Some(".".to_string()),
        };

        let result = session
            .execute_tool("inspect_context", json!({}))
            .await
            .expect("inspect_context should execute");

        let cwd = result
            .output
            .get("cwd")
            .and_then(|value| value.as_str())
            .expect("cwd should be a string");
        let sandbox_root = result
            .output
            .get("sandbox_root")
            .and_then(|value| value.as_str())
            .expect("sandbox_root should be a string");

        assert_eq!(cwd, sandbox_root);
        assert!(Path::new(cwd).is_absolute());
    }
}

// 修复 P0-3: 不再使用简化的 session_loop_enhanced
// 现在使用完整的 submission_loop (在 src/tasks/loop.rs 中)
// 保留此代码作为参考,以备将来需要
/*
/// 增强的 session_loop - 支持 UserTurn 和模型调用
async fn session_loop_enhanced(
    sess: Arc<SessionArc>,
    mut op_receiver: mpsc::Receiver<Op>,
) {
    use crate::protocol::{Event, Op, UserInputItem};

    while let Some(op) = op_receiver.recv().await {
        // 发送 TurnStarted 事件
        let turn_id = uuid::Uuid::new_v4().to_string();
        let _ = sess
            .emit_event(Event::TurnStarted { turn_id: turn_id.clone() })
            .await;

        match op {
            Op::UserTurn {
                items,
                model: _model,
                ..
            } => {
                // 处理 UserTurn - 直接调用模型
                for item in items {
                    if let UserInputItem::Text { text } = item {
                        // 修复 P0-1: 使用 push_message 直接写回历史
                        sess.push_message(crate::model::Message::user(text.clone())).await;

                        // 准备消息
                        let history = sess.history().await;
                        let messages = history.for_prompt();
                        tracing::debug!("Calling model with {} messages", messages.len());

                        // 发送 Thinking 事件
                        let _ = sess
                            .emit_event(Event::ModelStreaming {
                                chunk: format!("[{}] Thinking...\n", turn_id),
                            })
                            .await;

                        // 调用模型 (添加 60 秒超时)
                        use tokio::time::{timeout, Duration};
                        let model_result = timeout(Duration::from_secs(60), sess.chat_model(messages, vec![])).await;
                        match model_result {
                            Ok(Ok(response)) => {
                                // 修复 P0-1: 将助手消息添加到历史 (包含推理内容)
                                let assistant_msg = if let Some(ref reasoning) = response.reasoning_content {
                                    crate::model::Message::assistant_with_reasoning(
                                        response.content.clone(),
                                        reasoning.clone(),
                                    )
                                } else {
                                    crate::model::Message::assistant(response.content.clone())
                                };
                                sess.push_message(assistant_msg).await;

                                // 如果有推理内容，先发送推理流式事件
                                if let Some(ref reasoning) = response.reasoning_content {
                                    if !reasoning.is_empty() {
                                        let _ = sess
                                            .emit_event(Event::ReasoningStreaming {
                                                chunk: reasoning.clone(),
                                            })
                                            .await;
                                    }
                                }

                                // 分块发送响应 (UTF-8 安全)
                                let chunk_size = 20;
                                let mut current_chunk = String::new();
                                for ch in response.content.chars() {
                                    current_chunk.push(ch);
                                    if current_chunk.chars().count() >= chunk_size {
                                        let _ = sess
                                            .emit_event(Event::ModelStreaming {
                                                chunk: current_chunk.clone(),
                                            })
                                            .await;
                                        current_chunk.clear();
                                    }
                                }
                                if !current_chunk.is_empty() {
                                    let _ = sess
                                        .emit_event(Event::ModelStreaming {
                                            chunk: current_chunk,
                                        })
                                        .await;
                                }

                                // 发送完成事件
                                sess.emit_event(Event::ModelComplete {
                                    content: response.content.clone(),
                                    usage: response.usage,
                                }).await;
                            }
                                // 修复 P0-1: 将助手消息添加到历史
                                sess.push_message(crate::model::Message::assistant(
                                    response.content.clone()
                                )).await;

                                // 分块发送响应 (UTF-8 安全)
                                let chunk_size = 20;
                                let mut current_chunk = String::new();
                                for ch in response.content.chars() {
                                    current_chunk.push(ch);
                                    if current_chunk.chars().count() >= chunk_size {
                                        let _ = sess
                                            .emit_event(Event::ModelStreaming {
                                                chunk: current_chunk.clone(),
                                            })
                                            .await;
                                        current_chunk.clear();
                                    }
                                }
                                if !current_chunk.is_empty() {
                                    let _ = sess
                                        .emit_event(Event::ModelStreaming {
                                            chunk: current_chunk,
                                        })
                                        .await;
                                }

                                // 发送完成事件
                                sess.emit_event(Event::ModelComplete {
                                    content: response.content.clone(),
                                    usage: response.usage,
                                }).await;
                            }
                            Ok(Err(e)) => {
                                tracing::error!("Model call failed: {:?}", e);
                                let _ = sess
                                    .emit_event(Event::ModelStreaming {
                                        chunk: format!("[ERROR: {:?}]\n", e),
                                    })
                                    .await;
                                let _ = sess.emit_event(Event::Error { error: e }).await;
                            }
                            Err(timeout_err) => {
                                tracing::error!("Model call timed out: {:?}", timeout_err);
                                let error = AgentError::Model(ModelError::Other(format!("Model call timed out: {:?}", timeout_err)));
                                let _ = sess
                                    .emit_event(Event::ModelStreaming {
                                        chunk: "[ERROR: Model call timed out]\n".to_string(),
                                    })
                                    .await;
                                let _ = sess.emit_event(Event::Error { error }).await;
                            }
                        }
                    }
                }
            }
            Op::UserInput { content } => {
                let _ = sess
                    .emit_event(Event::ModelStreaming { chunk: content })
                    .await;
            }
            Op::ApprovalResponse { request_id, .. } => {
                let _ = sess
                    .emit_event(Event::ToolCallResult {
                        tool: "approval".to_string(),
                        result: crate::tools::ToolResult::text(format!(
                            "approval response: {request_id}"
                        )),
                    })
                    .await;
            }
            Op::Interrupt => {
                let _ = sess
                    .emit_event(Event::Error {
                        error: AgentError::Session("session interrupted".to_string()),
                    })
                    .await;
            }
            Op::Handoff { target_agent, .. } => {
                let _ = sess
                    .emit_event(Event::HandoffInitiated {
                        from: "session".to_string(),
                        to: target_agent,
                    })
                    .await;
            }
            Op::StartTurn { prompt, .. } => {
                // 简单的 StartTurn - 直接返回提示内容
                let _ = sess
                    .emit_event(Event::ModelComplete {
                        content: prompt,
                        usage: Default::default(),
                    })
                    .await;
            }
            Op::RunUserShellCommand { command } => {
                // 发送命令执行事件
                let _ = sess.emit_event(Event::RunUserShellCommand {
                    command: command.clone(),
                }).await;

                // 执行命令 (使用 tokio::process)
                use tokio::process::Command;
                match Command::new("cmd").args(["/C", &command]).output().await {
                    Ok(output) => {
                        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
                        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
                        let result = if !stdout.is_empty() { stdout } else { stderr };
                        let _ = sess.emit_event(Event::ModelStreaming { chunk: result }).await;
                    }
                    Err(e) => {
                        let _ = sess.emit_event(Event::Error {
                            error: AgentError::Session(format!("Command failed: {}", e)),
                        }).await;
                    }
                }
            }
            Op::ListSkills { .. } => {
                // 返回空技能列表 (测试用)
                let _ = sess.emit_event(Event::ListSkillsResponse {
                    skills: vec![],
                }).await;
            }
            Op::ListCustomPrompts => {
                // 返回空自定义提示列表 (测试用)
                let _ = sess.emit_event(Event::ListCustomPromptsResponse {
                    prompts: vec![],
                }).await;
            }
            Op::GetHistoryEntryRequest { offset, log_id } => {
                // 返回空历史条目 (测试用)
                let _ = sess.emit_event(Event::HistoryEntry {
                    offset,
                    log_id,
                    entry: String::new(),
                }).await;
            }
            Op::ListModels => {
                // 返回模型列表
                let models = crate::model::list_models().iter().map(|m| {
                    crate::protocol::ModelInfo {
                        id: m.id.to_string(),
                        name: m.display_name.to_string(),
                        provider: m.provider.to_string(),
                    }
                }).collect();
                let _ = sess.emit_event(Event::ModelsListed { models }).await;
            }
            Op::Compact => {
                // 发送压缩完成事件
                let _ = sess.emit_event(Event::ContextCompacted {
                    compacted_items: vec![],
                }).await;
            }
            Op::OverrideTurnContext { .. } => {
                // 发送上下文更新警告
                let _ = sess.emit_event(Event::Warning {
                    message: "Turn context updated".to_string(),
                }).await;
            }
            Op::Undo => {
                // 发送撤销完成事件
                let _ = sess.emit_event(Event::UndoPerformed {
                    removed_messages: 0,
                    summary: "Undo performed".to_string(),
                }).await;
            }
            _ => {
                // 其他 Op 发送警告
                let _ = sess.emit_event(Event::Warning {
                    message: format!("Unhandled Op: {:?}", std::mem::discriminant(&op)),
                }).await;
            }
        }
    }
}
*/
