pub mod agent;
pub mod error;
pub mod mcp;
pub mod model;
pub mod protocol;
pub mod session;
pub mod tools;
pub mod trace;

pub use agent::{Agent, AgentBuilder, AgentConfig, Orchestrator};
pub use error::{AgentError, AgentResult};
pub use protocol::{Event, Op};
pub use session::{Session, SessionHandle, TurnContext};

// Re-export MCP types for convenience
pub use mcp::{
    McpClient,
    McpManager,
    McpRequest,
    McpResponse,
    McpTool,
    McpToolCall,
    McpTransport,
    TransportConfig,
};
