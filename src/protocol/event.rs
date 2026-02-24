use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio_stream::wrappers::ReceiverStream;

use crate::error::AgentError;
use crate::model::TokenUsage;
use crate::protocol::{
    CompactedItem, CustomPromptInfo, McpPromptInfo, McpResourceInfo, McpToolInfo, ModelInfo,
    PromptMessage, SkillEntry, SubAgentMode, TurnAbortReason,
};
use crate::tools::ToolResult;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Event {
    TurnStarted {
        turn_id: String,
    },
    ModelStreaming {
        chunk: String,
    },
    /// 推理内容流式输出 (GLM 思考模式)
    ReasoningStreaming {
        chunk: String,
    },

    ModelComplete {
        content: String,
        usage: TokenUsage,
    },
    ToolCallRequested {
        tool: String,
        args: Value,
    },
    ToolCallResult {
        tool: String,
        result: ToolResult,
    },
    ApprovalRequired {
        request_id: String,
        tool: String,
        args: Value,
    },
    HandoffInitiated {
        from: String,
        to: String,
    },
    TurnComplete {
        result: Value,
    },
    Error {
        error: AgentError,
    },

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
    McpListResourcesResponse {
        resources: Vec<McpResourceInfo>,
    },
    McpResourceContent {
        uri: String,
        content: String,
    },
    McpListPromptsResponse {
        prompts: Vec<McpPromptInfo>,
    },
    McpPromptResult {
        messages: Vec<PromptMessage>,
    },
    ListCustomPromptsResponse {
        prompts: Vec<CustomPromptInfo>,
    },
    ListSkillsResponse {
        skills: Vec<SkillEntry>,
    },
    SkillContent {
        name: String,
        content: String,
        auxiliary_files: Vec<String>,
    },
    SkillApplied {
        name: String,
    },
    SkillFileContent {
        skill_name: String,
        file_path: String,
        content: String,
    },
    ThreadRolledBack {
        num_turns: u32,
    },
    UndoPerformed {
        removed_messages: usize,
        summary: String,
    },
    HistoryEntry {
        offset: usize,
        log_id: u64,
        entry: String,
    },
    RunUserShellCommand {
        command: String,
    },
    SubAgentStarted {
        mode: SubAgentMode,
        input: String,
    },
    SubAgentProgress {
        mode: SubAgentMode,
        message: String,
    },
    SubAgentCompleted {
        mode: SubAgentMode,
        output: String,
    },
    SubAgentFailed {
        mode: SubAgentMode,
        error: String,
    },
    ModelsListed {
        models: Vec<ModelInfo>,
    },
}

pub type EventStream = ReceiverStream<Event>;
