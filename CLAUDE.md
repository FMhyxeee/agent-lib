# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Development Commands

### Building
```bash
# Build the library (default features: openai, builtin-tools)
cargo build

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

# Run tests with verbose output
cargo test --verbose

# Run tests without actually executing them (compile only)
cargo test --no-run
```

### Running Examples
```bash
# Run tool call example
cargo run --example tool_call

# Run simple chat example
cargo run --example simple_chat

# Run multi-agent example
cargo run --example multi_agent

# Run MCP client example
cargo run --example mcp_client
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
   - `ConversationHistory` - Thread-safe message storage
   - Event-driven session loop processing operations

5. **Protocol System** (`src/protocol/`)
   - **SQ (Submission Queue)** - Commands TO the system (`Op` enum)
   - **EQ (Event Queue)** - Events FROM the system (`Event` enum)
   - Async message passing between components

6. **MCP Integration** (`src/mcp/`)
   - Model Context Protocol support
   - Multiple transports: stdio, tcp, http, websocket

### Key Communication Flow

1. **User submits prompt** → `Op::StartTurn` via SQ
2. **Agent processes turn** → Queries model with conversation history
3. **Model may call tools** → `ToolCallRequested` event via EQ
4. **Tool executor** → Runs tools with approval checks
5. **Results flow back** → `ModelComplete` event via EQ
6. **Turn completes** → Response returned to user

### Message Types
- `MessageRole::{System, User, Assistant, Tool}`
- `Event::{ModelStreaming, ToolCallResult, ApprovalRequired, TurnComplete, Error}`
- `Op::{StartTurn, UserInput, ApprovalResponse, Handoff}`

### Safety Features
- **Tool approval hooks** for manual intervention
- **Allow/deny lists** for tool access control
- **Sandbox roots** for filesystem tools
- **Structured error handling** with `AgentResult<T>`

### Configuration
- Features: `openai`, `anthropic`, `local-llm`, `builtin-tools`, `mcp`
- Environment variables for API keys (see README.md for details)

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

### Event Streaming
```rust
let mut event_stream = session_handle.event_stream();
while let Some(event) = event_stream.recv().await {
    match event {
        Event::ModelStreaming { chunk } => handle_stream(chunk),
        Event::ToolCallResult { result } => handle_tool_result(result),
        // ... other events
    }
}
```

### Error Handling
- Centralized `AgentError` type
- `AgentResult<T>` for fallible operations
- Structured error information for debugging

This architecture enables building sophisticated AI agents with tool use, multi-agent coordination, and robust safety mechanisms while maintaining clean abstractions and extensibility.