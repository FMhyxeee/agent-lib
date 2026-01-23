# Codex 核心功能移植计划

## 目标

将 OpenAI Codex 的三个核心功能移植到 agent-lib 项目：
1. **submission_loop 核心事件循环** - 统一处理所有 Op 的入口
2. **Session/Task/Turn 三层结构管理** - 清晰的任务层次划分
3. **Token 管理与自动压缩** - 智能历史压缩

## 设计原则

- **完全兼容 Codex Op 枚举** - 支持所有 Codex 的操作类型
- **支持多 Task 并行** - 可同时运行多个任务
- **精确 Token 计数** - 使用 tiktoken-rs
- **向后兼容** - 现有 API 继续工作
- **每个阶段完成后必须通过单元测试** - 不通过不进入下一阶段

---

## 阶段 1: Op/Event 枚举扩展 + TurnContext 增强

### 目标
扩展通信协议以兼容 Codex 的所有操作类型

### 文件修改

#### 1.1 `src/protocol/op.rs`

新增 Codex 兼容的 Op 变体：

```rust
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::PathBuf;

use crate::session::TurnContext;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Op {
    // === 现有 Op (保持兼容) ===
    StartTurn { prompt: String, context: TurnContext },
    UserInput { content: String },
    ApprovalResponse { request_id: String, approved: bool },
    Interrupt,
    Handoff { target_agent: String, context: Value },

    // === 新增 Codex 兼容 Op ===
    UserTurn {
        items: Vec<UserInputItem>,
        cwd: PathBuf,
        approval_policy: ApprovalPolicy,
        sandbox_policy: SandboxPolicy,
        model: String,
        effort: Option<ReasoningEffort>,
        summary: ReasoningSummary,
        final_output_json_schema: Option<Value>,
        collaboration_mode: Option<CollaborationMode>,
    },

    UserInputLegacy {
        items: Vec<UserInputItem>,
        final_output_json_schema: Option<Value>,
    },

    OverrideTurnContext {
        cwd: Option<PathBuf>,
        approval_policy: Option<ApprovalPolicy>,
        sandbox_policy: Option<SandboxPolicy>,
        model: Option<String>,
        effort: Option<Option<ReasoningEffort>>,
        summary: Option<ReasoningSummary>,
        collaboration_mode: Option<CollaborationMode>,
    },

    ExecApproval { id: String, decision: ReviewDecision },
    PatchApproval { id: String, decision: ReviewDecision },
    UserInputAnswer { id: String, response: UserInputResponse },
    AddToHistory { text: String },
    GetHistoryEntryRequest { offset: usize, log_id: u64 },
    ListMcpTools,
    RefreshMcpServers { config: McpServerRefreshConfig },
    ListCustomPrompts,
    ListSkills { cwds: Vec<PathBuf>, force_reload: bool },
    Undo,
    Compact,
    ThreadRollback { num_turns: u32 },
    Review { review_request: ReviewRequest },
    Shutdown,
    RunUserShellCommand { command: String },
    ListModels,
}

// === 新增支持类型 ===

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ApprovalPolicy {
    AlwaysAsk,
    ReadOnlySafe,
    NeverAsk,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SandboxPolicy {
    Readonly,
    Persistent,
    InMemory,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ReasoningEffort {
    Low,
    Medium,
    High,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReasoningSummary {
    pub enabled: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_length: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CollaborationMode {
    Custom {
        model: Option<String>,
        reasoning_effort: Option<ReasoningEffort>,
        developer_instructions: Option<String>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ReviewDecision {
    Approve,
    Deny,
    ApproveWithEdits { edits: Value },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserInputItem {
    #[serde(rename = "type")]
    pub item_type: String,
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub range: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerRefreshConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub args: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserInputResponse {
    pub response: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReviewRequest {
    pub diff: String,
}
```

#### 1.2 `src/protocol/event.rs`

