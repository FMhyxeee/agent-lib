use serde::{Deserialize, Serialize};

use crate::agent::AgentRole;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentDefinition {
    pub name: String,
    pub instructions: String,
    pub tools: Vec<String>,
    pub model: String,
    pub role: AgentRole,
    #[serde(default)]
    pub parallel_targets: Vec<String>,
}
