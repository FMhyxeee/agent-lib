//! MCP Configuration Integration Example
//!
//! This example shows how to use MCP configuration files with agent-lib.
//! It demonstrates different ways to load and use MCP server configurations.

use agent_lib::mcp::{ConfigLoader, McpManager};
use agent_lib::{AgentBuilder, AgentResult};
use std::sync::Arc;
use std::env;

async fn load_and_use_mcp_config() -> AgentResult<()> {
    println!("=== MCP Configuration Integration Example ===\n");

    // Method 1: Load from TOML file
    println!("1. Loading from TOML configuration file...");
    let manager = match McpManager::from_config_file("examples/mcp_config.toml").await {
        Ok(manager) => {
            println!("✓ Loaded {} MCP servers from TOML configuration", manager.server_count().await);
            manager
        }
        Err(e) => {
            println!("⚠ Failed to load TOML config: {}", e);
            println!("   Falling back to empty manager");
            McpManager::new()
        }
    };

    // Method 2: Load from JSON file
    println!("\n2. Loading from JSON configuration file...");
    match McpManager::from_config_json("examples/mcp_config.json").await {
        Ok(manager) => {
            println!("✓ Loaded {} MCP servers from JSON configuration", manager.server_count().await);
        }
        Err(e) => {
            println!("⚠ Failed to load JSON config: {}", e);
        }
    }

    // Method 3: Load from environment variables
    println!("\n3. Loading from environment variables...");
    match McpManager::from_env().await {
        Ok(manager) => {
            println!("✓ Loaded {} MCP servers from environment variables", manager.server_count().await);
        }
        Err(e) => {
            println!("⚠ No environment configuration found: {}", e);
        }
    }

    // Method 4: Auto-detect from common locations
    println!("\n4. Auto-detecting configuration from common locations...");
    match McpManager::from_common_locations().await {
        Ok(manager) => {
            println!("✓ Loaded {} MCP servers from common locations", manager.server_count().await);
        }
        Err(e) => {
            println!("⚠ No configuration found in common locations: {}", e);
            println!("   Common locations checked:");
            println!("   - /etc/agent-lib/mcp.{{toml,json}}");
            println!("   - ~/.config/agent-lib/mcp.{{toml,json}}");
            println!("   - ./mcp.{{toml,json}}");
        }
    }

    // Method 5: Create agent with loaded MCP servers
    println!("\n5. Creating agent with MCP configuration...");
    println!("\nAvailable servers in loaded configuration:");

    // List all servers
    let servers = manager.list_servers().await;
    for server_name in &servers {
        if let Some(tools) = manager.get_server_tools(server_name).await {
            println!("  - {}: {} tools", server_name, tools.len());
        } else {
            println!("  - {}: connected", server_name);
        }
    }

    if servers.is_empty() {
        println!("  No MCP servers configured. This is normal if no configuration files exist.");
        println!("\nTo try MCP configuration:");
        println!("  1. Copy examples/mcp_config.toml to your current directory");
        println!("  2. Set environment variables for authentication:");
        println!("     export API_TOKEN=your-token-here");
        println!("     export API_KEY=your-api-key-here");
        println!("  3. Run this example again");
    } else {
        println!("\nTo create an agent with MCP integration:");
        println!("  let agent = AgentBuilder::new()");
        println!("      .with_model(your_model_provider)");
        println!("      .with_mcp_manager(manager.clone())");
        println!("      .build()?");

        // Example of creating an agent (requires model provider)
        println!("\nAttempting to create agent (requires OpenAI API key)...");
        match env::var("OPENAI_API_KEY") {
            Ok(_key) => {
                // Uncomment to test actual agent creation
                /*
                let agent = AgentBuilder::new()
                    .with_mcp_manager(manager.clone())
                    .build()?;
                println!("✓ Agent created successfully with MCP integration");
                */
                println!("ℹ Agent creation requires OpenAI API key (set OPENAI_API_KEY environment variable)");
            }
            Err(_) => {
                println!("ℹ To test agent creation, set OpenAI API key:");
                println!("   export OPENAI_API_KEY=your-openai-key");
            }
        }
    }

    // Method 6: Show configuration file structure
    println!("\n6. Configuration file structure:");
    println!("TOML format (examples/mcp_config.toml):");
    println!("  [general]");
    println!("  default_timeout = 30");
    println!("  max_retries = 3");
    println!("  ");
    println!("  [[servers]]");
    println!("  name = \"filesystem\"");
    println!("  transport = \"stdio\"");
    println!("  command = \"npx\"");
    println!("  args = [\"@modelcontextprotocol/server-filesystem\"]");
    println!("  ");
    println!("  [[servers]]");
    println!("  name = \"remote-api\"");
    println!("  transport = \"http\"");
    println!("  endpoint = \"https://api.example.com/mcp\"");
    println!("  auth = {{ type = \"bearer\", token = \"${{API_TOKEN}}\" }}");
    println!("  headers = {{ \"X-API-Key\" = \"${{API_KEY}}\" }}");
    println!("  ");
    println!("JSON format (examples/mcp_config.json):");
    println!("  {{");
    println!("    \"general\": {{");
    println!("      \"default_timeout\": 30,");
    println!("      \"max_retries\": 3");
    println!("    }},");
    println!("    \"servers\": [");
    println!("      {{");
    println!("        \"name\": \"filesystem\",");
    println!("        \"transport\": \"stdio\",");
    println!("        \"command\": \"npx\",");
    println!("        \"args\": [\"@modelcontextprotocol/server-filesystem\"]");
    println!("      }}");
    println!("    ]");
    println!("  }}");

    // Method 7: Environment variable patterns
    println!("\n7. Environment variable patterns:");
    println!("Supported patterns in configuration files:");
    println!("  ${{VAR_NAME}}      - Braced notation (recommended)");
    println!("  $VAR_NAME        - Simple notation");
    println!("  ${{DEFAULT_VALUE}}- With fallback (custom implementation needed)");
    println!("");
    println!("Example usage:");
    println!("  token = \"${{API_TOKEN}}\"");
    println!("  endpoint = \"https://${{API_HOST}}/api/mcp\"");
    println!("  header_value = \"Bearer ${{AUTH_TOKEN:-default-token}}\"");

    Ok(())
}

