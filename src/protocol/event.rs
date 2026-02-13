use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio_stream::wrappers::ReceiverStream;

use crate::error::AgentError;
use crate::model::TokenUsage;
use crate::protocol::{
    CompactedItem, CustomPromptInfo, McpPromptInfo, McpResourceInfo, McpToolInfo, ModelInfo,
    PromptMessage, SkillEntry, TurnAbortReason,
};
use crate::tools::ToolResult;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Event {
    // === 现有 Event ===
    /// Turn 开始
    TurnStarted { turn_id: String },
    /// 模型流式输出
    ModelStreaming { chunk: String },
    /// 模型完成
    ModelComplete { content: String, usage: TokenUsage },
    /// 工具调用请求
    ToolCallRequested { tool: String, args: Value },
    /// 工具调用结果
    ToolCallResult { tool: String, result: ToolResult },
    /// 需要批准
    ApprovalRequired {
        request_id: String,
        tool: String,
        args: Value,
    },
    /// 移交发起
    HandoffInitiated { from: String, to: String },
    /// Turn 完成
    TurnComplete { result: Value },
    /// 错误
    Error { error: AgentError },

    // === 新增 Event ===
    /// Session 已配置
    SessionConfigured {
        rollout_path: String,
        thread_id: String,
    },
    /// Turn 中止
    TurnAborted { reason: TurnAbortReason },
    /// 上下文已压缩
    ContextCompacted { compacted_items: Vec<CompactedItem> },
    /// 警告
    Warning { message: String },
    /// MCP 工具列表响应
    McpListToolsResponse { tools: Vec<McpToolInfo> },
    /// MCP 资源列表响应
    McpListResourcesResponse { resources: Vec<McpResourceInfo> },
    /// MCP 资源内容响应
    McpResourceContent { uri: String, content: String },
    /// MCP 提示列表响应
    McpListPromptsResponse { prompts: Vec<McpPromptInfo> },
    /// MCP 提示获取响应
    McpPromptResult { messages: Vec<PromptMessage> },
    /// 自定义提示列表响应
    ListCustomPromptsResponse { prompts: Vec<CustomPromptInfo> },
    /// 技能列表响应
    ListSkillsResponse { skills: Vec<SkillEntry> },
    /// 技能内容响应
    SkillContent {
        name: String,
        content: String,
        auxiliary_files: Vec<String>,
    },
    /// 技能已应用
    SkillApplied { name: String },
    /// 技能文件内容
    SkillFileContent {
        skill_name: String,
        file_path: String,
        content: String,
    },
    /// 线程回滚完成
    ThreadRolledBack { num_turns: u32 },
    /// 撤销完成
    UndoPerformed {
        removed_messages: usize,
        summary: String,
    },
    /// 历史条目
    HistoryEntry {
        offset: usize,
        log_id: u64,
        entry: String,
    },
    /// 运行用户 shell 命令
    RunUserShellCommand { command: String },
    /// 模型列表响应
    ModelsListed { models: Vec<ModelInfo> },
}

pub type EventStream = ReceiverStream<Event>;
