# agent-lib

Core, non-UI agent library in Rust. Provides:

- **OpenAI Codex Compatible** - Complete SQ/EQ protocol with 20+ `Op`/`Event` types
- **Session Management** - Conversation history with token-aware compaction and undo/rollback
- **Enhanced TurnContext** - Dynamic configuration with approval/sandbox policies, reasoning effort
- **Model Client Abstraction** - OpenAI, GLM, and extensible provider support
- **Tools Registry** - Approval hooks, allow/deny lists, sandboxed filesystem tool
- **MCP Integration** - Multi-server manager with stdio/tcp/http/ws transports
- **SessionBuilder Pattern** - Fluent API for configuration
- **Advanced Features** - Token counting, history compression, task cancellation

## Quick Start

### Basic Agent Example

```rust
use agent_lib::model::provider::GlmProvider;
use agent_lib::{AgentBuilder, AgentResult};

#[tokio::main]
async fn main() -> AgentResult<()> {
    // 使用 GLM 模型
    let agent = AgentBuilder::new()
        .with_model(GlmProvider::new("GLM-4-Flash", "YOUR_API_KEY"))
        .with_instructions("你是一个友好的助手。")
        .build()?;

    let response = agent.run("什么是 Rust？").await?;
    println!("{response}");
    Ok(())
}
```

### Session with Event Streaming

For more control, use `Session` directly with Op/Event protocol:

```rust
use agent_lib::model::provider::GlmProvider;
use agent_lib::protocol::Op;
use agent_lib::session::{Session, SessionConfig};
use agent_lib::model::Message;

#[tokio::main]
async fn main() -> agent_lib::AgentResult<()> {
    // 创建带模型支持的 Session
    let config = SessionConfig {
        model: Some(std::sync::Arc::new(GlmProvider::new(
            "GLM-4-Flash",
            std::env::var("GLM_API_KEY")?
        ))),
        ..Default::default()
    };

    let (_session, handle) = Session::with_config(64, config);

    // 添加用户消息
    handle.submit(Op::AddToHistory {
        message: Message::user("你好"),
        turn_id: "turn-1".to_string(),
    }).await?;

    // 监听事件流
    while let Some(event) = handle.next_event().await {
        match event {
            agent_lib::protocol::Event::ModelStreaming { chunk } => print!("{chunk}"),
            agent_lib::protocol::Event::TurnComplete => break,
            _ => {}
        }
    }

    Ok(())
}
```

### SessionBuilder Pattern

For advanced configuration:

```rust
use agent_lib::session::SessionBuilder;
use agent_lib::mcp::McpManager;

let (session, handle) = SessionBuilder::new()
    .with_default_model("gpt-4")
    .with_mcp_manager(mcp_manager)
    .with_max_undo_steps(20)
    .build();
```

## Safety & Security

### Tool Approval System

Use allow/deny lists and approval hooks to guard tool execution:

```rust
use agent_lib::tools::{ApprovalDecision, ApprovalHook};

agent = agent
    .with_allowed_tools(vec!["filesystem".to_string(), "network".to_string()])
    .with_denied_tools(vec!["shell".to_string()]);
```

### Approval Policies

Choose how tools are approved:

- `AlwaysAsk` - Require approval for all operations
- `ReadOnlySafe` - Only ask for non-safe operations
- `NeverAsk` - Auto-approve all operations

### Sandbox Policies

Control filesystem access:

- `Readonly` - Read-only access
- `Persistent` - Changes saved to disk
- `InMemory` - Changes only in memory

`FileSystemTool` also honors `ToolContext.sandbox_root` to prevent path escapes.

## Tool call example

Create `dump.txt`, write `hello`, then delete it using the filesystem tool:

```bash
cargo run --example tool_call
```

## MCP Integration

### Multi-Server Support

Handle multiple MCP servers simultaneously:

```rust
let mcp_manager = McpManager::new();
mcp_manager.register_server("filesystem", &config)?;
mcp_manager.register_server("brave-search", &config)?;
```

