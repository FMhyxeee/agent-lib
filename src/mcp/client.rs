use serde_json::Value;
use serde_json::json;
use std::time::Duration;

use crate::error::{AgentError, AgentResult};
use crate::mcp::{
    McpPrompt, McpPromptResult, McpRequest, McpResource, McpResourceContent, McpResponse,
    McpTool, McpToolCall, McpTransport,
};

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

    // === Resources (MCP 协议) ===

    /// 列出所有可用资源
    pub async fn list_resources(&self) -> AgentResult<Vec<McpResource>> {
        let response = self
            .send(McpRequest {
                id: "resources.list".to_string(),
                method: "resources/list".to_string(),
                params: json!({}),
            })
            .await?;

        let result = response.result;
        let resources = serde_json::from_value::<Vec<McpResource>>(result)
            .map_err(|err| AgentError::Mcp(format!("invalid resources list: {err}")))?;
        Ok(resources)
    }

    /// 读取资源内容
    pub async fn read_resource(&self, uri: String) -> AgentResult<McpResourceContent> {
        let response = self
            .send(McpRequest {
                id: "resources.read".to_string(),
                method: "resources/read".to_string(),
                params: json!({ "uri": uri }),
            })
            .await?;

        let result = response.result;
        let content = serde_json::from_value::<McpResourceContent>(result)
            .map_err(|err| AgentError::Mcp(format!("invalid resource content: {err}")))?;
        Ok(content)
    }

    // === Prompts (MCP 协议) ===

    /// 列出所有可用提示
    pub async fn list_prompts(&self) -> AgentResult<Vec<McpPrompt>> {
        let response = self
            .send(McpRequest {
                id: "prompts.list".to_string(),
                method: "prompts/list".to_string(),
                params: json!({}),
            })
            .await?;

        let result = response.result;
        let prompts = serde_json::from_value::<Vec<McpPrompt>>(result)
            .map_err(|err| AgentError::Mcp(format!("invalid prompts list: {err}")))?;
        Ok(prompts)
    }

    /// 获取提示内容
    pub async fn get_prompt(
        &self,
        name: String,
        arguments: Option<Value>,
    ) -> AgentResult<McpPromptResult> {
        let mut params = json!({ "name": name });
        if let Some(args) = arguments {
            params["arguments"] = args;
        }

        let response = self
            .send(McpRequest {
                id: "prompts.get".to_string(),
                method: "prompts/get".to_string(),
                params,
            })
            .await?;

        let result = response.result;
        let prompt_result = serde_json::from_value::<McpPromptResult>(result)
            .map_err(|err| AgentError::Mcp(format!("invalid prompt result: {err}")))?;
        Ok(prompt_result)
    }
}
