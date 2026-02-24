use std::collections::HashSet;
use std::sync::Arc;

use serde_json::Value;

use crate::error::{AgentError, AgentResult};
use crate::tools::{
    ApprovalDecision, ApprovalHook, ToolContext, ToolDef, ToolRegistry, ToolResult,
};

/// 工具执行器 - 管理工具的执行和访问控制
///
/// # 功能
///
/// - 工具注册表管理
/// - 访问控制（允许列表、拒绝列表）
/// - 批准钩子集成
/// - 策略执行
///
/// # 示例
///
/// ```rust
/// use agent_lib::tools::{ToolExecutor, ToolRegistry, ToolContext};
/// use serde_json::json;
///
/// # #[tokio::main]
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// let registry = ToolRegistry::new();
/// let executor = ToolExecutor::new(registry);
/// let ctx = ToolContext::default();
///
/// // 假设注册了 read_file 工具
/// let result = executor.execute(
///     "read_file",
///     json!({"path": "/tmp/file.txt"}),
///     &ctx
/// ).await?;
/// # Ok(())
/// # }
/// ```
pub struct ToolExecutor {
    registry: ToolRegistry,
    approval: Option<Arc<dyn ApprovalHook>>,
    allowlist: Option<HashSet<String>>,
    denylist: HashSet<String>,
}

impl ToolExecutor {
    pub fn new(registry: ToolRegistry) -> Self {
        Self {
            registry,
            approval: None,
            allowlist: None,
            denylist: HashSet::new(),
        }
    }

    pub fn with_approval_hook(mut self, hook: Arc<dyn ApprovalHook>) -> Self {
        self.approval = Some(hook);
        self
    }

    pub fn with_allowlist(mut self, tools: impl IntoIterator<Item = String>) -> Self {
        self.allowlist = Some(tools.into_iter().collect());
        self
    }

    pub fn with_denylist(mut self, tools: impl IntoIterator<Item = String>) -> Self {
        self.denylist = tools.into_iter().collect();
        self
    }

    pub fn list(&self) -> Vec<ToolDef> {
        self.registry.list()
    }

    pub async fn execute(
        &self,
        name: &str,
        args: Value,
        ctx: &ToolContext,
    ) -> AgentResult<ToolResult> {
        if self.denylist.contains(name) {
            return Err(AgentError::Tool(format!("tool denied by policy: {name}")));
        }
        if let Some(allowlist) = &self.allowlist
            && !allowlist.contains(name)
        {
            return Err(AgentError::Tool(format!(
                "tool not allowed by policy: {name}"
            )));
        }

        let tool = self
            .registry
            .get(name)
            .ok_or_else(|| AgentError::Tool(format!("tool not found: {name}")))?;

        if let Some(hook) = &self.approval {
            match hook.check(name, &args).await {
                ApprovalDecision::Approve => {}
                ApprovalDecision::Ask => {
                    return Err(AgentError::Tool(format!(
                        "approval required for tool {name}"
                    )));
                }
                ApprovalDecision::Deny { reason } => {
                    return Err(AgentError::Tool(format!(
                        "tool denied: {name}, reason: {reason}"
                    )));
                }
            }
        }

        tool.execute(args, ctx).await
    }
}
