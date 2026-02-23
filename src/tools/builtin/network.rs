use async_trait::async_trait;
use serde_json::Value;
use serde_json::json;
use std::time::Duration;
use tokio::time::timeout;

use crate::error::{AgentError, AgentResult};
use crate::tools::{Tool, ToolContext, ToolDef, ToolResult};

#[derive(Debug, Default)]
pub struct NetworkTool;

const DEFAULT_TIMEOUT_SECS: u64 = 30;

impl NetworkTool {
    pub fn new() -> Self {
        Self
    }

    async fn execute_with_timeout(
        &self,
        args: Value,
        timeout_secs: u64,
    ) -> AgentResult<ToolResult> {
        self.execute_with_timeout_and_proxy(args, timeout_secs, false)
            .await
    }

    async fn execute_with_timeout_and_proxy(
        &self,
        args: Value,
        timeout_secs: u64,
        disable_proxy: bool,
    ) -> AgentResult<ToolResult> {
        let method = args
            .get("method")
            .and_then(|value| value.as_str())
            .ok_or_else(|| AgentError::Tool("missing method".to_string()))?;
        let url = args
            .get("url")
            .and_then(|value| value.as_str())
            .ok_or_else(|| AgentError::Tool("missing url".to_string()))?;
        let body = args.get("body").and_then(|value| value.as_str());

        let parsed_url = reqwest::Url::parse(url)
            .map_err(|err| AgentError::Tool(format!("invalid url: {err}")))?;
        match parsed_url.scheme() {
            "http" | "https" => {}
            scheme => {
                return Err(AgentError::Tool(format!(
                    "unsupported url scheme: {scheme}"
                )));
            }
        }

        let mut client_builder =
            reqwest::Client::builder().timeout(Duration::from_secs(timeout_secs));
        if disable_proxy {
            client_builder = client_builder.no_proxy();
        }
        let client = client_builder
            .build()
            .map_err(|err| AgentError::Tool(format!("client build failed: {err}")))?;
        let request = match method.to_uppercase().as_str() {
            "GET" => client.get(parsed_url.clone()),
            "POST" => client.post(parsed_url.clone()),
            "PUT" => client.put(parsed_url.clone()),
            "DELETE" => client.delete(parsed_url.clone()),
            "PATCH" => client.patch(parsed_url),
            other => return Err(AgentError::Tool(format!("unsupported method: {other}"))),
        };

        let request_operation = async {
            let response = if let Some(body) = body {
                request.body(body.to_string()).send().await
            } else {
                request.send().await
            }
            .map_err(|err| {
                if err.is_timeout() {
                    AgentError::Tool(format!("request timed out after {timeout_secs} seconds"))
                } else {
                    AgentError::Tool(format!("request failed: {err}"))
                }
            })?;

            let status = response.status().as_u16();
            let text = response.text().await.map_err(|err| {
                if err.is_timeout() {
                    AgentError::Tool(format!("request timed out after {timeout_secs} seconds"))
                } else {
                    AgentError::Tool(format!("read body failed: {err}"))
                }
            })?;

            Ok::<ToolResult, AgentError>(ToolResult {
                output: json!({
                    "status": status,
                    "body": text,
                }),
            })
        };

        match timeout(Duration::from_secs(timeout_secs), request_operation).await {
            Ok(result) => result,
            Err(_) => Err(AgentError::Tool(format!(
                "request timed out after {timeout_secs} seconds"
            ))),
        }
    }
}

#[async_trait]
impl Tool for NetworkTool {
    fn definition(&self) -> ToolDef {
        ToolDef {
            name: "network".to_string(),
            description: "Perform HTTP requests".to_string(),
            schema: json!({
                "type": "object",
                "properties": {
                    "method": { "type": "string" },
                    "url": { "type": "string" },
                    "body": { "type": "string" }
                },
                "required": ["method", "url"]
            }),
        }
    }

    async fn execute(&self, args: Value, _ctx: &ToolContext) -> AgentResult<ToolResult> {
        self.execute_with_timeout(args, DEFAULT_TIMEOUT_SECS).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::AsyncReadExt;
    use tokio::net::TcpListener;

    fn default_ctx() -> ToolContext {
        ToolContext {
            cwd: None,
            sandbox_root: None,
        }
    }

    #[tokio::test]
    async fn execute_fails_when_method_missing() {
        let tool = NetworkTool::new();
        let ctx = default_ctx();
        let err = tool.execute(json!({"url": "http://127.0.0.1"}), &ctx);
        assert!(err.await.is_err());
    }

    #[tokio::test]
    async fn execute_fails_when_url_missing() {
        let tool = NetworkTool::new();
        let ctx = default_ctx();
        let err = tool.execute(json!({"method": "GET"}), &ctx);
        assert!(err.await.is_err());
    }

    #[tokio::test]
    async fn execute_fails_for_unsupported_method() {
        let tool = NetworkTool::new();
        let ctx = default_ctx();
        let result = tool.execute(
            json!({
                "method": "TRACE",
                "url": "http://127.0.0.1"
            }),
            &ctx,
        );
        let err = result.await.expect_err("unsupported method should fail");
        assert!(err.to_string().contains("unsupported method"));
    }

    #[tokio::test]
    async fn execute_fails_for_invalid_url_scheme() {
        let tool = NetworkTool::new();
        let ctx = default_ctx();
        let result = tool.execute(
            json!({
                "method": "GET",
                "url": "file:///tmp/test.txt"
            }),
            &ctx,
        );
        let err = result.await.expect_err("invalid scheme should fail");
        assert!(err.to_string().contains("unsupported url scheme"));
    }

    #[tokio::test]
    async fn execute_times_out_when_server_hangs() {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("listener should bind");
        let addr = listener
            .local_addr()
            .expect("listener should have local addr");

        tokio::spawn(async move {
            if let Ok((mut socket, _)) = listener.accept().await {
                let mut buf = [0_u8; 1024];
                let _ = socket.read(&mut buf).await;
                tokio::time::sleep(Duration::from_secs(3)).await;
            }
        });

        let tool = NetworkTool::new();
        let result = tool
            .execute_with_timeout_and_proxy(
                json!({
                    "method": "GET",
                    "url": format!("http://{addr}/timeout")
                }),
                1,
                true,
            )
            .await;
        let err = result.expect_err("request should time out");
        let message = err.to_string();
        assert!(
            message.contains("timed out") || message.contains("deadline has elapsed"),
            "unexpected error message: {message}"
        );
    }
}
