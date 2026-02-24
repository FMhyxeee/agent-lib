//! Test GLM-5 with Coding Plan endpoint
//!
//! This tests GLM-5 model availability via Coding Plan API
//!
//! # Prerequisites
//! - GLM Coding Plan subscription (Max or Pro tier)
//! - Create `.env` file in project root with `GLM_API_KEY=your_key`
//! - Or set `GLM_API_KEY` environment variable manually
//!
//! # Run
//! ```bash
//! set GLM_API_KEY=your_key
//! cargo run --example test_glm5_coding
//! ```

use agent_lib::model::provider::{GlmCodingPlanProvider, GlmProvider};
use agent_lib::model::{Message, MessageRole, ModelClient};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Load .env file if it exists
    dotenv::dotenv().ok();

    // Load API key from environment
    let api_key = std::env::var("GLM_API_KEY");
    let api_key = match api_key {
        Ok(key) => key,
        Err(_) => {
            println!("⚠️  GLM_API_KEY not set");
            println!("   To test GLM-5 Coding Plan, set:");
            println!("   set GLM_API_KEY=your_api_key");
            println!("\n💡 Note: GLM-5 requires Coding Plan Max or Pro subscription");
            println!("   Lite tier will support GLM-5 in the future");
            return Ok(());
        }
    };

    println!("🚀 Testing GLM-5 with Standard vs Coding Plan endpoints\n");

    // Test 1: GLM-5 with standard API
    println!("📝 Test 1: GLM-5 with Standard API");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n");

    let standard_provider = GlmProvider::new("glm-5", &api_key);

    let messages = vec![Message {
        role: MessageRole::User,
        content: "你好,请用一句话介绍Rust语言。".to_string(),
        tool_call_id: None,
        tool_calls: None,
        reasoning_content: None,
    }];

    match standard_provider.chat(messages.clone(), vec![]).await {
        Ok(response) => {
            println!("✅ Standard API Response:");
            println!("{}\n", response.content);
            println!("📊 Token Usage:");
            println!("  - Prompt tokens: {}", response.usage.prompt_tokens);
            println!(
                "  - Completion tokens: {}",
                response.usage.completion_tokens
            );
            println!("  - Total tokens: {}", response.usage.total_tokens);
        }
        Err(e) => {
            eprintln!("❌ Standard API Error: {e}");
        }
    }

    println!("\n📝 Test 2: GLM-5 with Coding Plan API");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n");

    // Test 2: GLM-5 with Coding Plan endpoint
    let coding_provider = GlmCodingPlanProvider::new("glm-5", &api_key);

    match coding_provider.chat(messages, vec![]).await {
        Ok(response) => {
            println!("✅ Coding Plan API Response:");
            println!("{}\n", response.content);
            println!("📊 Token Usage:");
            println!("  - Prompt tokens: {}", response.usage.prompt_tokens);
            println!(
                "  - Completion tokens: {}",
                response.usage.completion_tokens
            );
            println!("  - Total tokens: {}", response.usage.total_tokens);
        }
        Err(e) => {
            eprintln!("❌ Coding Plan API Error: {e}");
            println!("\n💡 Possible reasons:");
            println!("   1. GLM-5 requires Coding Plan Max or Pro tier");
            println!("   2. Your subscription does not support GLM-5 yet");
            println!("   3. API key is invalid or expired");
        }
    }

    println!("\n✨ Summary:");
    println!("   📍 Standard API:  https://open.bigmodel.cn/api/paas/v4/");
    println!("   📍 Coding Plan API: https://open.bigmodel.cn/api/coding/paas/v4/");
    println!("\n💡 GLM-5 Features:");
    println!("   - Latest flagship model (SOTA)");
    println!("   - Optimized for Agentic Engineering");
    println!("   - 128K context window");
    println!("   - Supports both endpoints");

    Ok(())
}
