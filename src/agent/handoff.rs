use std::collections::HashMap;

use crate::agent::AgentDefinition;
use crate::error::{AgentError, AgentResult};

#[derive(Default)]
pub struct HandoffManager {
    agents: HashMap<String, AgentDefinition>,
}

impl HandoffManager {
    pub fn new(agents: HashMap<String, AgentDefinition>) -> Self {
        Self { agents }
    }

    pub fn can_handoff(&self, from: &str, to: &str) -> AgentResult<bool> {
        let agent = self
            .agents
            .get(from)
            .ok_or_else(|| AgentError::InvalidConfig(format!("agent not found: {from}")))?;
        Ok(agent.handoff_targets.contains(&to.to_string()))
    }
}
