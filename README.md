# agent-lib

Core, non-UI agent library in Rust. Provides:

- SQ/EQ protocol with `Op`/`Event`
- Sessions and turn context
- Model client abstraction with OpenAI provider
- Tools registry/executor with approval hooks and allow/deny lists
- MCP client with stdio/tcp/http/ws transports
- Basic orchestration primitives

## Quick start

```rust
use agent_lib::model::provider::OpenAiProvider;
use agent_lib::{AgentBuilder, AgentResult};

#[tokio::main]
async fn main() -> AgentResult<()> {
    let agent = AgentBuilder::new()
        .with_model(OpenAiProvider::new("gpt-4o-mini").with_api_key("YOUR_KEY"))
        .with_instructions("You are a helpful assistant.")
        .build()?;

    let response = agent.run("Hello").await?;
    println!("{response}");
    Ok(())
}
```

## Tools safety

Use allow/deny lists and the approval hook to guard tool execution:

```rust
use agent_lib::tools::{ApprovalDecision, ApprovalHook};

agent = agent
    .with_allowed_tools(vec!["filesystem".to_string(), "network".to_string()])
    .with_denied_tools(vec!["shell".to_string()]);
```

`FileSystemTool` also honors `ToolContext.sandbox_root` to prevent path escapes.

## Tool call example

Create `dump.txt`, write `hello`, then delete it using the filesystem tool:

```bash
cargo run --example tool_call
```

## MCP transport endpoints

- `stdio://path/to/binary --arg`
- `tcp://127.0.0.1:9000`
- `http://host/mcp`
- `ws://host/mcp` or `wss://host/mcp`

## Features

Enable OpenAI integration:

```
cargo check --features openai
```

## GLM (Zhipu) provider

```rust
use agent_lib::model::provider::GlmProvider;
use agent_lib::{AgentBuilder, AgentResult};

#[tokio::main]
async fn main() -> AgentResult<()> {
    let agent = AgentBuilder::new()
        .with_model(GlmProvider::new("GLM-4.7", "YOUR_API_KEY"))
        .build()?;
    let response = agent.run("你好").await?;
    println!("{response}");
    Ok(())
}
```

GLM integration can be exercised via the smoke test (uses env vars):

```
GLM_BASE_URL=https://open.bigmodel.cn/api/paas/v4/chat/completions
GLM_API_KEY=your_key_here
```
