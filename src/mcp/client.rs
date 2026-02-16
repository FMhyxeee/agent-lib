use std::sync::Arc;
use std::time::Duration;

use rmcp::model::{
    CallToolRequestParams, CallToolResult, GetPromptRequestParams, GetPromptResult, Prompt,
    ReadResourceRequestParams, ReadResourceResult, Resource, Tool,
};
use tokio::sync::Mutex;

use crate::error::{AgentError, AgentResult};
use crate::mcp::config::ServerConfig;
use crate::mcp::transport::{RmcpClientService, connect_client};

#[derive(Debug, Clone)]
pub struct McpClient {
    service: Arc<Mutex<RmcpClientService>>,
}

impl McpClient {
    pub async fn connect(config: ServerConfig) -> AgentResult<Self> {
        let service = connect_client(&config).await?;
        Ok(Self {
            service: Arc::new(Mutex::new(service)),
        })
    }

    pub async fn list_tools(&self) -> AgentResult<Vec<Tool>> {
        self.list_tools_with_timeout(None).await
    }

    pub async fn list_tools_with_timeout(&self, timeout: Option<Duration>) -> AgentResult<Vec<Tool>> {
        self.with_timeout(timeout, async {
            let service = self.service.lock().await;
            service
                .list_all_tools()
                .await
                .map_err(map_service_error)
        })
        .await
    }

    pub async fn call_tool(&self, request: CallToolRequestParams) -> AgentResult<CallToolResult> {
        self.call_tool_with_timeout(request, None).await
    }

    pub async fn call_tool_with_timeout(
        &self,
        request: CallToolRequestParams,
        timeout: Option<Duration>,
    ) -> AgentResult<CallToolResult> {
        if request.arguments.is_none() {
            return Err(AgentError::Mcp(
                "call_tool requires arguments object; pass Some(Default::default()) for empty arguments"
                    .to_string(),
            ));
        }

        self.with_timeout(timeout, async {
            let service = self.service.lock().await;
            service.call_tool(request).await.map_err(map_service_error)
        })
        .await
    }

    pub async fn list_resources(&self) -> AgentResult<Vec<Resource>> {
        self.list_resources_with_timeout(None).await
    }

    pub async fn list_resources_with_timeout(
        &self,
        timeout: Option<Duration>,
    ) -> AgentResult<Vec<Resource>> {
        self.with_timeout(timeout, async {
            let service = self.service.lock().await;
            service
                .list_all_resources()
                .await
                .map_err(map_service_error)
        })
        .await
    }

    pub async fn read_resource(&self, request: ReadResourceRequestParams) -> AgentResult<ReadResourceResult> {
        self.read_resource_with_timeout(request, None).await
    }

    pub async fn read_resource_with_timeout(
        &self,
        request: ReadResourceRequestParams,
        timeout: Option<Duration>,
    ) -> AgentResult<ReadResourceResult> {
        self.with_timeout(timeout, async {
            let service = self.service.lock().await;
            service
                .read_resource(request)
                .await
                .map_err(map_service_error)
        })
        .await
    }

    pub async fn list_prompts(&self) -> AgentResult<Vec<Prompt>> {
        self.list_prompts_with_timeout(None).await
    }

    pub async fn list_prompts_with_timeout(
        &self,
        timeout: Option<Duration>,
    ) -> AgentResult<Vec<Prompt>> {
        self.with_timeout(timeout, async {
            let service = self.service.lock().await;
            service
                .list_all_prompts()
                .await
                .map_err(map_service_error)
        })
        .await
    }

    pub async fn get_prompt(&self, request: GetPromptRequestParams) -> AgentResult<GetPromptResult> {
        self.get_prompt_with_timeout(request, None).await
    }

    pub async fn get_prompt_with_timeout(
        &self,
        request: GetPromptRequestParams,
        timeout: Option<Duration>,
    ) -> AgentResult<GetPromptResult> {
        self.with_timeout(timeout, async {
            let service = self.service.lock().await;
            service.get_prompt(request).await.map_err(map_service_error)
        })
        .await
    }

    async fn with_timeout<T, F>(&self, timeout: Option<Duration>, future: F) -> AgentResult<T>
    where
        F: std::future::Future<Output = AgentResult<T>>,
    {
        match timeout {
            Some(duration) => tokio::time::timeout(duration, future)
                .await
                .map_err(|_| AgentError::Mcp(format!("timed out after {:?}", duration)))?,
            None => future.await,
        }
    }
}

fn map_service_error(error: rmcp::service::ServiceError) -> AgentError {
    AgentError::Mcp(error.to_string())
}
