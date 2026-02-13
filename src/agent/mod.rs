mod config;
mod definition;
mod handoff;
mod mcp_integration;
mod orchestrator;
mod registry;

use std::sync::Arc;

use async_trait::async_trait;

use crate::error::{AgentError, AgentResult};
use crate::model::{Message, ModelClient, ModelResponse};
use crate::protocol::{Event, Op};
use crate::session::{Session, SessionHandle, TurnContext};
use crate::tools::{ApprovalHook, Tool, ToolExecutor, ToolRegistry};

pub use config::AgentConfig;
pub use definition::AgentDefinition;
pub use handoff::HandoffManager;
pub use orchestrator::Orchestrator;
pub use registry::{AgentRegistry, HandoffReceiver, global_agent_registry};

pub struct Agent {
    config: AgentConfig,
    model: Arc<dyn ModelClient>,
    tool_executor: ToolExecutor,
    session_handle: SessionHandle,
}

pub struct AgentBuilder {
    config: AgentConfig,
    model: Option<Arc<dyn ModelClient>>,
    registry: ToolRegistry,
    approval: Option<Arc<dyn ApprovalHook>>,
    allowlist: Option<Vec<String>>,
    denylist: Vec<String>,
}

impl Default for AgentBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl AgentBuilder {
    pub fn new() -> Self {
        Self {
            config: AgentConfig::default(),
            model: None,
            registry: ToolRegistry::new(),
            approval: None,
            allowlist: None,
            denylist: Vec::new(),
        }
    }

    pub fn with_model(mut self, provider: impl ModelClient + 'static) -> Self {
        self.model = Some(Arc::new(provider));
        self
    }

    pub fn with_tool(mut self, tool: impl Tool + 'static) -> Self {
        self.registry.register(Arc::new(tool));
        self
    }

    pub fn with_approval_hook(mut self, hook: impl ApprovalHook + 'static) -> Self {
        self.approval = Some(Arc::new(hook));
        self
    }

    pub fn with_allowed_tools(mut self, tools: Vec<String>) -> Self {
        self.allowlist = Some(tools);
        self
    }

    pub fn with_denied_tools(mut self, tools: Vec<String>) -> Self {
        self.denylist = tools;
        self
    }

    pub fn with_instructions(mut self, instructions: impl Into<String>) -> Self {
        self.config.instructions = instructions.into();
        self
    }

    pub fn with_context(mut self, context: TurnContext) -> Self {
        self.config.context = context;
        self
    }

    pub fn build(self) -> AgentResult<Agent> {
        let model = self
            .model
            .ok_or_else(|| AgentError::InvalidConfig("model provider missing".to_string()))?;
        let (_session, handle) = Session::new(self.config.queue_buffer);
        // _session 现在是 Arc<Session>,但我们暂时不需要它

        let mut executor = ToolExecutor::new(self.registry);
        if let Some(hook) = self.approval {
            executor = executor.with_approval_hook(hook);
        }
        if let Some(allowlist) = self.allowlist {
            executor = executor.with_allowlist(allowlist);
        }
        if !self.denylist.is_empty() {
            executor = executor.with_denylist(self.denylist);
        }

        Ok(Agent {
            config: self.config,
            model,
            tool_executor: executor,
            session_handle: handle,
        })
    }
}

impl Agent {
    pub fn new(config: AgentConfig, model: Arc<dyn ModelClient>, tools: ToolExecutor) -> Self {
        let (_session, handle) = Session::new(config.queue_buffer);
        // _session 现在是 Arc<Session>,但我们暂时不需要它
        Self {
            config,
            model,
            tool_executor: tools,
            session_handle: handle,
        }
    }

    pub fn tool_executor(&self) -> &ToolExecutor {
        &self.tool_executor
    }

    pub async fn start(&self) -> AgentResult<SessionHandle> {
        Ok(self.session_handle.clone())
    }

    pub async fn submit(&self, op: Op) -> AgentResult<()> {
        self.session_handle.submit(op).await
    }

    pub async fn next_event(&self) -> Option<Event> {
        self.session_handle.next_event().await
    }

    pub async fn run(&self, prompt: &str) -> AgentResult<String> {
        let messages = vec![
            Message::system(self.config.instructions.clone()),
            Message::user(prompt.to_string()),
        ];

        let response: ModelResponse = self.model.chat(messages, Vec::new()).await?;
        Ok(response.content)
    }
}

#[async_trait]
pub trait AgentRunner: Send + Sync {
    async fn run(&self, prompt: &str) -> AgentResult<String>;
}

#[async_trait]
impl AgentRunner for Agent {
    async fn run(&self, prompt: &str) -> AgentResult<String> {
        self.run(prompt).await
    }
}
