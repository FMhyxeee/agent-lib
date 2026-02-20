pub mod agent;
pub mod error;
pub mod mcp;
pub mod model;
pub mod protocol;
pub mod session;
pub mod skills;
pub mod tasks;
pub mod token;
pub mod tools;
pub mod trace;

pub use agent::{
    Agent, AgentBuilder, AgentConfig, AgentRegistry, AgentRole, BranchResult,
    GovernanceInjectionIssue, GovernanceInjectionRequest, GovernanceInjectionResult,
    GovernanceInjectionSeverity, GovernedOrchestrationResult, GovernedOrchestrator,
    HandoffReceiver, OrchestrationArtifact, OrchestrationHistoryEntry, OrchestrationRequest,
    OrchestrationResult, OrchestrationTimings, Orchestrator, OrchestratorOptions, StepStatus,
    global_agent_registry,
};
pub use error::{AgentError, AgentResult};
pub use protocol::{Event, Op};
pub use session::{
    CompactedSummary, Session, SessionConfig, SessionHandle, TaskSession, TurnContext,
};
pub use tasks::{
    CompactTask, RegularTask, RunningTask, SessionTask, Submission, TaskKind, submission_loop,
};
pub use token::{TokenCounter, TruncationMode, TruncationPolicy, count_tokens};

// Re-export MCP types for convenience
pub use mcp::{
    AuthConfig, AuthType, CallToolRequestParams, CallToolResult, ConfigLoader,
    GetPromptRequestParams, GetPromptResult, McpClient, McpConfig, McpManager,
    PaginatedRequestParams, Prompt, PromptArgument, PromptMessage, PromptMessageContent,
    PromptMessageRole, ReadResourceRequestParams, ReadResourceResult, Resource, ResourceContents,
    ServerConfig, TlsConfig, Tool, TransportType,
};

// Re-export Codex-compatible protocol types
pub use protocol::{
    ApprovalPolicy, CollaborationMode, CompactedItem, CustomPromptInfo, McpServerRefreshConfig,
    McpToolInfo, ReasoningEffort, ReasoningSummary, ReviewDecision, ReviewRequest, SandboxPolicy,
    SkillEntry, TurnAbortReason, UserInputItem, UserInputResponse,
};

pub use skills::{Skill, SkillConfig, SkillLoader, SkillMetadata, SkillRegistry, SkillSource};
