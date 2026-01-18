use async_trait::async_trait;
use serde_json::json;
use serde_json::Value;

use crate::error::{AgentError, AgentResult};
use crate::tools::{Tool, ToolContext, ToolDef, ToolResult};

#[derive(Debug, Default)]
pub struct NetworkTool;

impl NetworkTool {
    pub fn new() -> Self {
        Self
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

    async fn execute(&self, _args: Value, _ctx: &ToolContext) -> AgentResult<ToolResult> {
        let method = _args
            .get("method")
            .and_then(|value| value.as_str())
            .ok_or_else(|| AgentError::Tool("missing method".to_string()))?;
        let url = _args
            .get("url")
            .and_then(|value| value.as_str())
            .ok_or_else(|| AgentError::Tool("missing url".to_string()))?;
        let body = _args.get("body").and_then(|value| value.as_str());

        let client = reqwest::Client::new();
        let request = match method.to_uppercase().as_str() {
            "GET" => client.get(url),
            "POST" => client.post(url),
            "PUT" => client.put(url),
            "DELETE" => client.delete(url),
            "PATCH" => client.patch(url),
            other => {
                return Err(AgentError::Tool(format!(
                    "unsupported method: {other}"
                )))
            }
        };

        let response = if let Some(body) = body {
            request.body(body.to_string()).send().await
        } else {
            request.send().await
        }
        .map_err(|err| AgentError::Tool(format!("request failed: {err}")))?;

        let status = response.status().as_u16();
        let text = response
            .text()
            .await
            .map_err(|err| AgentError::Tool(format!("read body failed: {err}")))?;

        Ok(ToolResult {
            output: json!({
                "status": status,
                "body": text,
            }),
        })
    }
}
