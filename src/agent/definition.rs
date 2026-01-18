use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentDefinition {
    pub name: String,
    pub instructions: String,
    pub tools: Vec<String>,
    pub model: String,
    pub handoff_targets: Vec<String>,
}
