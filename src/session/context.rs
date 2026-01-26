use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::model::get_context_window;
use crate::protocol::{ApprovalPolicy, CollaborationMode, ReasoningEffort, ReasoningSummary};
use crate::token::TruncationPolicy;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TurnContext {
    // === 现有字段 (保持兼容) ===
    pub model: String,
    pub sandbox: Option<String>,
    pub cwd: Option<String>,
    pub approval_policy: Option<String>,

    // === 新增字段 ===
    /// Submission ID - 唯一标识本次提交
    pub sub_id: String,

    /// 新版批准策略
    pub approval_policy_v2: Option<ApprovalPolicy>,

    /// 沙盒策略
    pub sandbox_policy_v2: Option<crate::protocol::SandboxPolicy>,

    /// 协作模式
    pub collaboration_mode: Option<CollaborationMode>,

    /// 推理努力程度
    pub reasoning_effort: Option<ReasoningEffort>,

    /// 推理摘要
    pub reasoning_summary: Option<ReasoningSummary>,

    /// 用户指令
    pub user_instructions: Option<String>,

    /// 开发者指令
    pub developer_instructions: Option<String>,

    /// 最终输出 JSON Schema
    pub final_output_json_schema: Option<Value>,

    /// 截断策略
    pub truncation_policy: Option<TruncationPolicy>,

    /// 自动压缩 token 限制
    pub auto_compact_token_limit: Option<i64>,

    /// 上下文窗口大小
    pub context_window: usize,
}

impl Default for TurnContext {
    fn default() -> Self {
        Self {
            model: "default".to_string(),
            sandbox: None,
            cwd: None,
            approval_policy: None,
            sub_id: uuid::Uuid::new_v4().to_string(),
            approval_policy_v2: None,
            sandbox_policy_v2: None,
            collaboration_mode: None,
            reasoning_effort: None,
            reasoning_summary: None,
            user_instructions: None,
            developer_instructions: None,
            final_output_json_schema: None,
            truncation_policy: None,
            auto_compact_token_limit: None,
            context_window: 200_000, // 默认上下文窗口 (GLM 模型)
        }
    }
}

impl TurnContext {
    /// 创建新的 TurnContext
    /// 自动根据模型名称设置 context_window
    pub fn new(model: impl Into<String>) -> Self {
        let model_id = model.into();
        let context_window = get_context_window(&model_id);

        Self {
            model: model_id,
            context_window,
            ..Default::default()
        }
    }

    /// 设置模型
    pub fn with_model(mut self, model: impl Into<String>) -> Self {
        self.model = model.into();
        self
    }

    /// 设置工作目录
    pub fn with_cwd(mut self, cwd: impl Into<String>) -> Self {
        self.cwd = Some(cwd.into());
        self
    }

    /// 设置批准策略
    pub fn with_approval_policy(mut self, policy: ApprovalPolicy) -> Self {
        self.approval_policy_v2 = Some(policy);
        self
    }

    /// 设置沙盒策略
    pub fn with_sandbox_policy(mut self, policy: crate::protocol::SandboxPolicy) -> Self {
        self.sandbox_policy_v2 = Some(policy);
        self
    }

    /// 设置推理努力程度
    pub fn with_reasoning_effort(mut self, effort: ReasoningEffort) -> Self {
        self.reasoning_effort = Some(effort);
        self
    }

    /// 设置上下文窗口
    pub fn with_context_window(mut self, window: usize) -> Self {
        self.context_window = window;
        self
    }

    /// 设置自动压缩限制
    pub fn with_auto_compact_limit(mut self, limit: i64) -> Self {
        self.auto_compact_token_limit = Some(limit);
        self
    }

    /// 获取有效的批准策略
    pub fn get_approval_policy(&self) -> ApprovalPolicy {
        self.approval_policy_v2.unwrap_or_default()
    }

