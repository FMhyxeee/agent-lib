use std::sync::Arc;

use agent_lib::tools::{
    ApprovalDecision, ApprovalHook, Tool, ToolContext, ToolDef, ToolExecutor, ToolRegistry,
    ToolResult,
};
use async_trait::async_trait;
use serde_json::json;
use serde_json::Value;

struct DummyTool;

#[async_trait]
impl Tool for DummyTool {
    fn definition(&self) -> ToolDef {
        ToolDef {
            name: "dummy".to_string(),
            description: "dummy tool".to_string(),
            schema: json!({}),
        }
    }

    async fn execute(&self, _args: Value, _ctx: &ToolContext) -> agent_lib::AgentResult<ToolResult> {
        Ok(ToolResult::text("ok"))
    }
}

struct DenyAll;

#[async_trait]
impl ApprovalHook for DenyAll {
    async fn check(&self, _tool: &str, _args: &Value) -> ApprovalDecision {
        ApprovalDecision::Deny {
            reason: "blocked".to_string(),
        }
    }
}

#[tokio::test]
async fn tool_executor_runs_tool() {
    let mut registry = ToolRegistry::new();
    registry.register(Arc::new(DummyTool));

    let executor = ToolExecutor::new(registry);
    let result = executor
        .execute(
            "dummy",
            json!({}),
            &ToolContext {
                cwd: None,
                sandbox_root: None,
            },
        )
        .await
        .unwrap();

    assert_eq!(result.output, json!("ok"));
}

#[tokio::test]
async fn tool_executor_respects_approval() {
    let mut registry = ToolRegistry::new();
    registry.register(Arc::new(DummyTool));

    let executor = ToolExecutor::new(registry).with_approval_hook(Arc::new(DenyAll));
    let err = executor
        .execute(
            "dummy",
            json!({}),
            &ToolContext {
                cwd: None,
                sandbox_root: None,
            },
        )
        .await
        .err()
        .unwrap();

    assert!(format!("{err}").contains("denied"));
}