```rust
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio_stream::wrappers::ReceiverStream;

use crate::error::AgentError;
use crate::model::TokenUsage;
use crate::tools::ToolResult;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Event {
    // === 现有 Event ===
    TurnStarted { turn_id: String },
    ModelStreaming { chunk: String },
    ModelComplete { content: String, usage: TokenUsage },
    ToolCallRequested { tool: String, args: Value },
    ToolCallResult { tool: String, result: ToolResult },
    ApprovalRequired { request_id: String, tool: String, args: Value },
    HandoffInitiated { from: String, to: String },
    TurnComplete { result: Value },
    Error { error: AgentError },

    // === 新增 Event ===
    SessionConfigured {
        rollout_path: String,
        thread_id: String,
    },
    TurnAborted {
        reason: TurnAbortReason,
    },
    ContextCompacted {
        compacted_items: Vec<CompactedItem>,
    },
    Warning {
        message: String,
    },
    McpListToolsResponse {
        tools: Vec<McpToolInfo>,
    },
    ListCustomPromptsResponse {
        prompts: Vec<CustomPromptInfo>,
    },
    ListSkillsResponse {
        skills: Vec<SkillEntry>,
    },
    ThreadRolledBack {
        num_turns: u32,
    },
    HistoryEntry {
        offset: usize,
        log_id: u64,
        entry: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TurnAbortReason {
    Interrupted,
    Replaced,
    Shutdown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompactedItem {
    pub turn_id: String,
    pub summary: String,
    pub token_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpToolInfo {
    pub server: String,
    pub name: String,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomPromptInfo {
    pub id: String,
    pub name: String,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillEntry {
    pub path: String,
    pub display_name: String,
    pub short_description: String,
    pub enabled: bool,
}

pub type EventStream = ReceiverStream<Event>;
```

#### 1.3 `src/session/context.rs`

```rust
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TurnContext {
    // === 现有字段 ===
    pub model: String,
    pub sandbox: Option<String>,
    pub cwd: Option<String>,
    pub approval_policy: Option<String>,

    // === 新增字段 ===
    pub sub_id: String,
    pub approval_policy_v2: Option<ApprovalPolicy>,
    pub sandbox_policy: Option<SandboxPolicy>,
    pub collaboration_mode: Option<CollaborationMode>,
    pub reasoning_effort: Option<ReasoningEffort>,
    pub reasoning_summary: Option<ReasoningSummary>,
    pub user_instructions: Option<String>,
    pub developer_instructions: Option<String>,
    pub final_output_json_schema: Option<serde_json::Value>,
    pub truncation_policy: Option<TruncationPolicy>,
    pub auto_compact_token_limit: Option<i64>,
    pub context_window: usize,
}

// 重新导出新类型
pub use crate::protocol::{
    ApprovalPolicy, SandboxPolicy, ReasoningEffort, ReasoningSummary,
    CollaborationMode, TruncationPolicy,
};

impl Default for TurnContext {
    fn default() -> Self {
        Self {
            model: "default".to_string(),
            sandbox: None,
            cwd: None,
            approval_policy: None,
            sub_id: uuid::Uuid::new_v4().to_string(),
            approval_policy_v2: None,
            sandbox_policy: None,
            collaboration_mode: None,
            reasoning_effort: None,
            reasoning_summary: None,
            user_instructions: None,
            developer_instructions: None,
            final_output_json_schema: None,
            truncation_policy: None,
            auto_compact_token_limit: None,
            context_window: 128000,
        }
    }
}
```

### 单元测试（阶段 1）

