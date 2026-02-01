use serde::{Deserialize, Serialize};
use std::env;
use std::path::Path;

use crate::error::{AgentError, AgentResult};
use crate::session::TurnContext;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentConfig {
    pub name: String,
    pub instructions: String,
    pub context: TurnContext,
    pub queue_buffer: usize,
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            name: "default".to_string(),
            instructions: "You are a helpful assistant.".to_string(),
            context: TurnContext::default(),
            queue_buffer: 64,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
struct AgentConfigPartial {
    pub name: Option<String>,
    pub instructions: Option<String>,
    pub context: Option<TurnContext>,
    pub queue_buffer: Option<usize>,
}

impl AgentConfig {
    pub async fn from_toml_file(path: impl AsRef<Path>) -> AgentResult<Self> {
        let path = path.as_ref();
        let content = tokio::fs::read_to_string(path).await.map_err(|e| {
            AgentError::InvalidConfig(format!(
                "failed to read agent config file '{}': {}",
                path.display(),
                e
            ))
        })?;
        let partial: AgentConfigPartial = toml::from_str(&content).map_err(|e| {
            AgentError::InvalidConfig(format!(
                "failed to parse agent TOML config '{}': {}",
                path.display(),
                e
            ))
        })?;
        Ok(Self::apply_partial(Self::default(), partial))
    }

    pub async fn from_json_file(path: impl AsRef<Path>) -> AgentResult<Self> {
        let path = path.as_ref();
        let content = tokio::fs::read_to_string(path).await.map_err(|e| {
            AgentError::InvalidConfig(format!(
                "failed to read agent config file '{}': {}",
                path.display(),
                e
            ))
        })?;
        let partial: AgentConfigPartial = serde_json::from_str(&content).map_err(|e| {
            AgentError::InvalidConfig(format!(
                "failed to parse agent JSON config '{}': {}",
                path.display(),
                e
            ))
        })?;
        Ok(Self::apply_partial(Self::default(), partial))
    }

    pub fn from_env() -> AgentResult<Self> {
        if let Ok(json) = env::var("AGENT_CONFIG_JSON") {
            let partial: AgentConfigPartial = serde_json::from_str(&json).map_err(|e| {
                AgentError::InvalidConfig(format!("failed to parse AGENT_CONFIG_JSON: {e}"))
            })?;
            return Ok(Self::apply_partial(Self::default(), partial));
        }

        let mut config = Self::default();
        if let Ok(name) = env::var("AGENT_NAME") {
            if !name.trim().is_empty() {
                config.name = name;
            }
        }
        if let Ok(instructions) = env::var("AGENT_INSTRUCTIONS") {
            if !instructions.trim().is_empty() {
                config.instructions = instructions;
            }
        }
        if let Ok(queue) = env::var("AGENT_QUEUE_BUFFER") {
            if let Ok(value) = queue.parse::<usize>() {
                config.queue_buffer = value;
            }
        }
        if let Ok(context_json) = env::var("AGENT_CONTEXT_JSON") {
            let ctx: TurnContext = serde_json::from_str(&context_json).map_err(|e| {
                AgentError::InvalidConfig(format!("failed to parse AGENT_CONTEXT_JSON: {e}"))
            })?;
            config.context = ctx;
        }
        Ok(config)
    }

    fn apply_partial(mut base: Self, partial: AgentConfigPartial) -> Self {
        if let Some(name) = partial.name {
            base.name = name;
        }
        if let Some(instructions) = partial.instructions {
            base.instructions = instructions;
        }
        if let Some(context) = partial.context {
            base.context = context;
        }
        if let Some(queue_buffer) = partial.queue_buffer {
            base.queue_buffer = queue_buffer;
        }
        base
    }
}
