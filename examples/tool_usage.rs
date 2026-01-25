use agent_lib::model::provider::LocalProvider;
use agent_lib::tools::builtin::{CodeExecTool, FileSystemTool, NetworkTool, ShellTool};
use agent_lib::tools::{ApprovalDecision, ApprovalHook};
use agent_lib::{AgentBuilder, AgentResult};
use async_trait::async_trait;
use serde_json::Value;

struct AllowAllApproval;

#[async_trait]
impl ApprovalHook for AllowAllApproval {
    async fn check(&self, _tool: &str, _args: &Value) -> ApprovalDecision {
        ApprovalDecision::Approve
    }
}

#[tokio::main]
async fn main() -> AgentResult<()> {
    let agent = AgentBuilder::new()
        .with_model(LocalProvider::new("local-model"))
        .with_tool(ShellTool::new())
        .with_tool(FileSystemTool::new())
        .with_tool(NetworkTool::new())
        .with_tool(CodeExecTool::new())
        .with_approval_hook(AllowAllApproval)
        .build()?;

    println!("Registered tools: {}", agent.tool_executor().list().len());

    let _ = agent.run("Try using the shell tool.").await;
    Ok(())
}
