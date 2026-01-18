use std::collections::HashMap;

use crate::agent::{AgentDefinition, HandoffManager};
use crate::error::{AgentError, AgentResult};

pub struct Orchestrator {
    agents: HashMap<String, AgentDefinition>,
    current_agent: String,
    handoff: HandoffManager,
}

impl Orchestrator {
    pub fn new(agents: Vec<AgentDefinition>, initial_agent: &str) -> AgentResult<Self> {
        let map: HashMap<String, AgentDefinition> = agents
            .into_iter()
            .map(|agent| (agent.name.clone(), agent))
            .collect();

        if !map.contains_key(initial_agent) {
            return Err(AgentError::InvalidConfig(format!(
                "initial agent not found: {initial_agent}"
            )));
        }

        Ok(Self {
            handoff: HandoffManager::new(map.clone()),
            agents: map,
            current_agent: initial_agent.to_string(),
        })
    }

    pub fn current_agent(&self) -> &AgentDefinition {
        self.agents
            .get(&self.current_agent)
            .expect("current agent not found")
    }

    pub fn handoff_to(&mut self, target: &str) -> AgentResult<()> {
        if self.handoff.can_handoff(&self.current_agent, target)? {
            self.current_agent = target.to_string();
            Ok(())
        } else {
            Err(AgentError::InvalidConfig(format!(
                "handoff not allowed: {} -> {}",
                self.current_agent, target
            )))
        }
    }
}
