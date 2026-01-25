use serde_json::Value;
use serde_json::json;
use std::time::Duration;

use crate::error::{AgentError, AgentResult};
use crate::mcp::{McpRequest, McpResponse, McpTool, McpToolCall, McpTransport};

#[derive(Debug)]
pub struct McpClient {
    transport: McpTransport,
}

impl McpClient {
    pub fn new(transport: McpTransport) -> Self {
        Self { transport }
    }

    pub async fn list_tools(&self) -> AgentResult<Vec<McpTool>> {
        self.list_tools_with_timeout(None).await
    }

    pub async fn list_tools_with_timeout(
        &self,
        timeout: Option<Duration>,
    ) -> AgentResult<Vec<McpTool>> {
        let fut = self.list_tools_internal();

        match timeout {
            Some(duration) => tokio::time::timeout(duration, fut)
                .await
                .map_err(|_| AgentError::Mcp(format!("timed out after {:?}", duration)))?,
            None => fut.await,
        }
    }

    /// List tools without timeout wrapper (for internal use)
    async fn list_tools_internal(&self) -> AgentResult<Vec<McpTool>> {
        let response = self
            .send(McpRequest {
                id: "tools.list".to_string(),
                method: "tools/list".to_string(),
                params: json!({}),
            })
            .await?;

        let tools: Vec<McpTool> = serde_json::from_value(response.result)
            .map_err(|err| AgentError::Mcp(format!("invalid tools list: {err}")))?;
        Ok(tools)
    }

    pub async fn call_tool(&self, call: McpToolCall) -> AgentResult<Value> {
        self.call_tool_with_timeout(call, None).await
    }

    pub async fn call_tool_with_timeout(
        &self,
        call: McpToolCall,
        timeout: Option<Duration>,
    ) -> AgentResult<Value> {
        let fut = self.call_tool_internal(call);

        match timeout {
            Some(duration) => tokio::time::timeout(duration, fut).await.map_err(|_| {
                AgentError::Mcp(format!("tool call timed out after {:?}", duration))
            })?,
            None => fut.await,
        }
    }

    /// Call tool without timeout wrapper (for internal use)
    async fn call_tool_internal(&self, call: McpToolCall) -> AgentResult<Value> {
        let response = self
            .send(McpRequest {
                id: "tools.call".to_string(),
                method: "tools/call".to_string(),
                params: json!({
                    "name": call.name,
                    "args": call.args,
                }),
            })
            .await?;

        Ok(response.result)
    }

    async fn send(&self, _request: McpRequest) -> AgentResult<McpResponse> {
        self.transport.send(_request).await
    }
}
