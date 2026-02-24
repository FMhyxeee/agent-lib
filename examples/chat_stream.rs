//! Minimal `chat_stream` example using OpenAI-compatible streaming.
//!
//! # Prerequisites
//! - Set `OPENAI_API_KEY` in env or in a local `.env` file
//! - Optional: set `OPENAI_MODEL` (default: `gpt-4o-mini`)
//!
//! # Run
//! ```bash
//! cargo run --example chat_stream
//! ```

use agent_lib::model::provider::GlmCodingPlanProvider;
use agent_lib::model::{Message, ModelClient};
use futures::StreamExt;
use tokio::io::AsyncWriteExt;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    dotenv::dotenv().ok();

    let provider = GlmCodingPlanProvider::new("GLM-5", "12e3d3b373a54f1981aa698a4bfeeed0.mDxonoYiOTigaCsY");

    let messages = vec![
        Message::system("You are a concise assistant."),
        Message::user("Use 3 short bullets to explain what Rust ownership solves."),
    ];

    let mut stream = provider.chat_stream(messages, vec![]).await?;

    println!("Streaming response:\n");
    while let Some(chunk) = stream.next().await {
        print!("{}", chunk.delta);
        tokio::io::stdout().flush().await?;
    }
    println!();

    Ok(())
}
