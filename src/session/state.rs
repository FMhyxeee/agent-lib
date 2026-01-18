use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SessionState {
    Idle,
    Running,
    AwaitingApproval,
    Completed,
    Failed,
}

impl Default for SessionState {
    fn default() -> Self {
        SessionState::Idle
    }
}