    /// 获取有效的沙盒策略
    pub fn get_sandbox_policy(&self) -> crate::protocol::SandboxPolicy {
        self.sandbox_policy_v2.unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_turn_context_default() {
        let ctx = TurnContext::default();
        assert_eq!(ctx.model, "default");
        assert_eq!(ctx.context_window, 200_000); // GLM 模型默认 200K
        assert!(ctx.approval_policy_v2.is_none());
        assert!(ctx.sandbox_policy_v2.is_none());
    }

    #[test]
    fn test_turn_context_new() {
        let ctx = TurnContext::new("gpt-4");
        assert_eq!(ctx.model, "gpt-4");
    }

    #[test]
    fn test_turn_context_builder_pattern() {
        let ctx = TurnContext::new("gpt-4")
            .with_cwd("/home/user")
            .with_approval_policy(ApprovalPolicy::NeverAsk)
            .with_sandbox_policy(crate::protocol::SandboxPolicy::Readonly)
            .with_reasoning_effort(ReasoningEffort::High)
            .with_context_window(200000)
            .with_auto_compact_limit(50000);

        assert_eq!(ctx.model, "gpt-4");
        assert_eq!(ctx.cwd, Some("/home/user".to_string()));
        assert_eq!(ctx.approval_policy_v2, Some(ApprovalPolicy::NeverAsk));
        assert_eq!(
            ctx.sandbox_policy_v2,
            Some(crate::protocol::SandboxPolicy::Readonly)
        );
        assert_eq!(ctx.reasoning_effort, Some(ReasoningEffort::High));
        assert_eq!(ctx.context_window, 200000);
        assert_eq!(ctx.auto_compact_token_limit, Some(50000));
    }

    #[test]
    fn test_get_approval_policy() {
        let ctx = TurnContext::new("test").with_approval_policy(ApprovalPolicy::ReadOnlySafe);
        assert_eq!(ctx.get_approval_policy(), ApprovalPolicy::ReadOnlySafe);

        let ctx2 = TurnContext::new("test");
        assert_eq!(ctx2.get_approval_policy(), ApprovalPolicy::AlwaysAsk); // default
    }

    #[test]
    fn test_get_sandbox_policy() {
        let ctx =
            TurnContext::new("test").with_sandbox_policy(crate::protocol::SandboxPolicy::InMemory);
        assert_eq!(
            ctx.get_sandbox_policy(),
            crate::protocol::SandboxPolicy::InMemory
        );

        let ctx2 = TurnContext::new("test");
        assert_eq!(
            ctx2.get_sandbox_policy(),
            crate::protocol::SandboxPolicy::Persistent
        ); // default
    }

    #[test]
    fn test_turn_context_serialization() {
        let ctx = TurnContext::new("gpt-4")
            .with_cwd("/home/user")
            .with_approval_policy(ApprovalPolicy::NeverAsk);

        let json = serde_json::to_string(&ctx).unwrap();
        assert!(json.contains("gpt-4"));
        assert!(json.contains("/home/user"));

        let parsed: TurnContext = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.model, "gpt-4");
        assert_eq!(parsed.cwd, Some("/home/user".to_string()));
    }

    #[test]
    fn test_sub_id_is_unique() {
        let ctx1 = TurnContext::default();
        let ctx2 = TurnContext::default();
        assert_ne!(ctx1.sub_id, ctx2.sub_id);
    }

    #[test]
    fn test_turn_context_with_reasoning_summary() {
        let summary = ReasoningSummary {
            summary: "Previous reasoning".to_string(),
            token_count: 100,
        };

        let mut ctx = TurnContext::new("gpt-4");
        ctx.reasoning_summary = Some(summary);

        assert!(ctx.reasoning_summary.is_some());
        let rs = ctx.reasoning_summary.as_ref().unwrap();
        assert_eq!(rs.summary, "Previous reasoning");
        assert_eq!(rs.token_count, 100);
    }

    #[test]
    fn test_turn_context_with_json_schema() {
        let schema = serde_json::json!({
            "type": "object",
            "properties": {
                "name": {"type": "string"}
            }
        });

        let mut ctx = TurnContext::new("gpt-4");
        ctx.final_output_json_schema = Some(schema.clone());

        assert!(ctx.final_output_json_schema.is_some());
        assert_eq!(ctx.final_output_json_schema.unwrap(), schema);
    }
}
