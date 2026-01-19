//! Advanced MCP integration example using McpManager
//!
//! This example demonstrates how to use the McpManager for
//! centralized MCP server management and sharing across agents.

use agent_lib::mcp::McpManager;
use agent_lib::model::provider::OpenAiProvider;
use agent_lib::tools::{ApprovalDecision, ApprovalHook};
use agent_lib::{AgentBuilder, AgentResult};
use async_trait::async_trait;
use serde_json::Value;
use std::sync::Arc;
use std::time::Duration;

/// Approval hook that checks tool names
struct SmartApproval;

#[async_trait]
impl ApprovalHook for SmartApproval {
    async fn check(&self, tool: &str, _args: &Value) -> ApprovalDecision {
        // Allow filesystem tools, deny others in this example
        if tool.starts_with("filesystem:") || tool.starts_with("fs:") {
            ApprovalDecision::Approve
        } else {
            ApprovalDecision::Deny {
                reason: "Tool not allowed in this example".to_string(),
            }
        }
    }
}

async fn run_manager_example() -> AgentResult<()> {
    println!("=== Advanced MCP Manager Example ===\n");

    // Create MCP manager with custom timeout
    let manager = McpManager::with_timeout(Duration::from_secs(15));

    // Add multiple MCP servers with names
    println!("Adding MCP servers...");

    let mut results = Vec::new();

    // Add filesystem server
    results.push((
        "filesystem",
        manager.add_server("filesystem", "stdio://mcp-server-filesystem").await,
    ));

    // Add database server
    results.push((
        "database",
        manager.add_server_with_timeout(
            "database",
            "tcp://localhost:5432",
            Duration::from_secs(10),
        ).await,
    ));

    // Show results
    for (name, result) in results {
        match result {
            Ok(tools) => {
                println!("✓ {} server: {} tools", name, tools.len());
            }
            Err(err) => {
                println!("✗ {} server failed: {}", name, err);
            }
        }
    }

    // Build agent with the shared manager
    let builder = AgentBuilder::new()
        .with_model(OpenAiProvider::new("gpt-4"));
    let builder = builder.with_mcp_manager(manager.clone()).await?;
    let agent = builder
        .with_approval_hook(SmartApproval)
        .build()?;

    // Show tool information
    println!("\n=== Tool Information ===");
    println!("Total registered tools: {}", agent.tool_executor().list().len());

    for tool in agent.tool_executor().list() {
        println!("  - {} ({})", tool.name, tool.description);

        // Show JSON schema (first 100 chars)
        let schema_str = tool.schema.to_string();
        if schema_str.len() > 100 {
            println!("    Schema: {}...", &schema_str[..100]);
        } else {
            println!("    Schema: {}", schema_str);
        }
    }

    // Try to run the agent with a filesystem prompt
    println!("\n=== Agent Running ===");
    let prompt = "Use the filesystem tool to create a test file.";

    println!("Prompt: {}", prompt);

    // Note: This will likely fail without a real MCP server running,
    // but shows the pattern
    match agent.run(prompt).await {
        Ok(response) => {
            println!("Agent response: {}", response);
        }
        Err(err) => {
            println!("Agent run failed: {}", err);
            println!("(This is expected without a real MCP server)");
        }
    }

    // Demonstrate manager API usage
    println!("\n=== Manager API Demo ===");

    // List all servers
    let servers = manager.list_servers().await;
    println!("Registered servers: {:?}", servers);

    // Show server counts
    println!("Total servers: {}", manager.server_count().await);
    println!("Total tools: {}", manager.total_tools_count().await);

    // Get specific client
    if let Some(client) = manager.get_client("filesystem").await {
        println!("Found filesystem client: {}", Arc::as_ptr(&client) as *const () as usize);
    } else {
        println!("Filesystem client not found");
    }

    Ok(())
}

#[tokio::main]
async fn main() -> AgentResult<()> {
    run_manager_example().await
}

/// Example of sharing manager between multiple agents
async fn example_shared_manager() -> AgentResult<()> {
    use agent_lib::mcp::McpManager;

    println!("=== Shared Manager Example ===\n");

    // Create shared manager
    let manager = McpManager::new();

    // Add a server (will fail without real server, but shows pattern)
    if let Err(err) = manager.add_server("shared", "stdio://mcp-server-test").await {
        println!("Server add failed (expected): {}", err);
    }

    // Create multiple agents sharing the same manager
    println!("Creating agents with shared manager...");

    let builder = AgentBuilder::new()
        .with_model(OpenAiProvider::new("gpt-4"));
    let builder = builder.with_mcp_manager(manager.clone()).await?;
    let agent1 = builder.build()?;

    let builder = AgentBuilder::new()
        .with_model(OpenAiProvider::new("gpt-4"));
    let builder = builder.with_mcp_manager(manager.clone()).await?;
    let agent2 = builder.build()?;

    println!("Agent 1 tools: {}", agent1.tool_executor().list().len());
    println!("Agent 2 tools: {}", agent2.tool_executor().list().len());

    // Show that they share the same MCP connections
    println!("Agents share the same MCP manager instance");

    Ok(())
}

/// Example of server management operations
async fn example_server_management() -> AgentResult<()> {
    use agent_lib::mcp::McpManager;

    println!("=== Server Management Example ===\n");

    let manager = McpManager::new();

    // Add servers (will fail without real servers)
    println!("Adding test servers...");
    let servers = ["test1", "test2", "test3"];

    for name in &servers {
        let result = manager.add_server(*name, format!("stdio://server-{}", name)).await;
        match result {
            Ok(_) => println!("✓ {} added", name),
            Err(err) => println!("✗ {} failed: {}", name, err),
        }
    }

    // Show server state
    println!("Servers: {:?}", manager.list_servers().await);
    println!("Total servers: {}", manager.server_count().await);

    // Remove a server
    if manager.remove_server("test2").await.is_some() {
        println!("✓ test2 removed");
    }

    println!("Remaining servers: {:?}", manager.list_servers().await);
    println!("Remaining server count: {}", manager.server_count().await);

    Ok(())
}

/// Main function demonstrating all advanced examples
async fn main_advanced_examples() -> AgentResult<()> {
    println!("=== Advanced MCP Examples ===\n");

    println!("1. Manager Example:");
    if let Err(e) = run_manager_example().await {
        println!("Example failed: {}", e);
    }

    println!("\n2. Shared Manager Example:");
    if let Err(e) = example_shared_manager().await {
        println!("Example failed: {}", e);
    }

    println!("\n3. Server Management Example:");
    if let Err(e) = example_server_management().await {
        println!("Example failed: {}", e);
    }

    Ok(())
}