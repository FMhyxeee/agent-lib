use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub enum SessionState {
    #[default]
    Idle,
    Running,
    AwaitingApproval,
    Completed,
    Failed,
}
