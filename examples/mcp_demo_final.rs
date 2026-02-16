//! MCP integration demo in strict official mode (stdio + streamable_http).

use agent_lib::mcp::{McpClient, McpManager, ServerConfig, TransportType};
use agent_lib::{AgentBuilder, AgentResult};
use std::time::Duration;

#[tokio::main]
async fn main() -> AgentResult<()> {
    println!("=== MCP Integration Demo ===\n");

    println!("1. McpManager creation");
    let manager = McpManager::new();
    println!("   manager created");
    println!("   server count: {}", manager.server_count().await);

    println!("\n2. Invalid endpoint handling");
    match manager.add_server("legacy", "tcp://localhost:9000").await {
        Ok(tools) => println!("   unexpected success with {} tools", tools.len()),
        Err(err) => println!("   expected error: {}", err),
    }

    println!("\n3. AgentBuilder with MCP");
    let _builder = AgentBuilder::new().with_mcp_server("invalid://test").await;
    println!("   builder handled invalid endpoint without panic");

    println!("\n4. McpClient creation with unsupported scheme");
    let bad_config = ServerConfig {
        name: "bad".to_string(),
        transport: TransportType::StreamableHttp,
        endpoint: "invalid://test".to_string(),
        command: None,
        args: Vec::new(),
        auth: None,
        headers: Default::default(),
        tls: None,
        timeout: Duration::from_secs(10),
        enabled: true,
        env: Default::default(),
    };
    match McpClient::connect(bad_config).await {
        Ok(_) => println!("   unexpected success"),
        Err(err) => println!("   expected client error: {}", err),
    }

    println!("\n=== Summary ===");
    println!("supported transports: stdio, streamable_http");
    println!("legacy transports are rejected with migration hints");

    Ok(())
}