创建 `src/protocol/op_tests.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json;

    #[test]
    fn test_op_serialize_start_turn() {
        let op = Op::StartTurn {
            prompt: "test".to_string(),
            context: TurnContext::default(),
        };
        let json = serde_json::to_string(&op).unwrap();
        assert!(json.contains("StartTurn"));
    }

    #[test]
    fn test_op_serialize_user_turn() {
        let op = Op::UserTurn {
            items: vec![],
            cwd: PathBuf::from("/tmp"),
            approval_policy: ApprovalPolicy::AlwaysAsk,
            sandbox_policy: SandboxPolicy::Persistent,
            model: "gpt-4".to_string(),
            effort: None,
            summary: ReasoningSummary { enabled: false, max_length: None },
            final_output_json_schema: None,
            collaboration_mode: None,
        };
        let json = serde_json::to_string(&op).unwrap();
        assert!(json.contains("user-turn"));
    }

    #[test]
    fn test_op_deserialize() {
        let json = r#"{"type":"interrupt"}"#;
        let op: Op = serde_json::from_str(json).unwrap();
        assert!(matches!(op, Op::Interrupt));
    }

    #[test]
    fn test_approval_policy_serialize() {
        let policy = ApprovalPolicy::AlwaysAsk;
        let json = serde_json::to_string(&policy).unwrap();
        assert_eq!(json, r#""always-ask""#);
    }
}
```

创建 `src/protocol/event_tests.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_event_turn_started() {
        let event = Event::TurnStarted {
            turn_id: "test-id".to_string(),
        };
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("TurnStarted"));
    }

    #[test]
    fn test_event_context_compacted() {
        let event = Event::ContextCompacted {
            compacted_items: vec![CompactedItem {
                turn_id: "turn-1".to_string(),
                summary: "summary".to_string(),
                token_count: 100,
            }],
        };
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("ContextCompacted"));
    }
}
```

### 验收标准（阶段 1）
- [ ] 所有 Op 变体可以正常序列化/反序列化
- [ ] 所有 Event 变体可以正常序列化/反序列化
- [ ] TurnContext 包含所有新字段
- [ ] 单元测试全部通过
- [ ] `cargo test` 通过

---

## 阶段 2: Token 管理模块

### 目标
实现 Token 计数和 TruncationPolicy

### 新增文件

#### 2.1 `src/token/mod.rs`

```rust
mod counter;
mod policy;

pub use counter::{approx_token_count, tiktoken_count, TokenCounter};
pub use policy::{TruncationMode, TruncationPolicy};
```

#### 2.2 `src/token/counter.rs`

```rust
/// Token 计数器
#[derive(Clone, Default)]
pub struct TokenCounter {
    use_tiktoken: bool,
}

impl TokenCounter {
    pub fn new(use_tiktoken: bool) -> Self {
        Self { use_tiktoken }
    }

    pub fn count(&self, text: &str) -> usize {
        if self.use_tiktoken {
            tiktoken_count(text)
        } else {
            approx_token_count(text)
        }
    }
}

/// 粗略的 token 计数 (Codex 原版: ~4 bytes/token)
pub fn approx_token_count(text: &str) -> usize {
    const APPROX_BYTES_PER_TOKEN: usize = 4;
    let len = text.len();
    len.saturating_add(APPROX_BYTES_PER_TOKEN.saturating_sub(1)) / APPROX_BYTES_PER_TOKEN
}

/// 精确的 token 计数 (使用 tiktoken-rs)
pub fn tiktoken_count(text: &str) -> usize {
    // TODO: 阶段 2.5 集成 tiktoken-rs
    // 临时使用近似值
    approx_token_count(text)
}
```

