//! MCP Configuration Loader Demo
//!
//! This example demonstrates how to load MCP server configurations from various sources:
//! - TOML files
//! - JSON files
//! - Environment variables
//! - Common configuration locations

use agent_lib::mcp::{
    AuthConfig, AuthType, ConfigLoader, McpConfig, McpManager, ServerConfig, TransportConfig,
    TransportType,
};
use std::path::Path;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== MCP Configuration Loader Demo ===\n");

    // Test 1: Load from TOML file
    println!("1. Testing TOML configuration loading...");
    match load_toml_config().await {
        Ok(config) => {
            println!("✓ TOML configuration loaded successfully");
            println!("   Servers: {}", config.servers.len());
            for server in &config.servers {
                if server.enabled {
                    println!(
                        "   - {}: {} (timeout: {:?})",
                        server.name, server.transport_config.endpoint, server.timeout
                    );
                }
            }
        }
        Err(e) => println!("✗ TOML loading failed: {}", e),
    }

    println!();

    // Test 2: Load from JSON file
    println!("2. Testing JSON configuration loading...");
    match load_json_config().await {
        Ok(config) => {
            println!("✓ JSON configuration loaded successfully");
            println!("   Servers: {}", config.servers.len());
        }
        Err(e) => println!("✗ JSON loading failed: {}", e),
    }

    println!();

    // Test 3: Load from environment variables
    println!("3. Testing environment variable configuration...");
    match load_env_config().await {
        Ok(config) => {
            println!("✓ Environment configuration loaded successfully");
            println!("   Servers: {}", config.servers.len());
        }
        Err(e) => println!("✗ Environment loading failed: {}", e),
    }

    println!();

    // Test 4: Load from common locations
    println!("4. Testing configuration from common locations...");
    match load_from_common_locations().await {
        Ok(_) => println!("✓ Configuration loaded from common locations"),
        Err(e) => println!("✗ Common locations loading failed: {}", e),
    }

    println!();

    // Test 5: Load into McpManager
    println!("5. Testing McpManager configuration loading...");
    match load_into_manager().await {
        Ok(_) => println!("✓ McpManager loaded configuration successfully"),
        Err(e) => println!("✗ McpManager loading failed: {}", e),
    }

    println!();

    // Test 6: Configuration validation
    println!("6. Testing configuration validation...");
    test_config_validation().await;

    println!("\n=== Configuration Demo Completed ===");
    Ok(())
}

async fn load_toml_config() -> Result<McpConfig, Box<dyn std::error::Error>> {
    let config_path = "examples/mcp_config.toml";

    if !Path::new(config_path).exists() {
        return Err("TOML config file not found".into());
    }

    let config = ConfigLoader::from_toml_file(config_path).await?;
    Ok(config)
}

async fn load_json_config() -> Result<McpConfig, Box<dyn std::error::Error>> {
    let config_path = "examples/mcp_config.json";

    if !Path::new(config_path).exists() {
        return Err("JSON config file not found".into());
    }

    let config = ConfigLoader::from_json_file(config_path).await?;
    Ok(config)
}

async fn load_env_config() -> Result<McpConfig, Box<dyn std::error::Error>> {
    // Set some test environment variables
    unsafe {
        std::env::set_var("TEST_API_TOKEN", "test-token-12345");
        std::env::set_var("TEST_API_KEY", "test-key-67890");
    }

    // Create a JSON config with environment variables
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

    // Write test config to temporary file
    let temp_path = "temp_test_config.json";
    tokio::fs::write(temp_path, json_config).await?;

    let config = ConfigLoader::from_json_file(temp_path).await?;

    // Clean up
    tokio::fs::remove_file(temp_path).await.ok();

    Ok(config)
}

async fn load_from_common_locations() -> Result<(), Box<dyn std::error::Error>> {
    match McpManager::from_common_locations().await {
        Ok(_manager) => Ok(()),
        Err(e) => Err(e.into()),
    }
}

async fn load_into_manager() -> Result<(), Box<dyn std::error::Error>> {
    // Load TOML configuration
    let config_path = "examples/mcp_config.toml";
    if !Path::new(config_path).exists() {
        return Err("TOML config file not found".into());
    }

    let config = ConfigLoader::from_toml_file(config_path).await?;
    let manager = McpManager::from_config(config).await?;

    println!(
        "   Manager created with {} servers",
        manager.server_count().await
    );
    println!(
        "   Total tools across all servers: {}",
        manager.total_tools_count().await
    );

    // List servers
    let servers = manager.list_servers().await;
    for server_name in servers {
        if let Some(tools) = manager.get_server_tools(&server_name).await {
            println!("   - {}: {} tools", server_name, tools.len());
        }
    }

    Ok(())
}

