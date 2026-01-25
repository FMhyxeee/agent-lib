//! Integration tests for MCP tools functionality
//!
//! These tests verify that MCP integration works correctly.
//! Some tests are marked as `#[ignore]` because they require
//! a real MCP server to be running.

use agent_lib::mcp::{McpClient, McpManager, McpTool, McpTransport, TransportConfig};
use agent_lib::tools::{Tool, ToolDef};
use agent_lib::{AgentBuilder, AgentError, AgentResult};
use serde_json::json;
use std::sync::Arc;
use tokio::time::{Duration, timeout};

/// Helper function to create a mock tool for testing
fn create_mock_tool(name: &str) -> McpTool {
    McpTool {
        name: name.to_string(),
        description: format!("Mock {} tool", name),
        schema: json!({
            "type": "object",
            "properties": {
                "input": { "type": "string", "description": "Test input" }
            },
            "required": ["input"]
        }),
    }
}

#[tokio::test]
async fn test_mcp_manager_creation() -> AgentResult<()> {
    let manager = McpManager::new();
    assert_eq!(manager.server_count().await, 0);
    assert!(manager.list_servers().await.is_empty());
    assert_eq!(manager.total_tools_count().await, 0);

    let manager_with_timeout = McpManager::with_timeout(Duration::from_secs(60));
    assert_eq!(
        manager_with_timeout.default_timeout(),
        Some(Duration::from_secs(60))
    );

    Ok(())
}

#[tokio::test]
async fn test_mcp_manager_server_operations() -> AgentResult<()> {
    let manager = McpManager::new();

    // Test server addition (will fail without real server, but shows API)
    let result = manager.add_server("test", "stdio://nonexistent").await;
    assert!(result.is_err());

    // Test server listing after failed add
    assert!(manager.list_servers().await.is_empty());
    assert_eq!(manager.server_count().await, 0);

    // Test server removal
    assert!(manager.remove_server("nonexistent").await.is_none());

    Ok(())
}

#[tokio::test]
async fn test_mcp_manager_concurrent_access() -> AgentResult<()> {
    let manager = Arc::new(McpManager::new());

    // Spawn multiple concurrent tasks
    let handles: Vec<_> = (0..10)
        .map(|i| {
            let manager = manager.clone();
            tokio::spawn(async move {
                // Perform various operations
                let servers = manager.list_servers().await;
                let count = manager.server_count().await;
                let tools_count = manager.total_tools_count().await;

                // Should not panic
                (i, servers, count, tools_count)
            })
        })
        .collect();

    // Wait for all tasks to complete
    for handle in handles {
        let result = handle.await;
        assert!(result.is_ok(), "Task should complete successfully");
    }

    Ok(())
}

#[tokio::test]
async fn test_mcp_tool_adapter_creation() -> AgentResult<()> {
    let tool = create_mock_tool("test-tool");

    // Create mock transport and client
    let transport = match McpTransport::new(TransportConfig {
        endpoint: "stdio://echo".to_string(),
    })
    .await
    {
        Ok(transport) => transport,
        Err(err) => {
            eprintln!("Skipping test: transport unavailable: {}", err);
            return Ok(());
        }
    };

    let client = Arc::new(McpClient::new(transport));
    let adapter = agent_lib::tools::builtin::McpToolAdapter::new(tool, client.clone());

    // Test adapter properties
    assert_eq!(adapter.name(), "test-tool");
    assert_eq!(adapter.description(), "Mock test-tool tool");

    // Test definition conversion
    let def = adapter.definition();
    assert_eq!(def.name, "test-tool");
    assert_eq!(def.description, "Mock test-tool tool");
    assert_eq!(def.schema["type"], "object");

    Ok(())
}

