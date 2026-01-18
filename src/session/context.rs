use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TurnContext {
    pub model: String,
    pub sandbox: Option<String>,
    pub cwd: Option<String>,
    pub approval_policy: Option<String>,
}

impl Default for TurnContext {
    fn default() -> Self {
        Self {
            model: "default".to_string(),
            sandbox: None,
            cwd: None,
            approval_policy: None,
        }
    }
}