async fn test_config_validation() {
    // Test valid configuration
    let mut valid_config = McpConfig::default();
    valid_config.servers.push(ServerConfig {
        name: "test".to_string(),
        transport: TransportType::Http,
        transport_config: TransportConfig {
            endpoint: "http://example.com".to_string(),
        },
        auth: None,
        headers: std::collections::HashMap::new(),
        tls: None,
        timeout: std::time::Duration::from_secs(30),
        enabled: true,
        env: std::collections::HashMap::new(),
    });

    match valid_config.validate() {
        Ok(_) => println!("✓ Valid configuration validation passed"),
        Err(e) => println!("✗ Valid configuration validation failed: {}", e),
    }

    // Test invalid configuration (duplicate server names)
    let mut invalid_config = McpConfig::default();
    invalid_config.servers.push(ServerConfig {
        name: "duplicate".to_string(),
        transport: TransportType::Http,
        transport_config: TransportConfig {
            endpoint: "http://example1.com".to_string(),
        },
        auth: None,
        headers: std::collections::HashMap::new(),
        tls: None,
        timeout: std::time::Duration::from_secs(30),
        enabled: true,
        env: std::collections::HashMap::new(),
    });

    invalid_config.servers.push(ServerConfig {
        name: "duplicate".to_string(), // Duplicate name
        transport: TransportType::Http,
        transport_config: TransportConfig {
            endpoint: "http://example2.com".to_string(),
        },
        auth: None,
        headers: std::collections::HashMap::new(),
        tls: None,
        timeout: std::time::Duration::from_secs(30),
        enabled: true,
        env: std::collections::HashMap::new(),
    });

    match invalid_config.validate() {
        Ok(_) => println!("✗ Invalid configuration validation should have failed"),
        Err(e) => println!("✓ Invalid configuration validation correctly failed: {}", e),
    }

    // Test authentication validation
    let mut invalid_auth_config = McpConfig::default();
    invalid_auth_config.servers.push(ServerConfig {
        name: "invalid-auth".to_string(),
        transport: TransportType::Http,
        transport_config: TransportConfig {
            endpoint: "http://example.com".to_string(),
        },
        auth: Some(AuthConfig {
            auth_type: AuthType::Bearer,
            token: None, // Missing token for Bearer auth
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
        headers: std::collections::HashMap::new(),
        tls: None,
        timeout: std::time::Duration::from_secs(30),
        enabled: true,
        env: std::collections::HashMap::new(),
    });

    match invalid_auth_config.validate() {
        Ok(_) => println!("✗ Invalid authentication validation should have failed"),
        Err(e) => println!(
            "✓ Invalid authentication validation correctly failed: {}",
            e
        ),
    }
}

// Environment variable expansion test
#[allow(dead_code)]
async fn test_env_expansion() {
    println!("\n7. Testing environment variable expansion...");

    // Set test environment variables
    unsafe {
        std::env::set_var("TEST_SERVER_NAME", "expanded-server");
        std::env::set_var("TEST_ENDPOINT", "http://expanded.example.com");
        std::env::set_var("TEST_TOKEN", "expanded-token-123");
    }

    // Test pattern: ${VAR}
    let result1 = McpConfig::expand_env_vars("prefix-${TEST_SERVER_NAME}-suffix");
    assert_eq!(result1, "prefix-expanded-server-suffix");

    // Test pattern: $VAR
    let result2 = McpConfig::expand_env_vars("$TEST_ENDPOINT/api");
    assert_eq!(result2, "http://expanded.example.com/api");

    // Test non-existent variable
    let result3 = McpConfig::expand_env_vars("${NONEXISTENT_VAR}");
    assert_eq!(result3, "${NONEXISTENT_VAR}");

    println!("✓ Environment variable expansion tests passed");

    // Clean up
    unsafe {
        std::env::remove_var("TEST_SERVER_NAME");
        std::env::remove_var("TEST_ENDPOINT");
        std::env::remove_var("TEST_TOKEN");
    }
}