#### 2.3 `src/token/policy.rs`

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TruncationPolicy {
    pub mode: TruncationMode,
    pub limit: u64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TruncationMode {
    Bytes,
    Tokens,
}

impl TruncationPolicy {
    pub fn token_budget(&self) -> usize {
        match self.mode {
            TruncationMode::Tokens(tokens) => tokens as usize,
            TruncationMode::Bytes(bytes) => (bytes / 4) as usize,
        }
    }

    pub fn byte_budget(&self) -> usize {
        match self.mode {
            TruncationMode::Tokens(tokens) => (tokens * 4) as usize,
            TruncationMode::Bytes(bytes) => bytes as usize,
        }
    }
}

impl Default for TruncationPolicy {
    fn default() -> Self {
        Self {
            mode: TruncationMode::Tokens,
            limit: 128000,
        }
    }
}
```

### 修改文件

#### 2.4 `src/lib.rs`

添加模块导出：

```rust
pub mod token;
```

### 单元测试（阶段 2）

创建 `src/token/counter_tests.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_approx_token_count_empty() {
        assert_eq!(approx_token_count(""), 0);
    }

    #[test]
    fn test_approx_token_count_short() {
        // "hello world" = 11 bytes ≈ 3 tokens
        let count = approx_token_count("hello world");
        assert!(count == 3);
    }

    #[test]
    fn test_approx_token_count_long() {
        let text = "a".repeat(4000);
        assert_eq!(approx_token_count(&text), 1000);
    }

    #[test]
    fn test_token_counter_approx() {
        let counter = TokenCounter::new(false);
        assert_eq!(counter.count("hello"), 2);
    }

    #[test]
    fn test_token_counter_tiktoken() {
        let counter = TokenCounter::new(true);
        // tiktoken 暂未实现，使用近似值
        assert_eq!(counter.count("hello"), 2);
    }
}
```

创建 `src/token/policy_tests.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_truncation_mode_tokens() {
        let policy = TruncationPolicy {
            mode: TruncationMode::Tokens,
            limit: 1000,
        };
        assert_eq!(policy.token_budget(), 1000);
        assert_eq!(policy.byte_budget(), 4000);
    }

    #[test]
    fn test_truncation_mode_bytes() {
        let policy = TruncationPolicy {
            mode: TruncationMode::Bytes,
            limit: 4000,
        };
        assert_eq!(policy.token_budget(), 1000);
        assert_eq!(policy.byte_budget(), 4000);
    }

    #[test]
    fn test_truncation_policy_default() {
        let policy = TruncationPolicy::default();
        assert_eq!(policy.limit, 128000);
    }

    #[test]
    fn test_truncation_policy_serialize() {
        let policy = TruncationPolicy {
            mode: TruncationMode::Tokens,
            limit: 1000,
        };
        let json = serde_json::to_string(&policy).unwrap();
        assert!(json.contains("tokens"));
        assert!(json.contains("1000"));
    }
}
```

### 验收标准（阶段 2）
- [ ] TokenCounter 可以正确计算 token 数
- [ ] TruncationPolicy 可以正确计算预算
- [ ] 单元测试全部通过
- [ ] `cargo test` 通过

---

## 阶段 3: Tasks 模块骨架

### 目标
创建 SessionTask trait 和基础 Task 类型

### 新增文件

#### 3.1 `src/tasks/mod.rs`

```rust
mod regular;
mod compact;
mod r#loop;

pub use regular::RegularTask;
pub use compact::CompactTask;
pub use r#loop::{submission_loop, Submission};

use async_trait::async_trait;
use std::sync::Arc;
use tokio_util::sync::CancellationToken;

use crate::session::{Session, TurnContext};

/// Task 类型标识
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskKind {
    Regular,
    Compact,
    Review,
    Undo,
    UserShell,
}

/// SessionTask trait - 所有 Task 必须实现
#[async_trait]
pub trait SessionTask: Send + Sync + 'static {
    fn kind(&self) -> TaskKind;

    async fn run(
        self: Arc<Self>,
        session: Arc<Session>,
        ctx: Arc<TurnContext>,
        cancellation_token: CancellationToken,
    ) -> Option<String>;
}

/// RunningTask - 运行中的任务
pub struct RunningTask {
    pub kind: TaskKind,
    pub cancellation_token: CancellationToken,
    pub turn_context: Arc<TurnContext>,
    pub done: Arc<tokio::sync::Notify>,
}
```

#### 3.2 `src/tasks/regular.rs`

```rust
use std::sync::Arc;
use async_trait::async_trait;

use crate::tasks::{SessionTask, TaskKind};
use crate::session::{Session, TurnContext};

