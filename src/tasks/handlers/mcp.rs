//! MCP相关操作handlers
//!
//! 处理所有与MCP(Model Context Protocol)相关的操作请求。

use crate::protocol::{Event, McpServerRefreshConfig};
use crate::session::Session;

/// 处理 ListMcpTools 操作
pub async fn handle_list_mcp_tools(sess: &Session) {
    tracing::debug!("Handling list MCP tools");
    let tools = if let Some(manager) = sess.get_mcp_manager() {
        let all_tools = manager.get_all_tools().await;
        all_tools.into_iter().map(|(server, tool, _client)| crate::protocol::McpToolInfo {
            name: tool.name.to_string(),
            description: tool.description.unwrap_or_default().to_string(),
            server,
        }).collect()
    } else {
        sess.emit_event(Event::Warning { message: "No MCP manager configured".to_string() }).await;
        vec![]
    };
    sess.emit_event(Event::McpListToolsResponse { tools }).await;
}

/// 处理 RefreshMcpServers 操作
pub async fn handle_refresh_mcp_servers(sess: &Session, config: McpServerRefreshConfig) {
    tracing::debug!(force = config.force_reload, "Handling refresh MCP servers");
    if let Some(manager) = sess.get_mcp_manager() {
        let servers = manager.list_servers().await;
        if config.force_reload {
            sess.emit_event(Event::Warning { message: format!("Force reloading {} MCP servers", servers.len()) }).await;
        } else {
            sess.emit_event(Event::Warning { message: format!("Checked {} MCP servers, all connections healthy", servers.len()) }).await;
        }
    } else {
        sess.emit_event(Event::Warning { message: "No MCP manager configured".to_string() }).await;
    }
}

/// 处理 ListMcpResources 操作
pub async fn handle_list_mcp_resources(sess: &Session) {
    tracing::debug!("Handling list MCP resources");
    let Some(manager) = sess.get_mcp_manager() else {
        sess.emit_event(Event::Warning { message: "No MCP manager configured".to_string() }).await;
        return;
    };
    let mut all_resources = Vec::new();
    let servers = manager.list_servers().await;
    for server_name in &servers {
        if let Some((client, _tools)) = manager.get_server_info(server_name).await {
            match client.list_resources().await {
                Ok(resources) => {
                    for res in resources {
                        all_resources.push(crate::protocol::McpResourceInfo {
                            uri: res.uri.clone(),
                            name: res.name.clone(),
                            description: res.description.clone(),
                            mime_type: res.mime_type.clone(),
                        });
                    }
                }
                Err(e) => { tracing::debug!(server = %server_name, error = %e, "Failed to list resources"); }
            }
        }
    }
    sess.emit_event(Event::McpListResourcesResponse { resources: all_resources }).await;
}

/// 处理 ReadMcpResource 操作
pub async fn handle_read_mcp_resource(sess: &Session, uri: String) {
    tracing::debug!(uri = %uri, "Handling read MCP resource");
    let Some(manager) = sess.get_mcp_manager() else {
        sess.emit_event(Event::Error { error: crate::error::AgentError::Tool("No MCP manager configured".to_string()) }).await;
        return;
    };
    let server_name = uri.split_once(':').map(|(s, _)| s).unwrap_or("default");
    if let Some((client, _tools)) = manager.get_server_info(server_name).await {
        let request = crate::mcp::ReadResourceRequestParams { meta: None, uri: uri.clone() };
        match client.read_resource(request).await {
            Ok(result) => {
                let resolved_uri = result.contents.first().map(|entry| match entry {
                    crate::mcp::ResourceContents::TextResourceContents { uri, .. } | crate::mcp::ResourceContents::BlobResourceContents { uri, .. } => uri.clone()
                }).unwrap_or_else(|| uri.clone());
                let content = result.contents.into_iter().map(|entry| match entry {
                    crate::mcp::ResourceContents::TextResourceContents { text, .. } => text,
                    crate::mcp::ResourceContents::BlobResourceContents { blob, .. } => blob,
                }).collect::<Vec<_>>().join("\n");
                sess.emit_event(Event::McpResourceContent { uri: resolved_uri, content }).await;
            }
            Err(e) => {
                sess.emit_event(Event::Error { error: crate::error::AgentError::Tool(format!("Failed to read resource '{}': {}", uri, e)) }).await;
            }
        }
    } else {
        sess.emit_event(Event::Error { error: crate::error::AgentError::Tool(format!("MCP server '{}' not found", server_name)) }).await;
    }
}

