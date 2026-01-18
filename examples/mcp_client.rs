use agent_lib::mcp::{McpClient, McpTransport, TransportConfig};
use agent_lib::AgentResult;

#[tokio::main]
async fn main() -> AgentResult<()> {
    let transport = McpTransport::new(TransportConfig {
        endpoint: "stdio://mcp-server".to_string(),
    })
    .await?;
    let client = McpClient::new(transport);

    let tools = client.list_tools().await;
    println!("List tools result: {:?}", tools);

    Ok(())
}
