use std::sync::Arc;

use agent_lib::tools::builtin::FileSystemTool;
use agent_lib::tools::{ApprovalDecision, ApprovalHook, ToolContext, ToolExecutor, ToolRegistry};
use async_trait::async_trait;
use serde_json::json;
use serde_json::Value;

struct AllowAllApproval;

#[async_trait]
impl ApprovalHook for AllowAllApproval {
    async fn check(&self, _tool: &str, _args: &Value) -> ApprovalDecision {
        ApprovalDecision::Approve
    }
}

#[tokio::main]
async fn main() -> agent_lib::AgentResult<()> {
    let mut registry = ToolRegistry::new();
    registry.register(Arc::new(FileSystemTool::new()));

    let executor = ToolExecutor::new(registry).with_approval_hook(Arc::new(AllowAllApproval));
    let ctx = ToolContext {
        cwd: None,
        sandbox_root: None,
    };

    executor
        .execute(
            "filesystem",
            json!({
                "operation": "write",
                "path": "dump.txt",
                "content": "hello"
            }),
            &ctx,
        )
        .await?;

    executor
        .execute(
            "filesystem",
            json!({
                "operation": "delete",
                "path": "dump.txt"
            }),
            &ctx,
        )
        .await?;

    println!("dump.txt created and deleted via filesystem tool.");
    Ok(())
}
