use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::PathBuf;

use crate::protocol::{
    CollaborationMode, McpServerRefreshConfig, ReasoningEffort, ReasoningSummary, ReviewDecision,
    ReviewRequest, UserInputItem, UserInputResponse,
};
use crate::session::TurnContext;

// === Op 构造辅助函数 ===

/// 创建一个简单的 User Turn 操作
pub fn user_turn(items: Vec<UserInputItem>, model: impl Into<String>) -> Op {
    Op::UserTurn {
        items,
        cwd: std::path::PathBuf::from("."),
        approval_policy: crate::protocol::ApprovalPolicy::AlwaysAsk,
        sandbox_policy: crate::protocol::SandboxPolicy::Persistent,
        model: model.into(),
        effort: None,
        summary: ReasoningSummary {
            summary: String::new(),
            token_count: 0,
        },
        final_output_json_schema: None,
        collaboration_mode: None,
    }
}

/// 创建一个带有完整配置的 User Turn 操作
pub fn user_turn_with_config(
    items: Vec<UserInputItem>,
    model: impl Into<String>,
    cwd: impl Into<PathBuf>,
    approval_policy: crate::protocol::ApprovalPolicy,
    sandbox_policy: crate::protocol::SandboxPolicy,
) -> Op {
    Op::UserTurn {
        items,
        cwd: cwd.into(),
        approval_policy,
        sandbox_policy,
        model: model.into(),
        effort: None,
        summary: ReasoningSummary {
            summary: String::new(),
            token_count: 0,
        },
        final_output_json_schema: None,
        collaboration_mode: None,
    }
}

/// 创建一个简单的中断操作
pub fn interrupt() -> Op {
    Op::Interrupt
}

/// 创建一个撤销操作
pub fn undo() -> Op {
    Op::Undo
}

/// 创建一个关闭操作
pub fn shutdown() -> Op {
    Op::Shutdown
}

/// 创建一个压缩历史操作
pub fn compact() -> Op {
    Op::Compact
}

/// 判断 Op 是否需要用户交互
pub fn requires_user_interaction(op: &Op) -> bool {
    matches!(
        op,
        Op::UserTurn { .. }
            | Op::UserInputLegacy { .. }
            | Op::UserInput { .. }
            | Op::UserInputAnswer { .. }
            | Op::GetHistoryEntryRequest { .. }
            | Op::Review { .. }
    )
}

/// 判断 Op 是否是系统控制操作
pub fn is_system_control(op: &Op) -> bool {
    matches!(
        op,
        Op::Interrupt | Op::Shutdown | Op::Compact | Op::Undo | Op::ThreadRollback { .. }
    )
}

/// 判断 Op 是否是 MCP 相关操作
pub fn is_mcp_related(op: &Op) -> bool {
    matches!(
        op,
        Op::ListMcpTools
            | Op::RefreshMcpServers { .. }
            | Op::ListCustomPrompts
            | Op::ListSkills { .. }
    )
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Op {
    // === 基础会话操作 ===
    /// 开始一个新的 Turn
    StartTurn {
        prompt: String,
        context: TurnContext,
    },
    /// 用户输入 - 简单形式（向后兼容）
    UserInput { content: String },
    /// 中断当前操作
    Interrupt,

    // === 用户交互操作 ===
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

    /// 用户输入回答 - 对用户输入请求的响应
    UserInputAnswer {
        id: String,
        response: UserInputResponse,
    },

    // === 审查和批准操作 ===
    /// 批准响应 - 对工具执行批准的响应
    ApprovalResponse { request_id: String, approved: bool },
    /// 执行批准 - 执行代码审查结果
    ExecApproval {
        id: String,
        decision: ReviewDecision,
    },
    /// 补丁批准 - 执行补丁审查结果
    PatchApproval {
        id: String,
        decision: ReviewDecision,
    },

    // === 上下文管理操作 ===
    /// 覆盖 Turn 上下文 - 动态修改上下文
    OverrideTurnContext {
        cwd: Option<PathBuf>,
        approval_policy: Option<crate::protocol::ApprovalPolicy>,
        sandbox_policy: Option<crate::protocol::SandboxPolicy>,
        model: Option<String>,
        effort: Option<Option<ReasoningEffort>>,
        summary: Option<ReasoningSummary>,
        collaboration_mode: Option<CollaborationMode>,
    },

    /// 添加到历史 - 手动添加内容到对话历史
    AddToHistory { text: String },
    /// 获取历史条目请求 - 获取特定历史条目
    GetHistoryEntryRequest { offset: usize, log_id: u64 },

    // === 历史管理操作 ===
    /// 压缩历史 - 手动压缩对话历史
    Compact,
    /// 撤销操作 - 撤销最近的操作
    Undo,
    /// 线程回滚 - 回滚多个回合
    ThreadRollback { num_turns: u32 },

    /// 审查 - 代码审查请求
    Review { review_request: ReviewRequest },

    // === 系统控制操作 ===
    /// 关闭系统
    Shutdown,
    /// 运行用户 Shell 命令
    RunUserShellCommand { command: String },

    // === MCP 协议操作 ===
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

    // === 模型管理操作 ===
    /// 列出可用模型
    ListModels,

    // === 代理协作操作 ===
    /// 移交到其他 Agent
    Handoff {
        target_agent: String,
        context: Value,
    },

    // === 向后兼容标记 ===
    /// [已弃用] 保持向后兼容的旧版本标记
    #[deprecated(note = "Use UserTurn instead")]
    LegacyUserInput { content: String },
}
