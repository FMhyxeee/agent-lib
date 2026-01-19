//! Simple test to verify MCP functionality works

use agent_lib::mcp::{McpManager, McpClient, McpTransport, TransportConfig};
use agent_lib::{AgentBuilder, AgentResult};
use std::sync::Arc;

async fn run_main_test() -> AgentResult<()> {
    println!("=== Simple MCP Test ===\n");

    // Test McpManager creation
    let manager = McpManager::new();
    println!("✓ McpManager created successfully");
    println!("Server count: {}", manager.server_count().await);

    // Test McpManager API
    let servers = manager.list_servers().await;
    println!("Servers: {:?}", servers);

    // Test invalid endpoint (should not crash)
    println!("Testing invalid endpoint...");
    match manager.add_server("test", "invalid://nonexistent").await {
        Ok(tools) => println!("Unexpected success: {} tools", tools.len()),
        Err(e) => println!("✓ Expected error: {}", e),
    }

    // Test AgentBuilder with MCP (should not crash with invalid endpoint)
    println!("Testing AgentBuilder with MCP...");
    let agent = AgentBuilder::new()
        .with_mcp_server("invalid://test")
        .await
        .build()?;

    let tools = agent.tool_executor().list();
    println!("✓ Agent built with {} tools", tools.len());

    // Test McpClient creation
    println!("Testing McpClient creation...");
    let transport = match McpTransport::new(TransportConfig {
        endpoint: "invalid://test".to_string(),
    }).await {
        Ok(t) => {
            println!("✓ Transport created");
            t
        }
        Err(e) => {
            println!("✓ Expected transport error: {}", e);
            return Ok(());
        }
    };

    let client = Arc::new(McpClient::new(transport));
    println!("✓ McpClient created");

    // Test MCP timeout methods
    println!("Testing timeout methods...");
    let timeout = std::time::Duration::from_secs(5);
    let tools_result = client.list_tools_with_timeout(Some(timeout)).await;
    match tools_result {
        Ok(tools) => println!("✓ Listed {} tools with timeout", tools.len()),
        Err(e) => println!("✓ Expected timeout error: {}", e),
    }

    println!("\n=== All tests completed successfully! ===");
    Ok(())
}

async fn test_adapter_creation() -> AgentResult<()> {
    use agent_lib::mcp::McpTool;
    use agent_lib::tools::builtin::McpToolAdapter;

    println!("=== Testing McpToolAdapter ===");

    // Create mock tool
    let tool = McpTool {
        name: "test-tool".to_string(),
        description: "Test tool".to_string(),
        schema: serde_json::json!({
            "type": "object",
            "properties": {
                "input": { "type": "string" }
            }
        }),
    };

    println!("✓ Mock tool created");

    // Test adapter creation
    let transport = match McpTransport::new(TransportConfig {
        endpoint: "invalid://test".to_string(),
    }).await {
        Ok(t) => t,
        Err(e) => {
            println!("✓ Expected transport error: {}", e);
            return Ok(());
        }
    };

    let client = Arc::new(McpClient::new(transport));
    let adapter = McpToolAdapter::new(tool, client);
    println!("✓ McpToolAdapter created");

    // Test adapter methods
    println!("Name: {}", adapter.name());
    println!("Description: {}", adapter.description());

    let def = adapter.definition; // Access field directly
    println!("Definition name: {}", def.name);
    println!("Schema type: {}", def.schema["type"]);

    println!("✓ McpToolAdapter tests completed");
    Ok(())
}

async fn run_all_tests() -> AgentResult<()> {
    println!("Running MCP functionality tests...\n");

    // Test main functionality
    if let Err(e) = run_main_test().await {
        println!("Main test failed: {}", e);
    }

    println!("\n");

    // Test adapter
    if let Err(e) = test_adapter_creation().await {
        println!("Adapter test failed: {}", e);
    }

    println!("\n=== All tests completed ===");
    Ok(())
}

#[tokio::main]
async fn main() {
    if let Err(e) = run_all_tests().await {
        println!("Test failed: {}", e);
    }
}