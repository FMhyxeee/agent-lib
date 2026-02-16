//! Integration-oriented MCP tests for the rmcp typed API.

use agent_lib::mcp::{
    CallToolRequestParams, GetPromptRequestParams, McpManager, ReadResourceRequestParams, Tool,
};
use agent_lib::{AgentBuilder, AgentResult};
use serde_json::json;
use std::time::Duration;

fn create_mock_tool(name: &str) -> Tool {
    let schema = json!({
        "type": "object",
        "properties": {
            "input": { "type": "string", "description": "Test input" }
        },
        "required": ["input"]
    });

    Tool::new(
        name.to_string(),
        format!("Mock {} tool", name),
        schema.as_object().cloned().unwrap_or_default(),
    )
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
async fn test_mcp_manager_rejects_legacy_transport_endpoint() -> AgentResult<()> {
    let manager = McpManager::new();
    let err = manager
        .add_server("legacy", "tcp://localhost:9000")
        .await
        .expect_err("legacy transports must be rejected");

    let message = err.to_string();
    assert!(message.contains("Unsupported transport 'tcp'"));
    assert!(message.contains("Supported: stdio, streamable_http"));
    assert!(message.contains("http/https -> streamable_http"));
    assert!(message.contains("tcp/ws/wss/sse are removed in strict official mode"));

    Ok(())
}

#[tokio::test]
async fn test_mcp_manager_rejects_unknown_endpoint_scheme() -> AgentResult<()> {
    let manager = McpManager::new();
    let err = manager
        .add_server("unknown", "ftp://localhost/resource")
        .await
        .expect_err("unknown schemes must be rejected");

    assert!(
        err.to_string()
            .contains("Supported endpoint prefixes: stdio://, http://, https://")
    );
    Ok(())
}

#[tokio::test]
async fn test_mcp_manager_concurrent_access() -> AgentResult<()> {
    let manager = McpManager::new();
    let handles: Vec<_> = (0..10)
        .map(|_| {
            let manager = manager.clone();
            tokio::spawn(async move {
                let _ = manager.list_servers().await;
                let _ = manager.server_count().await;
                let _ = manager.total_tools_count().await;
            })
        })
        .collect();

    for handle in handles {
        handle.await.expect("task should complete");
    }

    Ok(())
}

#[tokio::test]
async fn test_agent_builder_mcp_server_method() -> AgentResult<()> {
    // Should gracefully ignore unsupported/invalid endpoints.
    let _builder = AgentBuilder::new()
        .with_mcp_server("invalid://nonexistent")
        .await;
    let _builder = AgentBuilder::new()
        .with_mcp_server("tcp://127.0.0.1:9999")
        .await;
    let _builder = AgentBuilder::new()
        .with_mcp_server("http://invalid-domain.local/mcp")
        .await;

    Ok(())
}

#[tokio::test]
async fn test_agent_builder_mcp_manager_method() -> AgentResult<()> {
    let manager = McpManager::new();
    let builder = AgentBuilder::new();
    let result = builder.with_mcp_manager(manager).await;
    assert!(result.is_ok());
    Ok(())
}

#[test]
fn test_typed_request_params_shape() {
    let call = CallToolRequestParams {
        meta: None,
        name: "calculator".into(),
        arguments: Some(
            json!({
                "a": 1,
                "b": 2
            })
            .as_object()
            .cloned()
            .unwrap_or_default(),
        ),
        task: None,
    };
    assert_eq!(call.name, "calculator");
    assert!(call.arguments.is_some());

    let read = ReadResourceRequestParams {
        meta: None,
        uri: "file:///tmp/example.txt".to_string(),
    };
    assert_eq!(read.uri, "file:///tmp/example.txt");

    let prompt = GetPromptRequestParams {
        meta: None,
        name: "summarize".to_string(),
        arguments: Some(
            json!({
                "style": "brief"
            })
            .as_object()
            .cloned()
            .unwrap_or_default(),
        ),
    };
    assert_eq!(prompt.name, "summarize");
}

#[test]
fn test_tool_schema_compatibility() {
    let mcp_tool = create_mock_tool("schema");
    let schema = serde_json::Value::Object((*mcp_tool.input_schema).clone());
    assert_eq!(schema["type"], "object");
    assert_eq!(schema["properties"]["input"]["type"], "string");
}
