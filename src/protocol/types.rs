use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// 批准策略 - 定义工具执行需要用户批准的条件
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "kebab-case")]
pub enum ApprovalPolicy {
    /// 总是询问用户是否批准
    #[default]
    AlwaysAsk,
    /// 只在非安全操作时询问
    ReadOnlySafe,
    /// 从不询问，自动批准所有操作
    NeverAsk,
}

/// 沙盒策略 - 定义文件系统操作的沙盒模式
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "kebab-case")]
pub enum SandboxPolicy {
    /// 只读模式，不能修改文件
    Readonly,
    /// 持久化模式，修改会保存到磁盘
    #[default]
    Persistent,
    /// 内存模式，修改只在内存中，不保存
    InMemory,
}

/// 推理努力程度 - 控制模型推理的深度
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "kebab-case")]
pub enum ReasoningEffort {
    Low,
    #[default]
    Medium,
    High,
}

/// 推理摘要 - 存储之前的推理结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReasoningSummary {
    pub summary: String,
    pub token_count: usize,
}

/// 审查决定 - 用户对工具调用的批准决定
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ReviewDecision {
    Approve,
    Deny,
    ApproveWithEdits { edits: String },
}

/// 用户输入响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum UserInputResponse {
    Text(String),
    Cancel,
}

/// 用户输入项
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum UserInputItem {
    Text { text: String },
    Image { path: PathBuf },
    File { path: PathBuf },
    Command { command: String },
}

impl UserInputItem {
    /// 创建文本输入项
    pub fn text(text: impl Into<String>) -> Self {
        Self::Text { text: text.into() }
    }

    /// 创建图片输入项
    pub fn image(path: PathBuf) -> Self {
        Self::Image { path }
    }

    /// 创建文件输入项
    pub fn file(path: PathBuf) -> Self {
        Self::File { path }
    }

    /// 创建命令输入项
    pub fn command(command: impl Into<String>) -> Self {
        Self::Command {
            command: command.into(),
        }
    }
}

/// 协作模式
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "kebab-case")]
pub enum CollaborationMode {
    /// 单人模式
    #[default]
    Solo,
    /// 协作模式
    Collaborative,
}

/// MCP 服务器刷新配置
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct McpServerRefreshConfig {
    pub force_reload: bool,
}

/// 审查请求
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReviewRequest {
    pub content: String,
    pub context: Option<String>,
}

/// MCP 工具信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpToolInfo {
    pub name: String,
    pub description: String,
    pub server: String,
}

/// 自定义提示信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomPromptInfo {
    pub name: String,
    pub description: String,
}

/// 技能条目
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillEntry {
    pub name: String,
    pub description: String,
    pub path: PathBuf,
}

/// Turn 中止原因
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TurnAbortReason {
    UserCancelled,
    Error(String),
    Timeout,
    TokenLimitExceeded,
}

/// 压缩项
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompactedItem {
    pub turn_id: String,
    pub summary: String,
    pub original_token_count: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json;

    #[test]
    fn test_approval_policy_default() {
        assert_eq!(ApprovalPolicy::default(), ApprovalPolicy::AlwaysAsk);
    }

    #[test]
    fn test_approval_policy_serialization() {
        let policy = ApprovalPolicy::AlwaysAsk;
        let json = serde_json::to_string(&policy).unwrap();
        assert_eq!(json, "\"always-ask\"");

        let parsed: ApprovalPolicy = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, ApprovalPolicy::AlwaysAsk);
    }

    #[test]
    fn test_sandbox_policy_default() {
        assert_eq!(SandboxPolicy::default(), SandboxPolicy::Persistent);
    }

    #[test]
    fn test_reasoning_effort_default() {
        assert_eq!(ReasoningEffort::default(), ReasoningEffort::Medium);
    }

    #[test]
    fn test_collaboration_mode_default() {
        assert_eq!(CollaborationMode::default(), CollaborationMode::Solo);
    }

    #[test]
    fn test_user_input_item_text() {
        let item = UserInputItem::text("hello");
        assert!(matches!(item, UserInputItem::Text { text } if text == "hello"));
    }

    #[test]
    fn test_user_input_item_command() {
        let item = UserInputItem::command("ls -la");
        assert!(matches!(item, UserInputItem::Command { command } if command == "ls -la"));
    }

    #[test]
    fn test_user_input_item_serialization() {
        let item = UserInputItem::text("test");
        let json = serde_json::to_string(&item).unwrap();
        assert!(json.contains("text"));

        let parsed: UserInputItem = serde_json::from_str(&json).unwrap();
        match parsed {
            UserInputItem::Text { text } => assert_eq!(text, "test"),
            _ => panic!("Expected Text variant"),
        }
    }

    #[test]
    fn test_review_decision_equality() {
        assert_eq!(ReviewDecision::Approve, ReviewDecision::Approve);
        assert_ne!(ReviewDecision::Approve, ReviewDecision::Deny);
    }

    #[test]
    fn test_reasoning_summary_creation() {
        let summary = ReasoningSummary {
            summary: "test summary".to_string(),
            token_count: 100,
        };
        assert_eq!(summary.summary, "test summary");
        assert_eq!(summary.token_count, 100);
    }

    #[test]
    fn test_mcp_server_refresh_config_default() {
        let config = McpServerRefreshConfig::default();
        assert!(!config.force_reload);
    }

    #[test]
    fn test_compacted_item_creation() {
        let item = CompactedItem {
            turn_id: "turn-1".to_string(),
            summary: "summary".to_string(),
            original_token_count: 1000,
        };
        assert_eq!(item.turn_id, "turn-1");
        assert_eq!(item.summary, "summary");
        assert_eq!(item.original_token_count, 1000);
    }

    #[test]
    fn test_turn_abort_reason_serialization() {
        let reason = TurnAbortReason::UserCancelled;
        let json = serde_json::to_string(&reason).unwrap();
        let parsed: TurnAbortReason = serde_json::from_str(&json).unwrap();
        assert!(matches!(parsed, TurnAbortReason::UserCancelled));
    }

    #[test]
    fn test_mcp_tool_info() {
        let info = McpToolInfo {
            name: "test-tool".to_string(),
            description: "A test tool".to_string(),
            server: "test-server".to_string(),
        };
        assert_eq!(info.name, "test-tool");
        assert_eq!(info.description, "A test tool");
        assert_eq!(info.server, "test-server");
    }
}
