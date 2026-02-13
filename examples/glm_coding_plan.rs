//! GLM Coding Plan Provider Example
//!
//! This example demonstrates how to use the GLM Coding Plan provider
//! which requires a separate subscription from the standard GLM API.
//!
//! # Prerequisites
//! - GLM Coding Plan subscription from https://www.bigmodel.cn/glm-coding
//! - Create `.env` file in project root with `GLM_API_KEY=your_key`
//! - Or set the `GLM_API_KEY` environment variable manually
//!
//! # Run
//! ```bash
//! cargo run --example glm_coding_plan
//! ```

use agent_lib::model::provider::GlmCodingPlanProvider;
use agent_lib::model::{Message, MessageRole, ModelClient};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Load .env file if it exists
    dotenv::dotenv().ok();

    // Load API key from environment
    let api_key = std::env::var("GLM_API_KEY").expect("GLM_API_KEY environment variable not set");

    println!("🚀 GLM Coding Plan Provider Example\n");

    // Example 1: Basic chat with glm-4.7
    println!("📝 Example 1: Basic Chat");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n");

    let provider = GlmCodingPlanProvider::new("glm-4.7", &api_key);

    let messages = vec![Message {
        role: MessageRole::User,
        content: "你好!请用一句话介绍Rust编程语言的特点。".to_string(),
        tool_call_id: None,
        tool_calls: None,
    }];

    match provider.chat(messages, vec![]).await {
        Ok(response) => {
            println!("✅ Response:");
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
            eprintln!("❌ Error: {e}");
        }
    }

    println!("\n📝 Example 2: Streaming Chat");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n");

    let provider = GlmCodingPlanProvider::new("glm-4.7", &api_key);

    let messages = vec![Message {
        role: MessageRole::User,
        content: "请写一首关于AI编程的简短诗歌,不超过50字。".to_string(),
        tool_call_id: None,
        tool_calls: None,
    }];

    match provider.chat_stream(messages, vec![]).await {
        Ok(mut stream) => {
            println!("✅ Streaming response:");
            print!("  ");
            use futures::StreamExt;
            while let Some(chunk) = stream.next().await {
                print!("{}", chunk.delta);
                tokio::io::AsyncWriteExt::flush(&mut tokio::io::stdout()).await?;
            }
            println!("\n");
        }
        Err(e) => {
            eprintln!("❌ Error: {e}");
        }
    }

    println!("\n📝 Example 3: Custom Endpoint");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n");

    // You can also use a custom endpoint if needed
    let _custom_provider = GlmCodingPlanProvider::new("glm-4.7", &api_key)
        .with_base_url("https://open.bigmodel.cn/api/coding/paas/v4/chat/completions");

    println!("✅ Custom endpoint provider created");
    println!("   Default endpoint: https://open.bigmodel.cn/api/coding/paas/v4/chat/completions");

    println!("\n✨ All examples completed!");
    println!("\n💡 Note: GLM Coding Plan provides:");
    println!("   - Coding-optimized model access");
    println!("   - Higher usage limits (Lite/Pro/Max tiers)");
    println!("   - Dedicated endpoint: https://open.bigmodel.cn/api/coding/paas/v4/");
    println!("   - Compatible with Claude Code, Cline, and other coding tools");

    Ok(())
}
