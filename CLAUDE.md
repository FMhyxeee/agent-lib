# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Common
Please call me '叉叉' when every time you talk to me.

## Development Commands

### Building
```bash
# Build the library (default features: openai, builtin-tools)
cargo build

# Build with Codex compatibility (enables tiktoken for precise token counting)
cargo build --features codex-compat

# Build without default features
cargo build --no-default-features

# Build with specific features
cargo build --features openai,mcp
```

### Testing
```bash
# Run all tests
cargo test

# Run specific test file
cargo test session_tests
cargo test codex_compat_tests

# Run tests with verbose output
cargo test --verbose

# Run tests without actually executing them (compile only)
cargo test --no-run
```

### Running Examples
```bash
# Basic agent examples
cargo run --example tool_call          # Tool usage demonstration
cargo run --example simple_chat        # Simple chat agent
cargo run --example tool_usage         # Tool usage patterns
cargo run --example multi_agent        # Multi-agent coordination

# MCP (Model Context Protocol) examples
cargo run --example mcp_client         # Basic MCP client
cargo run --example mcp_demo           # MCP demo with basic operations
cargo run --example mcp_demo_final     # Advanced MCP demo
cargo run --example agent_with_mcp     # Agent with MCP integration
cargo run --example agent_with_mcp_manager  # Agent with MCP manager
cargo run --example simple_mcp_test   # Simple MCP testing

# Configuration examples
cargo run --example config_loader_demo  # Configuration loading demo
cargo run --example mcp_config_example  # MCP configuration example
```

### Linting & Format Check
```bash
# Check formatting
cargo fmt --check

# Format code
cargo fmt

# Clippy lints
cargo clippy
```

## Architecture Overview

### Core Design Pattern
This library uses an **event-driven architecture** based on SQ/EQ (Submission/Event Queue) protocol for asynchronous communication between components.

### Main Components

1. **Agent System** (`src/agent/`)
   - `Agent` - Main agent interface combining model, tools, and session
   - `AgentBuilder` - Fluent API for constructing agents with model, tools, and policies
   - `Orchestrator` - Multi-agent coordination with handoff capabilities

2. **Model Abstraction** (`src/model/`)
   - `ModelClient` trait for LLM provider abstraction
   - Built-in providers: `OpenAiProvider`, `GlmProvider`
   - Streaming support via `chat_stream()`

3. **Tool System** (`src/tools/`)
   - `Tool` trait for pluggable capabilities
   - `ToolRegistry` for tool discovery
   - `ToolExecutor` with approval hooks and allow/deny lists
   - Built-in tools: `FileSystemTool` with sandbox support

4. **Session Management** (`src/session/`)
   - `Session` - Conversation state and message history
   - `ConversationHistory` - Thread-safe message storage with token counting and compaction
   - `TurnContext` - Enhanced context with approval policies, sandbox policies, reasoning effort
   - Event-driven session loop processing operations

5. **Protocol System** (`src/protocol/`)
   - **SQ (Submission Queue)** - Commands TO the system (`Op` enum)
   - **EQ (Event Queue)** - Events FROM the system (`Event` enum)
   - Codex-compatible Op/Event types for full OpenAI Codex compatibility

6. **Task System** (`src/tasks/`)
   - `SessionTask` trait for defining async tasks
   - `submission_loop` - Codex-compatible core event loop
   - Built-in tasks: `RegularTask`, `CompactTask`
   - Task cancellation and lifecycle management

7. **Token Management** (`src/token/`)
   - `TokenCounter` - Approximate or precise (tiktoken) token counting
   - `TruncationPolicy` - Token/byte-based context truncation
   - Automatic history compaction when exceeding limits

8. **MCP Integration** (`src/mcp/`)
   - Model Context Protocol support with full async implementation
   - Multiple transports: stdio, tcp, http, websocket
   - Configuration management via JSON/TOML
   - MCP manager for handling multiple MCP servers
   - Transport backup and fallback mechanisms

### Key Communication Flow

1. **User submits prompt** → `Op::UserTurn` via SQ
2. **submission_loop processes** → Routes to appropriate handler
3. **Task spawned** → `SessionTask::run()` executes asynchronously
4. **Model queries** → With compacted history if needed
5. **Events flow back** → Via EQ to listeners
6. **Turn completes** → Response or compacted history

### Message Types
- `MessageRole::{System, User, Assistant, Tool}`
- `Event::{ModelStreaming, ToolCallResult, ApprovalRequired, TurnComplete, ContextCompacted, Error}`
- `Op::{UserTurn, Interrupt, Compact, Undo, OverrideTurnContext, Shutdown}` (20+ variants)

### Safety Features
- **Tool approval hooks** for manual intervention
- **Allow/deny lists** for tool access control
- **Sandbox roots** for filesystem tools
- **ApprovalPolicy** - AlwaysAsk, ReadOnlySafe, NeverAsk
- **SandboxPolicy** - Readonly, Persistent, InMemory
- **Structured error handling** with `AgentResult<T>`