### Transport Endpoints

- `stdio://path/to/binary --arg`
- `tcp://127.0.0.1:9000`
- `http://host/mcp`
- `ws://host/mcp` or `wss://host/mcp`

### Automatic Failover

Transports support health monitoring and automatic failover between endpoints.

## Features

### Feature Flags

Enable specific integrations:

```
# OpenAI + GLM + Tools + MCP + Codex compatibility
cargo check --features openai,glm,builtin-tools,mcp,codex-compat

# Individual features
cargo check --features openai        # OpenAI provider
cargo check --features glm           # GLM provider
cargo check --features builtin-tools  # Built-in tools
cargo check --features mcp           # MCP integration
cargo check --features codex-compat  # Tiktoken for precise counting
```

### Advanced Capabilities

#### Token Management
- Approximate counting (fast, ~4 bytes/token)
- Precise counting with tiktoken
- Automatic history compaction when exceeding limits
- Configurable truncation policies

#### History & Context
- Message history with compaction
- Undo/rollback operations
- Context window size configuration
- Reasoning summaries and effort levels

#### Session Operations
- Dynamic turn context override
- Thread rollback by number of turns
- Manual history addition
- Real-time event streaming

## Model Providers

### GLM (智谱AI)

```rust
use agent_lib::model::provider::GlmProvider;
use agent_lib::{AgentBuilder, AgentResult};

#[tokio::main]
async fn main() -> AgentResult<()> {
    let agent = AgentBuilder::new()
        .with_model(GlmProvider::new("GLM-4-Flash", "YOUR_API_KEY"))
        .with_instructions("你是一个友好的助手。")
        .build()?;

    let response = agent.run("你好，介绍一下 Rust").await?;
    println!("{response}");
    Ok(())
}
```

**Available Models:** `GLM-4-Flash`, `GLM-4`, `GLM-4-Plus`, `GLM-4-Air`

### OpenAI

```rust
use agent_lib::model::provider::OpenAiProvider;

let agent = AgentBuilder::new()
    .with_model(OpenAiProvider::new("gpt-4o-mini").with_api_key("YOUR_KEY"))
    .build()?;
```

## Environment Variables

### Required for specific features:

```bash
# OpenAI provider
OPENAI_API_KEY=your_key_here

# GLM provider
GLM_BASE_URL=https://open.bigmodel.cn/api/paas/v4/chat/completions
GLM_API_KEY=your_key_here

# Anthropic provider
ANTHROPIC_API_KEY=your_key_here

# MCP configuration
MCPSERVER_FILE=./mcp_config.json
```

## Examples

Run examples to test features:

```bash
# Basic agent
cargo run --example simple_chat

# Tool usage (create/write/delete files)
cargo run --example tool_call

# GLM provider example
set GLM_API_KEY=your_key
cargo run --example simple_chat

# MCP integration
cargo run --example agent_with_mcp

# Mock MCP filesystem server
cargo run --example mock_filesystem_mcp

# RegularTask E2E test with GLM
set GLM_API_KEY=your_key
cargo run --example regular_task_glm_test

# Multi-agent coordination
cargo run --example multi_agent

# Configuration loading
cargo run --example config_loader_demo
```

### Example: Mock Filesystem MCP

Test MCP integration without a real server:

```bash
cargo run --example mock_filesystem_mcp
```

This creates an in-memory filesystem with:
- `read_file(path)` - Read file contents
- `write_file(path, content)` - Write to files
- `list_directory(path)` - List directory entries
- `delete_file(path)` - Delete files

### Example: RegularTask with GLM

End-to-end test of the RegularTask implementation:

```bash
set GLM_API_KEY=your_key
cargo run --example regular_task_glm_test
```

This demonstrates:
- Session with model integration
- Event streaming (ModelStreaming, ModelComplete)
- Conversation history management
- Token-aware compaction
- Turn context configuration

## Testing

Run all tests:

```bash
cargo test

# Test specific modules
cargo test session
cargo test mcp
cargo test tasks
```