#[derive(Clone, Copy, Default)]
pub struct RegularTask;

#[async_trait]
impl SessionTask for RegularTask {
    fn kind(&self) -> TaskKind {
        TaskKind::Regular
    }

    async fn run(
        self: Arc<Self>,
        _session: Arc<Session>,
        _ctx: Arc<TurnContext>,
        _cancellation_token: CancellationToken,
    ) -> Option<String> {
        // TODO: 阶段 4 实现 run_turn 逻辑
        Some("not implemented yet".to_string())
    }
}
```

#### 3.3 `src/tasks/compact.rs`

```rust
use std::sync::Arc;
use async_trait::async_trait;

use crate::tasks::{SessionTask, TaskKind};
use crate::session::{Session, TurnContext};

#[derive(Clone, Copy, Default)]
pub struct CompactTask;

#[async_trait]
impl SessionTask for CompactTask {
    fn kind(&self) -> TaskKind {
        TaskKind::Compact
    }

    async fn run(
        self: Arc<Self>,
        _session: Arc<Session>,
        _ctx: Arc<TurnContext>,
        _cancellation_token: CancellationToken,
    ) -> Option<String> {
        // TODO: 阶段 6 实现压缩逻辑
        Some("not implemented yet".to_string())
    }
}
```

#### 3.4 `src/tasks/loop.rs`

```rust
use std::sync::Arc;
use tokio::sync::mpsc;

use crate::protocol::Op;
use crate::session::Session;

/// Submission 结构
pub struct Submission {
    pub id: String,
    pub op: Op,
}

/// Codex 兼容的核心事件循环
pub async fn submission_loop(
    _sess: Arc<Session>,
    _rx_sub: mpsc::Receiver<Submission>,
) {
    // TODO: 阶段 4 实现完整逻辑
    tracing::info!("submission_loop not implemented yet");
}
```

### 修改文件

#### 3.5 `src/lib.rs`

添加模块导出：

```rust
pub mod tasks;
```

#### 3.6 `Cargo.toml`

添加依赖：

```toml
[dependencies]
# ... 现有依赖 ...
tokio-util = { version = "0.7", features = ["sync"] }
```

### 单元测试（阶段 3）

创建 `src/tasks/regular_tests.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_regular_task_kind() {
        let task = RegularTask;
        assert_eq!(task.kind(), TaskKind::Regular);
    }

    #[test]
    fn test_task_kind_eq() {
        assert_eq!(TaskKind::Regular, TaskKind::Regular);
        assert_ne!(TaskKind::Regular, TaskKind::Compact);
    }
}
```

### 验收标准（阶段 3）
- [ ] SessionTask trait 定义正确
- [ ] RegularTask 和 CompactTask 实现了 trait
- [ ] 代码可以编译通过
- [ ] 单元测试全部通过

---

## 阶段 4: submission_loop 核心循环

### 目标
实现 Codex 兼容的核心事件循环

**⚠️ 此阶段需要和叉叉确认细节后再实现**

### 需要确认的问题
1. submission_loop 是否需要替换现有的 session_loop？
2. 如何处理现有 Op 到新 Op 的转换？
3. Task 取消和中断的详细逻辑？

### 修改文件

#### 4.1 `src/tasks/loop.rs` (完整实现)

```rust
use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::{debug, info};

use crate::protocol::{Event, Op};
use crate::session::Session;
use crate::tasks::{CompactTask, RegularTask};

/// Submission 结构
pub struct Submission {
    pub id: String,
    pub op: Op,
}