### Configuration
- Features: `openai`, `anthropic`, `local-llm`, `builtin-tools`, `mcp`, `codex-compat`
- Environment variables:
  - `OPENAI_API_KEY` - OpenAI API key (required for OpenAI feature)
  - `GLM_BASE_URL` - GLM provider base URL (default: https://open.bigmodel.cn/api/paas/v4/chat/completions)
  - `GLM_API_KEY` - GLM API key (required for GLM provider)
  - `ANTHROPIC_API_KEY` - Anthropic API key (required for Anthropic feature)
- MCP configuration via JSON/TOML files or environment variables

## Codex Compatibility Features

### Enhanced Op Enum
The `Op` enum now supports 20+ operation types compatible with OpenAI Codex:
- `UserTurn` - Full user input with context and policies
- `OverrideTurnContext` - Dynamic context modification
- `Compact` - Manual history compaction
- `Undo` / `ThreadRollback` - State rollback
- `ExecApproval` / `PatchApproval` - Tool approval responses
- `ListMcpTools` / `RefreshMcpServers` - MCP management
- `Review` - Code review requests
- `Shutdown` - Graceful shutdown

### Enhanced Event Enum
New events for comprehensive state tracking:
- `SessionConfigured` - Session initialization
- `TurnAborted` - Turn termination reasons
- `ContextCompacted` - History compaction notifications
- `Warning` - Non-fatal warnings
- `McpListToolsResponse` / `ListSkillsResponse` - Query responses

### TurnContext Enhancements
```rust
pub struct TurnContext {
    // Basic
    pub model: String,
    pub cwd: Option<String>,
    pub sub_id: String,

    // Policies
    pub approval_policy_v2: Option<ApprovalPolicy>,
    pub sandbox_policy_v2: Option<SandboxPolicy>,

    // Reasoning
    pub reasoning_effort: Option<ReasoningEffort>, // Low, Medium, High
    pub reasoning_summary: Option<ReasoningSummary>,

    // Output control
    pub final_output_json_schema: Option<Value>,

    // Token management
    pub truncation_policy: Option<TruncationPolicy>,
    pub auto_compact_token_limit: Option<i64>,
    pub context_window: usize,
}
```

### Token Management
```rust
// Approximate counting (fast, ~4 bytes/token)
let count = approx_token_count("hello world");

// Precise counting (requires codex-compat feature)
let count = tiktoken_count("hello world");

// Truncation policies
let policy = TruncationPolicy::tokens(100000);
let budget = policy.token_budget();
```

### Task System
```rust
use agent_lib::tasks::{SessionTask, CompactTask, submission_loop};

// Define custom task
struct MyTask;
#[async_trait]
impl SessionTask for MyTask {
    fn kind(&self) -> TaskKind { TaskKind::Regular }

    async fn run(self: Arc<Self>, session: Arc<dyn TaskSession>, ...) -> Option<String> {
        // Task implementation
    }
}

// Spawn task
session.spawn_task(ctx, MyTask).await;
```

### History Compaction
ConversationHistory supports automatic token-aware compaction:
```rust
// Get total tokens
let total = history.total_tokens();

// Compact history, keeping recent messages
history.compact(keep_recent: 10, summary: "Previous conversation...".to_string());

// Get messages for model prompt (includes summaries)
let messages = history.for_prompt();
```

## Important Patterns

### Async-First Design
- All components built on `tokio`
- `Send + Sync` traits for thread safety
- Async traits via `async-trait`

### Builder Pattern
```rust
AgentBuilder::new()
    .with_model(model)
    .with_instructions(system_prompt)
    .with_allowed_tools(allowlist)
    .with_approval_hook(hook)
    .build()
```

### TurnContext Builder
```rust
TurnContext::new("gpt-4")
    .with_cwd("/home/user")
    .with_approval_policy(ApprovalPolicy::NeverAsk)
    .with_sandbox_policy(SandboxPolicy::Readonly)
    .with_reasoning_effort(ReasoningEffort::High)
    .with_context_window(200000)
```

### Event Streaming
```rust
let mut event_stream = session_handle.event_stream();
while let Some(event) = event_stream.recv().await {
    match event {
        Event::ModelStreaming { chunk } => handle_stream(chunk),
        Event::ToolCallResult { result } => handle_tool_result(result),
        Event::ContextCompacted { compacted_items } => handle_compaction(compacted_items),
        // ... other events
    }
}
```

### Error Handling
- Centralized `AgentError` type
- `AgentResult<T>` for fallible operations
- Structured error information for debugging

This architecture enables building sophisticated AI agents with tool use, multi-agent coordination, token-aware history management, and robust safety mechanisms while maintaining clean abstractions and extensibility.

## MCP Configuration

### Configuration Files
MCP can be configured via:
- JSON files (`mcp_config.json`)
- TOML files (`mcp_config.toml`)
- Environment variables
- Claude Desktop configuration files (`claude_desktop_config.json`)

### Example MCP Configuration
```json
{
  "servers": {
    "filesystem": {
      "command": "npx",
      "args": ["-y", "@modelcontextprotocol/server-filesystem", "/path/to/root"]
    },
    "brave-search": {
      "command": "npx",
      "args": ["-y", "@modelcontextprotocol/server-brave-search"],
      "env": {
        "BRAVE_API_KEY": "your_key_here"
      }
    }
  }
}
```

### Transport Endpoints
- `stdio://path/to/binary --args` - Standard I/O
- `tcp://127.0.0.1:9000` - TCP socket
- `http://localhost:8080/mcp` - HTTP endpoint
- `ws://localhost:8080/mcp` - WebSocket endpoint
- `wss://localhost:8080/mcp` - Secure WebSocket endpoint

### MCP Manager Features
- Handle multiple MCP servers simultaneously
- Transport health monitoring and automatic failover
- Dynamic server registration/deregistration
- Load balancing across multiple instances