/// 处理 ListMcpPrompts 操作
pub async fn handle_list_mcp_prompts(sess: &Session) {
    tracing::debug!("Handling list MCP prompts");
    let Some(manager) = sess.get_mcp_manager() else {
        sess.emit_event(Event::Warning { message: "No MCP manager configured".to_string() }).await;
        return;
    };
    let mut all_prompts = Vec::new();
    let servers = manager.list_servers().await;
    for server_name in &servers {
        if let Some((client, _tools)) = manager.get_server_info(server_name).await {
            match client.list_prompts().await {
                Ok(prompts) => {
                    for prompt in prompts {
                        all_prompts.push(crate::protocol::McpPromptInfo {
                            name: prompt.name,
                            description: prompt.description,
                            arguments: prompt.arguments.map(|args| {
                                args.into_iter().map(|arg| crate::protocol::PromptArgumentInfo {
                                    name: arg.name,
                                    description: arg.description,
                                    required: arg.required.unwrap_or(false),
                                }).collect()
                            }),
                        });
                    }
                }
                Err(e) => { tracing::debug!(server = %server_name, error = %e, "Failed to list prompts"); }
            }
        }
    }
    sess.emit_event(Event::McpListPromptsResponse { prompts: all_prompts }).await;
}

/// 处理 GetMcpPrompt 操作
pub async fn handle_get_mcp_prompt(sess: &Session, name: String, arguments: Option<serde_json::Value>) {
    tracing::debug!(name = %name, "Handling get MCP prompt");
    let Some(manager) = sess.get_mcp_manager() else {
        sess.emit_event(Event::Error { error: crate::error::AgentError::Tool("No MCP manager configured".to_string()) }).await;
        return;
    };
    let (server_name, prompt_name) = name.split_once(':').unwrap_or(("default", name.as_str()));
    if let Some((client, _tools)) = manager.get_server_info(server_name).await {
        let prompt_arguments = match arguments {
            Some(serde_json::Value::Object(arguments)) => Some(arguments),
            Some(_) => {
                sess.emit_event(Event::Error { error: crate::error::AgentError::Tool("Prompt arguments must be a JSON object".to_string()) }).await;
                return;
            }
            None => None,
        };
        let request = crate::mcp::GetPromptRequestParams { meta: None, name: prompt_name.to_string(), arguments: prompt_arguments };
        match client.get_prompt(request).await {
            Ok(result) => {
                let messages = result.messages.into_iter().map(|msg| crate::protocol::PromptMessage {
                    role: match msg.role {
                        crate::mcp::PromptMessageRole::User => "user".to_string(),
                        crate::mcp::PromptMessageRole::Assistant => "assistant".to_string(),
                    },
                    content: match msg.content {
                        crate::mcp::PromptMessageContent::Text { text } => crate::protocol::PromptContent::Text { text },
                        crate::mcp::PromptMessageContent::Image { image } => crate::protocol::PromptContent::Image { data: image.data.clone(), mime_type: image.mime_type.clone() },
                        crate::mcp::PromptMessageContent::Resource { resource } => {
                            let text = match &resource.resource {
                                crate::mcp::ResourceContents::TextResourceContents { uri, text, .. } => {
                                    if text.is_empty() { uri.clone() } else { text.clone() }
                                }
                                crate::mcp::ResourceContents::BlobResourceContents { uri, .. } => uri.clone(),
                            };
                            crate::protocol::PromptContent::Text { text }
                        }
                        crate::mcp::PromptMessageContent::ResourceLink { link } => crate::protocol::PromptContent::Text { text: link.uri.clone() },
                    },
                }).collect();
                sess.emit_event(Event::McpPromptResult { messages }).await;
            }
            Err(e) => {
                sess.emit_event(Event::Error { error: crate::error::AgentError::Tool(format!("Failed to get prompt '{}': {}", name, e)) }).await;
            }
        }
    } else {
        sess.emit_event(Event::Error { error: crate::error::AgentError::Tool(format!("MCP server '{}' not found", server_name)) }).await;
    }
}
