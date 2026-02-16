//! Basic MCP integration example
//!
//! This example demonstrates how to create an agent with MCP tools
//! using the simple `with_mcp_server()` method.

use agent_lib::model::provider::OpenAiProvider;
use agent_lib::tools::{ApprovalDecision, ApprovalHook};
use agent_lib::{AgentBuilder, AgentResult};
use async_trait::async_trait;
use serde_json::Value;
use std::time::Duration;

/// Simple approval hook that allows all tool calls
struct AllowAllApproval;

#[async_trait]
impl ApprovalHook for AllowAllApproval {
    async fn check(&self, _tool: &str, _args: &Value) -> ApprovalDecision {
        ApprovalDecision::Approve
    }
}

async fn run_basic_example() -> AgentResult<()> {
    println!("=== Agent with MCP Tools Example ===\n");

    // Build an agent with MCP server tools
    let builder = AgentBuilder::new().with_model(OpenAiProvider::new("gpt-4"));
    let builder = builder
        // Add tools from an MCP server (e.g., filesystem server)
        // Note: This requires a real MCP server running
        .with_mcp_server("stdio://mcp-server-filesystem")
        .await;
    let agent = builder.with_approval_hook(AllowAllApproval).build()?;

    // List all registered tools (builtin + MCP)
    println!("Registered tools:");
    for tool in agent.tool_executor().list() {
        println!("  - {} ({})", tool.name, tool.description);
    }

    // Run the agent with a prompt
    println!("\n=== Agent Running ===");
    let response = agent
        .run("Use the filesystem tool to create a file called hello.txt with content 'Hello from MCP!'")
        .await?;

    println!("\nAgent response: {}", response);

    Ok(())
}

/// Alternative example using a pre-configured MCP client
#[allow(dead_code)]
async fn example_with_client() -> AgentResult<()> {
    use agent_lib::mcp::{McpClient, ServerConfig, TransportType};
    use std::sync::Arc;

    println!("=== Agent with MCP Client Example ===\n");

    // Manually create and configure MCP client
    let config = ServerConfig {
        name: "filesystem".to_string(),
        transport: TransportType::Stdio,
        endpoint: "stdio://mcp-server-filesystem".to_string(),
        command: None,
        args: Vec::new(),
        auth: None,
        headers: Default::default(),
        tls: None,
        timeout: Duration::from_secs(30),
        enabled: true,
        env: Default::default(),
    };
    let client = Arc::new(McpClient::connect(config).await?);

    // Inspect available tools before building agent
    let tools = client.list_tools().await?;
    println!("Available MCP tools: {}", tools.len());
    for tool in &tools {
        println!(
            "  - {}: {}",
            tool.name,
            tool.description.as_deref().unwrap_or_default()
        );
    }

    if tools.is_empty() {
        println!("No tools found - check if MCP server is running");
        return Ok(());
    }

    // Build agent with the pre-configured client
    let builder = AgentBuilder::new().with_model(OpenAiProvider::new("gpt-4"));
    let builder = builder.with_mcp_client(client.clone()).await?;
    let agent = builder.build()?;

    println!(
        "Total registered tools: {}",
        agent.tool_executor().list().len()
    );
    Ok(())
}

/// Example showing multiple MCP servers
#[allow(dead_code)]
async fn example_multiple_servers() -> AgentResult<()> {
    use agent_lib::mcp::McpManager;

    println!("=== Agent with Multiple MCP Servers ===\n");

    // Create MCP manager
    let manager = McpManager::new();

    // Add multiple MCP servers
    match manager
        .add_server("filesystem", "stdio://mcp-server-filesystem")
        .await
    {
        Ok(tools) => {
            println!("Added filesystem server with {} tools", tools.len());
        }
        Err(err) => {
            println!("Failed to add filesystem server: {}", err);
        }
    }

    match manager
        .add_server("search", "https://example.com/mcp")
        .await
    {
        Ok(tools) => {
            println!("Added search server with {} tools", tools.len());
        }
        Err(err) => {
            println!("Failed to add search server: {}", err);
        }
    }

    // Build agent with the manager
    let builder = AgentBuilder::new().with_model(OpenAiProvider::new("gpt-4"));
    let builder = builder.with_mcp_manager(manager.clone()).await?;
    let agent = builder.build()?;

    println!(
        "Total registered tools: {}",
        agent.tool_executor().list().len()
    );

    Ok(())
}

/// Transport examples showing all supported protocols
#[allow(dead_code)]
fn show_transport_examples() {
    println!("=== Supported MCP Transports ===\n");

    println!("Strict official transport mode:");
    println!("  # Stdio (spawn subprocess)");
    println!("  .with_mcp_server(\"stdio://mcp-server-filesystem\").await");
    println!();
    println!("  # Streamable HTTP");
    println!("  .with_mcp_server(\"http://localhost:9000/mcp\").await");
    println!("  .with_mcp_server(\"https://api.example.com/mcp\").await");
}

/// Main function demonstrating all examples
#[allow(dead_code)]
async fn main_all_examples() -> AgentResult<()> {
    show_transport_examples();

    println!("\n=== Basic MCP Example ===");
    if let Err(e) = run_basic_example().await {
        println!("Example failed (expected without MCP server): {}", e);
    }

    println!("\n=== MCP Client Example ===");
    if let Err(e) = example_with_client().await {
        println!("Example failed (expected without MCP server): {}", e);
    }

    println!("\n=== Multiple Servers Example ===");
    if let Err(e) = example_multiple_servers().await {
        println!("Example failed (expected without MCP server): {}", e);
    }

    Ok(())
}

#[tokio::main]
async fn main() -> AgentResult<()> {
    run_basic_example().await
}
