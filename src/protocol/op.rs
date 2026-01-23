use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::PathBuf;

use crate::protocol::{
    CollaborationMode, McpServerRefreshConfig, ReasoningEffort, ReasoningSummary,
    ReviewDecision, ReviewRequest, UserInputItem, UserInputResponse,
};
use crate::session::TurnContext;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Op {
    // === 现有 Op (保持兼容) ===
    /// 开始一个新的 Turn
    StartTurn { prompt: String, context: TurnContext },
    /// 用户输入
    UserInput { content: String },
    /// 批准响应
    ApprovalResponse { request_id: String, approved: bool },
    /// 中断当前操作
    Interrupt,
    /// 移交到其他 Agent
    Handoff { target_agent: String, context: Value },

    // === 新增 Codex 兼容 Op ===
    /// 用户 Turn - 完整的用户输入上下文
    UserTurn {
        items: Vec<UserInputItem>,
        cwd: PathBuf,
        approval_policy: crate::protocol::ApprovalPolicy,
        sandbox_policy: crate::protocol::SandboxPolicy,
        model: String,
        effort: Option<ReasoningEffort>,
        summary: ReasoningSummary,
        final_output_json_schema: Option<Value>,
        collaboration_mode: Option<CollaborationMode>,
    },

    /// 遗留用户输入（向后兼容）
    UserInputLegacy {
        items: Vec<UserInputItem>,
        final_output_json_schema: Option<Value>,
    },

    /// 覆盖 Turn 上下文
    OverrideTurnContext {
        cwd: Option<PathBuf>,
        approval_policy: Option<crate::protocol::ApprovalPolicy>,
        sandbox_policy: Option<crate::protocol::SandboxPolicy>,
        model: Option<String>,
        effort: Option<Option<ReasoningEffort>>,
        summary: Option<ReasoningSummary>,
        collaboration_mode: Option<CollaborationMode>,
    },

    /// 执行批准
    ExecApproval { id: String, decision: ReviewDecision },

    /// 补丁批准
    PatchApproval { id: String, decision: ReviewDecision },

    /// 用户输入回答
    UserInputAnswer { id: String, response: UserInputResponse },

    /// 添加到历史
    AddToHistory { text: String },

    /// 获取历史条目请求
    GetHistoryEntryRequest { offset: usize, log_id: u64 },

    /// 列出 MCP 工具
    ListMcpTools,

    /// 刷新 MCP 服务器
    RefreshMcpServers { config: McpServerRefreshConfig },

    /// 列出自定义提示
    ListCustomPrompts,

    /// 列出技能
    ListSkills {
        cwds: Vec<PathBuf>,
        force_reload: bool,
    },

    /// 撤销操作
    Undo,

    /// 压缩历史
    Compact,

    /// 线程回滚
    ThreadRollback { num_turns: u32 },

    /// 审查
    Review { review_request: ReviewRequest },

    /// 关闭
    Shutdown,

    /// 运行用户 Shell 命令
    RunUserShellCommand { command: String },

    /// 列出模型
    ListModels,
}
