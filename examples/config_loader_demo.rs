//! MCP configuration loader demo for strict official transport mode.

use agent_lib::mcp::{
    AuthConfig, AuthType, ConfigLoader, McpConfig, McpManager, ServerConfig, TransportType,
};
use std::path::Path;
use std::time::Duration;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== MCP Configuration Loader Demo ===\n");

    println!("1. Load TOML configuration");
    match load_toml_config().await {
        Ok(config) => {
            println!("  loaded {} servers", config.servers.len());
            for server in &config.servers {
                if server.enabled {
                    println!(
                        "  - {}: {} ({:?})",
                        server.name, server.endpoint, server.timeout
                    );
                }
            }
        }
        Err(err) => println!("  failed: {}", err),
    }

    println!("\n2. Load JSON configuration");
    match load_json_config().await {
        Ok(config) => println!("  loaded {} servers", config.servers.len()),
        Err(err) => println!("  failed: {}", err),
    }

    println!("\n3. Load configuration from environment variables");
    match load_env_config().await {
        Ok(config) => println!("  loaded {} servers", config.servers.len()),
        Err(err) => println!("  failed: {}", err),
    }

    println!("\n4. Load from common locations");
    match McpManager::from_common_locations().await {
        Ok(_) => println!("  loaded manager from common locations"),
        Err(err) => println!("  failed: {}", err),
    }

    println!("\n5. Build manager from file config");
    match load_into_manager().await {
        Ok(_) => println!("  manager build completed"),
        Err(err) => println!("  failed: {}", err),
    }

    println!("\n6. Validation examples");
    test_config_validation().await;

    println!("\n=== Done ===");
    Ok(())
}

async fn load_toml_config() -> Result<McpConfig, Box<dyn std::error::Error>> {
    let path = "examples/mcp_config.toml";
    if !Path::new(path).exists() {
        return Err("TOML config file not found".into());
    }
    Ok(ConfigLoader::from_toml_file(path).await?)
}

async fn load_json_config() -> Result<McpConfig, Box<dyn std::error::Error>> {
    let path = "examples/mcp_config.json";
    if !Path::new(path).exists() {
        return Err("JSON config file not found".into());
    }
    Ok(ConfigLoader::from_json_file(path).await?)
}

async fn load_env_config() -> Result<McpConfig, Box<dyn std::error::Error>> {
    unsafe {
        std::env::set_var("TEST_API_TOKEN", "test-token-12345");
        std::env::set_var("TEST_API_KEY", "test-key-67890");
    }

    let json_config = r#"
{
  "general": {
    "default_timeout": 30,
    "max_retries": 3
  },
  "servers": [
    {
      "name": "test-server",
      "transport": "http",
      "endpoint": "https://api.test.com/mcp",
      "auth": {
        "type": "bearer",
        "token": "${TEST_API_TOKEN}"
      },
      "headers": {
        "X-API-Key": "${TEST_API_KEY}"
      },
      "enabled": true
    }
  ]
}
"#;

    let temp_path = "temp_test_config.json";
    tokio::fs::write(temp_path, json_config).await?;
    let config = ConfigLoader::from_json_file(temp_path).await?;
    tokio::fs::remove_file(temp_path).await.ok();
    Ok(config)
}

async fn load_into_manager() -> Result<(), Box<dyn std::error::Error>> {
    let path = "examples/mcp_config.toml";
    if !Path::new(path).exists() {
        return Err("TOML config file not found".into());
    }

    let config = ConfigLoader::from_toml_file(path).await?;
    let manager = McpManager::from_config(config).await?;

    println!("  servers: {}", manager.server_count().await);
    println!("  tools: {}", manager.total_tools_count().await);

    for server_name in manager.list_servers().await {
        if let Some(tools) = manager.get_server_tools(&server_name).await {
            println!("  - {}: {} tools", server_name, tools.len());
        }
    }

    Ok(())
}

async fn test_config_validation() {
    let mut valid_config = McpConfig::default();
    valid_config.servers.push(ServerConfig {
        name: "valid-http".to_string(),
        transport: TransportType::StreamableHttp,
        endpoint: "https://example.com/mcp".to_string(),
        command: None,
        args: Vec::new(),
        auth: None,
        headers: Default::default(),
        tls: None,
        timeout: Duration::from_secs(30),
        enabled: true,
        env: Default::default(),
    });

    match valid_config.validate() {
        Ok(_) => println!("  valid config passed"),
        Err(err) => println!("  valid config failed: {}", err),
    }

    let mut duplicate_name_config = McpConfig::default();
    duplicate_name_config.servers.push(ServerConfig {
        name: "duplicate".to_string(),
        transport: TransportType::StreamableHttp,
        endpoint: "https://one.example.com/mcp".to_string(),
        command: None,
        args: Vec::new(),
        auth: None,
        headers: Default::default(),
        tls: None,
        timeout: Duration::from_secs(30),
        enabled: true,
        env: Default::default(),
    });
    duplicate_name_config.servers.push(ServerConfig {
        name: "duplicate".to_string(),
        transport: TransportType::Stdio,
        endpoint: "stdio://echo-server".to_string(),
        command: None,
        args: Vec::new(),
        auth: None,
        headers: Default::default(),
        tls: None,
        timeout: Duration::from_secs(30),
        enabled: true,
        env: Default::default(),
    });

    match duplicate_name_config.validate() {
        Ok(_) => println!("  duplicate-name config should have failed"),
        Err(err) => println!("  duplicate-name config failed as expected: {}", err),
    }

    let mut invalid_auth_config = McpConfig::default();
    invalid_auth_config.servers.push(ServerConfig {
        name: "invalid-auth".to_string(),
        transport: TransportType::StreamableHttp,
        endpoint: "https://example.com/mcp".to_string(),
        command: None,
        args: Vec::new(),
        auth: Some(AuthConfig {
            auth_type: AuthType::Bearer,
            token: None,
            username: None,
            password: None,
            api_key: None,
            api_key_header: None,
            query_param: None,
            token_url: None,
            client_id: None,
            client_secret: None,
            scope: None,
            audience: None,
        }),
        headers: Default::default(),
        tls: None,
        timeout: Duration::from_secs(30),
        enabled: true,
        env: Default::default(),
    });

    match invalid_auth_config.validate() {
        Ok(_) => println!("  invalid-auth config should have failed"),
        Err(err) => println!("  invalid-auth config failed as expected: {}", err),
    }
}