/// Codex 兼容的核心事件循环
pub async fn submission_loop(
    sess: Arc<Session>,
    rx_sub: mpsc::Receiver<Submission>,
) {
    let mut previous_context: Option<Arc<TurnContext>> = None;

    info!("Starting submission loop");

    while let Ok(sub) = rx_sub.recv().await {
        debug!(op = ?sub.op, "Processing submission");

        match sub.op {
            Op::Interrupt => {
                handle_interrupt(&sess).await;
            }

            Op::OverrideTurnContext { cwd, approval_policy, sandbox_policy, model, effort, summary, collaboration_mode } => {
                handle_override_turn_context(&sess, sub.id, cwd, approval_policy, sandbox_policy, model, effort, summary, collaboration_mode).await;
                previous_context = Some(sess.new_default_turn().await);
            }

            Op::UserTurn { .. } | Op::UserInputLegacy { .. } => {
                handle_user_input_or_turn(&sess, sub.id, sub.op, &mut previous_context).await;
            }

            Op::ExecApproval { id, decision } => {
                handle_exec_approval(&sess, id, decision).await;
            }

            Op::Compact => {
                if let Some(ctx) = &previous_context {
                    sess.spawn_task(Arc::clone(ctx), CompactTask).await;
                }
            }

            Op::Shutdown => {
                info!("Shutdown requested, exiting submission loop");
                break;
            }

            // 兼容现有 Op
            Op::StartTurn { prompt, context } => {
                // TODO: 转换为 UserTurn 处理
                debug!("StartTurn: {}", prompt);
            }

            Op::UserInput { content } => {
                debug!("UserInput: {}", content);
            }

            Op::ApprovalResponse { request_id, approved } => {
                debug!("ApprovalResponse: {} -> {}", request_id, approved);
            }

            Op::Handoff { target_agent, .. } => {
                debug!("Handoff to: {}", target_agent);
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
    _cwd: Option<std::path::PathBuf>,
    _approval_policy: Option<crate::protocol::ApprovalPolicy>,
    _sandbox_policy: Option<crate::protocol::SandboxPolicy>,
    _model: Option<String>,
    _effort: Option<Option<crate::protocol::ReasoningEffort>>,
    _summary: Option<crate::protocol::ReasoningSummary>,
    _collaboration_mode: Option<crate::protocol::CollaborationMode>,
) {
    // TODO: 更新 TurnContext
    debug!("Handling override turn context");
}

async fn handle_user_input_or_turn(
    sess: &Session,
    _sub_id: String,
    _op: Op,
    _previous_context: &mut Option<Arc<TurnContext>>,
) {
    // TODO: 处理用户输入
    debug!("Handling user input or turn");
}

async fn handle_exec_approval(
    sess: &Session,
    _id: String,
    _decision: crate::protocol::ReviewDecision,
) {
    // TODO: 处理执行批准
    debug!("Handling exec approval");
}
```

### 验收标准（阶段 4）
- [ ] 代码可以编译通过
- [ ] submission_loop 可以启动并接收 Op
- [ ] 基本的 Op 路由工作正常
- [ ] 单元测试通过

---

## 阶段 5: Session Task 管理

### 目标
增强 Session 以支持 Task 管理

**⚠️ 此阶段需要和叉叉确认细节后再实现**

### 需要确认的问题
1. Session 如何跟踪活动任务？
2. abort_all_tasks 的实现细节？
3. spawn_task 如何集成 tokio::spawn？

### 修改文件

#### 5.1 `src/session/session.rs`

```rust
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex};
use tokio_stream::wrappers::ReceiverStream;

use crate::error::{AgentError, AgentResult};
use crate::protocol::{Event, EventQueue, Op, SubmissionQueue};
use crate::session::{ConversationHistory, SessionState, TurnContext};
use crate::tasks::{SessionTask, TaskKind};

#[derive(Debug)]
pub struct Session {
    history: Arc<Mutex<ConversationHistory>>,
    state: Arc<Mutex<SessionState>>,
    submission: SubmissionQueue,
    event_sender: mpsc::Sender<Event>,

    // === 新增字段 ===
    active_turn: Arc<Mutex<Option<ActiveTurn>>>,
    config: SessionConfig,
}

struct ActiveTurn {
    // TODO: 添加任务跟踪字段
}

#[derive(Clone, Debug)]
pub struct SessionConfig {
    pub queue_buffer: usize,
    pub event_buffer: usize,
    pub default_model: String,
}

impl Default for SessionConfig {
    fn default() -> Self {
        Self {
            queue_buffer: 64,
            event_buffer: 64,
            default_model: "gpt-4".to_string(),
        }
    }
}

// ... SessionHandle 和现有方法保持不变 ...

impl Session {
    pub fn new(buffer: usize) -> (Self, SessionHandle) {
        let (op_sender, op_receiver) = mpsc::channel(buffer);
        let submission = SubmissionQueue::new(op_sender);

        let (event_sender, event_queue) = EventQueue::new(buffer);
        let event_stream = event_queue.stream();

        let session = Self {
            history: Arc::new(Mutex::new(ConversationHistory::new())),
            state: Arc::new(Mutex::new(SessionState::Idle)),
            submission,
            event_sender,
            active_turn: Arc::new(Mutex::new(None)),
            config: SessionConfig::default(),
        };

        let handle = SessionHandle {
            submission: session.submission.clone(),
            event_stream: Arc::new(Mutex::new(event_stream)),
        };

        tokio::spawn(session_loop(op_receiver, session.event_sender.clone()));

        (session, handle)
    }

    // === 新增方法 ===

    pub async fn new_default_turn(&self) -> Arc<TurnContext> {
        Arc::new(TurnContext {
            sub_id: uuid::Uuid::new_v4().to_string(),
            model: self.config.default_model.clone(),
            ..Default::default()
        })
    }

    pub async fn spawn_task<T: SessionTask>(
        &self,
        _turn_context: Arc<TurnContext>,
        _task: T,
    ) {
        // TODO: 阶段 5 实现
        tracing::warn!("spawn_task not fully implemented yet");
    }

    pub async fn abort_all_tasks(&self) {
        // TODO: 阶段 5 实现
        tracing::warn!("abort_all_tasks not fully implemented yet");
    }
}
```

### 验收标准（阶段 5）
- [ ] Session 有新的字段和方法
- [ ] 代码可以编译通过
- [ ] 单元测试通过

---

## 阶段 6: 历史压缩功能

### 目标
实现 ConversationHistory 的自动压缩

**⚠️ 此阶段需要和叉叉确认细节后再实现**

### 需要确认的问题
1. 压缩提示词的格式？
2. 如何调用模型生成摘要？
3. 压缩后的历史如何存储？

### 修改文件

#### 6.1 `src/session/history.rs`

```rust
use serde::{Deserialize, Serialize};
use crate::model::Message;
use crate::token::TokenCounter;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ConversationHistory {
    messages: Vec<Message>,
    compacted_summaries: Vec<CompactedSummary>,
    token_counter: TokenCounter,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompactedSummary {
    pub turn_id: String,
    pub summary: String,
    pub original_token_count: usize,
    pub timestamp: i64,
}

impl ConversationHistory {
    pub fn new() -> Self {
        Self {
            messages: Vec::new(),
            compacted_summaries: Vec::new(),
            token_counter: TokenCounter::new(false),
        }
    }

    pub fn push(&mut self, message: Message) {
        self.messages.push(message);
    }

    pub fn all(&self) -> &[Message] {
        &self.messages
    }

    // === 新增方法 ===

    pub fn total_tokens(&self) -> usize {
        let messages_tokens: usize = self.messages
            .iter()
            .map(|m| self.token_counter.count(&m.content))
            .sum();

        let summaries_tokens: usize = self.compacted_summaries
            .iter()
            .map(|s| self.token_counter.count(&s.summary))
            .sum();

        messages_tokens + summaries_tokens
    }

    pub fn compact(&mut self, keep_recent: usize, summary: String) {
        if self.messages.len() > keep_recent {
            let compacted_count = self.messages.len() - keep_recent;
            let compacted_tokens: usize = self.messages
                .iter()
                .take(compacted_count)
                .map(|m| self.token_counter.count(&m.content))
                .sum();

            self.messages = self.messages.split_off(self.messages.len() - keep_recent);

            self.compacted_summaries.push(CompactedSummary {
                turn_id: uuid::Uuid::new_v4().to_string(),
                summary,
                original_token_count: compacted_tokens,
                timestamp: chrono::Utc::now().timestamp(),
            });
        }
    }

    pub fn for_prompt(&self) -> Vec<Message> {
        let mut result = Vec::new();

        for summary in &self.compacted_summaries {
            result.push(Message::system(format!(
                "[Previous conversation summary: {}]",
                summary.summary
            )));
        }

        result.extend(self.messages.iter().cloned());
        result
    }
}
```

### 单元测试（阶段 6）

创建 `src/session/history_tests.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Message;

    #[test]
    fn test_history_new() {
        let history = ConversationHistory::new();
        assert_eq!(history.all().len(), 0);
        assert_eq!(history.compacted_summaries.len(), 0);
    }

    #[test]
    fn test_history_push() {
        let mut history = ConversationHistory::new();
        history.push(Message::user("hello".to_string()));
        assert_eq!(history.all().len(), 1);
    }

    #[test]
    fn test_total_tokens() {
        let mut history = ConversationHistory::new();
        history.push(Message::user("hello world".to_string()));
        // "hello world" ≈ 3 tokens
        assert!(history.total_tokens() >= 2 && history.total_tokens() <= 4);
    }

    #[test]
    fn test_compact() {
        let mut history = ConversationHistory::new();
        for i in 0..10 {
            history.push(Message::user(format!("message {}", i)));
        }
        history.compact(5, "summary".to_string());
        assert_eq!(history.all().len(), 5);
        assert_eq!(history.compacted_summaries.len(), 1);
    }

    #[test]
    fn test_for_prompt() {
        let mut history = ConversationHistory::new();
        history.push(Message::user("hello".to_string()));
        history.compact(0, "previous summary".to_string());

        let prompt = history.for_prompt();
        assert_eq!(prompt.len(), 2); // summary + current message
        assert!(prompt[0].content.contains("previous summary"));
    }
}
```

### 验收标准（阶段 6）
- [ ] ConversationHistory 有新方法
- [ ] total_tokens 正确计算
- [ ] compact 正确压缩历史
- [ ] for_prompt 正确组合消息
- [ ] 单元测试全部通过

---

## 关键文件清单

### 新增文件
| 文件 | 说明 | 阶段 |
|------|------|------|
| `src/token/mod.rs` | Token 模块入口 | 2 |
| `src/token/counter.rs` | Token 计数器 | 2 |
| `src/token/policy.rs` | TruncationPolicy | 2 |
| `src/tasks/mod.rs` | Tasks 模块入口，SessionTask trait | 3 |
| `src/tasks/regular.rs` | RegularTask 实现 | 3 |
| `src/tasks/compact.rs` | CompactTask 实现 | 3 |
| `src/tasks/loop.rs` | submission_loop 核心 | 3-4 |

### 修改文件
| 文件 | 说明 | 阶段 |
|------|------|------|
| `src/protocol/op.rs` | 扩展 Op 枚举 | 1 |
| `src/protocol/event.rs` | 扩展 Event 枚举 | 1 |
| `src/session/context.rs` | 增强 TurnContext | 1 |
| `src/session/session.rs` | 增强任务管理 | 5 |
| `src/session/history.rs` | 增强历史管理 | 6 |
| `src/session/mod.rs` | 导出新类型 | 1 |
| `src/lib.rs` | 导出新模块 | 2, 3 |
| `Cargo.toml` | 添加依赖 | 3 |

---

## 注意事项

1. **每个阶段完成后必须通过所有单元测试**
2. **有不确认的点及时和叉叉沟通，不要自己瞎写**
3. **保持向后兼容，现有 API 继续工作**
4. **渐进式实现，每完成一个阶段再进入下一个**
