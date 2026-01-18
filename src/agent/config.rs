use serde::{Deserialize, Serialize};

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