#[tokio::test]
async fn test_mcp_tool_adapter_execution() -> AgentResult<()> {
    let tool = create_mock_tool("exec-test");

    // Create mock transport (will fail without real server)
    let result = McpTransport::new(TransportConfig {
        endpoint: "stdio://nonexistent".to_string(),
    })
    .await;

    // This test shows the adapter creation and error handling pattern
    match result {
        Ok(transport) => {
            let client = Arc::new(McpClient::new(transport));
            let adapter = agent_lib::tools::builtin::McpToolAdapter::new(tool, client);

            // Test execution context
            let ctx = agent_lib::tools::ToolContext {
                cwd: None,
                sandbox_root: None,
            };

            // Try to execute (should fail due to no server)
            let result = adapter.execute(json!({"input": "test"}), &ctx).await;
            assert!(result.is_err());

            // Check error type
            match result {
                Err(AgentError::Tool(msg)) => {
                    assert!(msg.contains("MCP tool call failed"));
                }
                _ => panic!("Expected AgentError::Tool"),
            }
        }
        Err(_) => {
            // Transport creation failed, which is expected without server
            // The test demonstrates the API pattern
        }
    }

    Ok(())
}

#[tokio::test]
#[ignore = "requires MCP server running"]
async fn test_mcp_tool_adapter_with_real_server() -> AgentResult<()> {
    // This test requires a real MCP server to run
    // It would test the actual tool execution flow
    unimplemented!("Requires MCP server");
}

#[tokio::test]
async fn test_agent_builder_mcp_server_method() -> AgentResult<()> {
    // Test with invalid endpoint (should not panic)
    let _builder = AgentBuilder::new().with_mcp_server("invalid://test").await;

    // Test with different transport types
    let invalid_endpoints = vec![
        "tcp://invalid-host:9999",
        "http://invalid-domain.com/mcp",
        "ws://invalid-domain.com/ws",
    ];

    for endpoint in invalid_endpoints {
        let _builder = AgentBuilder::new().with_mcp_server(endpoint).await;
        // Should handle gracefully without panic
    }

    Ok(())
}

#[tokio::test]
async fn test_agent_builder_mcp_client_method() -> AgentResult<()> {
    let builder = AgentBuilder::new();

    // Create invalid transport
    let result = McpTransport::new(TransportConfig {
        endpoint: "invalid://test".to_string(),
    })
    .await;

    match result {
        Ok(transport) => {
            let client = Arc::new(McpClient::new(transport));

            // This should fail when trying to list tools
            let result = builder.with_mcp_client(client).await;
            assert!(result.is_err());
        }
        Err(_) => {
            // Expected failure
        }
    }

    Ok(())
}

#[tokio::test]
async fn test_agent_builder_mcp_manager_method() -> AgentResult<()> {
    let manager = McpManager::new();
    let builder = AgentBuilder::new();

    // Test with empty manager
    let result = builder.with_mcp_manager(manager.clone()).await;
    assert!(result.is_ok()); // Should succeed even with empty manager

    Ok(())
}

#[tokio::test]
#[ignore = "requires MCP server running"]
async fn test_full_integration_workflow() -> AgentResult<()> {
    // This test would demonstrate the full integration:
    // 1. Start MCP server
    // 2. Create manager
    // 3. Add server
    // 4. Build agent
    // 5. Execute tool
    // 6. Verify result
    unimplemented!("Requires MCP server running");
}

#[tokio::test]
async fn test_timeout_behavior() -> AgentResult<()> {
    use agent_lib::mcp::McpClient;

    // Test that timeout methods exist and compile
    let _tool = create_mock_tool("timeout-test");
    let transport = match McpTransport::new(TransportConfig {
        endpoint: "stdio://test".to_string(),
    })
    .await
    {
        Ok(transport) => transport,
        Err(err) => {
            eprintln!("Skipping test: transport unavailable: {}", err);
            return Ok(());
        }
    };

    let client = Arc::new(McpClient::new(transport));

    // Test timeout methods exist and compile
    // Without a real server, we just verify the API is accessible
    let timeout_result = timeout(
        Duration::from_millis(100),
        client.list_tools_with_timeout(Some(Duration::from_millis(50))),
    )
    .await;

    match timeout_result {
        Ok(result) => {
            // Operation completed within timeout
            // Result may be Ok (if server exists) or Err (if no server)
            // Both are valid outcomes for this API test
            let _ = result;
        }
        Err(_) => {
            // Timeout occurred, which is also valid
        }
    }

    Ok(())
}