async fn example_authentication_patterns() {
    println!("\n=== Authentication Patterns Example ===\n");

    println!("Supported authentication types:");

    println!("\n1. Bearer Token:");
    println!("  auth = {{");
    println!("    type = \"bearer\",");
    println!("    token = \"${{API_TOKEN}}\"");
    println!("  }}");

    println!("\n2. Basic Authentication:");
    println!("  auth = {{");
    println!("    type = \"basic\",");
    println!("    username = \"${{USERNAME}}\",");
    println!("    password = \"${{PASSWORD}}\"");
    println!("  }}");

    println!("\n3. API Key:");
    println!("  auth = {{");
    println!("    type = \"api_key\",");
    println!("    api_key = \"${{API_KEY}}\",");
    println!("    api_key_header = \"X-API-Key\"  // Optional, defaults to \"X-API-Key\"");
    println!("  }}");

    println!("\n4. API Key in Query Parameter:");
    println!("  auth = {{");
    println!("    type = \"api_key\",");
    println!("    api_key = \"${{API_KEY}}\",");
    println!("    query_param = \"api_key\"  // Add as query parameter");
    println!("  }}");

    println!("\nEnvironment variable examples:");
    println!("  export API_TOKEN=your-bearer-token");
    println!("  export DB_USER=your-username");
    println!("  export DB_PASSWORD=your-password");
    println!("  export SERVICE_API_KEY=your-api-key");
}

#[tokio::main]
async fn main() -> AgentResult<()> {
    println!("Starting MCP Configuration Examples...\n");

    // Main configuration example
    if let Err(e) = load_and_use_mcp_config().await {
        println!("Error in configuration example: {}", e);
    }

    // Authentication patterns
    example_authentication_patterns().await;

    println!("\n=== MCP Configuration Examples Completed ===");
    println!("\nNext steps:");
    println!("1. Copy configuration files to your project");
    println!("2. Set appropriate environment variables");
    println!("3. Install MCP servers (filesystem, git, etc.)");
    println!("4. Test with your own model provider");

    Ok(())
}