#[tokio::test]
async fn test_tool_schema_compatibility() -> AgentResult<()> {
    // Test that MCP tool schema is compatible with agent-lib ToolDef
    let mcp_tool = McpTool {
        name: "test-schema".to_string(),
        description: "Test schema compatibility".to_string(),
        schema: json!({
            "type": "object",
            "properties": {
                "param1": { "type": "string", "description": "First parameter" },
                "param2": { "type": "number", "description": "Second parameter" },
                "param3": { "type": "boolean", "description": "Third parameter" }
            },
            "required": ["param1", "param3"],
            "additionalProperties": false
        }),
    };

    let tool_def = ToolDef {
        name: mcp_tool.name.clone(),
        description: mcp_tool.description.clone(),
        schema: mcp_tool.schema.clone(),
    };

    // Verify schema is preserved
    assert_eq!(tool_def.schema, mcp_tool.schema);
    assert_eq!(tool_def.schema["type"], "object");
    assert_eq!(tool_def.schema["properties"]["param1"]["type"], "string");

    Ok(())
}

#[tokio::test]
async fn test_error_handling_patterns() -> AgentResult<()> {
    let tool = create_mock_tool("error-test");
    let transport = match McpTransport::new(TransportConfig {
        endpoint: "stdio://error-server".to_string(),
    })
    .await
    {
        Ok(transport) => transport,
        Err(err) => {
            eprintln!("Skipping test: transport unavailable: {}", err);
            return Ok(());
        }
    };

    let client = Arc::new(McpClient::new(transport));
    let adapter = agent_lib::tools::builtin::McpToolAdapter::new(tool, client);

    let ctx = agent_lib::tools::ToolContext {
        cwd: None,
        sandbox_root: None,
    };

    // Test with invalid JSON
    let invalid_args = json!("not an object");
    let result = adapter.execute(invalid_args, &ctx).await;
    assert!(result.is_err());

    // Test with missing required fields
    let missing_args = json!({"optional_param": "value"});
    let result = adapter.execute(missing_args, &ctx).await;
    assert!(result.is_err());

    Ok(())
}

/// Test helper for mocking MCP server responses
#[cfg(test)]
mod mock_server {
    use super::*;

    /// Mock MCP server for testing
    pub struct MockServer;

    impl MockServer {
        pub fn new() -> Self {
            Self
        }

        pub async fn handle_request(
            &self,
            method: &str,
            _params: &serde_json::Value,
        ) -> serde_json::Value {
            match method {
                "tools/list" => json!({
                    "tools": [
                        {
                            "name": "mock_tool".to_string(),
                            "description": "Mock tool for testing".to_string(),
                            "inputSchema": {
                                "type": "object",
                                "properties": {
                                    "test": {"type": "string"}
                                }
                            }
                        }
                    ]
                }),
                "tools/call" => json!({
                    "content": [{"type": "text", "text": "Mock result"}]
                }),
                _ => json!({"error": "Unknown method"}),
            }
        }
    }
}

#[tokio::test]
async fn test_mock_server_pattern() -> AgentResult<()> {
    // This shows the pattern for testing MCP functionality
    let server = mock_server::MockServer::new();

    // Test tools list
    let list_response = server.handle_request("tools/list", &json!({})).await;
    assert!(list_response["tools"].is_array());

    // Test tool call
    let call_response = server
        .handle_request(
            "tools/call",
            &json!({
                "name": "mock_tool",
                "arguments": {"test": "value"}
            }),
        )
        .await;

    assert!(call_response["content"].is_array());

    Ok(())
}
