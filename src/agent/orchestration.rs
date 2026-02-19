use std::collections::HashMap;
use std::time::Duration;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AgentRole {
    Planner,
    Worker,
    Reviewer,
}

#[derive(Debug, Clone)]
pub struct OrchestratorOptions {
    pub branch_timeout: Duration,
}

impl Default for OrchestratorOptions {
    fn default() -> Self {
        Self {
            branch_timeout: Duration::from_secs(30),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrchestrationRequest {
    pub goal: String,
    pub run_id: Option<String>,
}

impl OrchestrationRequest {
    pub fn new(goal: impl Into<String>) -> Self {
        Self {
            goal: goal.into(),
            run_id: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum StepStatus {
    Success,
    Failed,
    Timeout,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrchestrationArtifact {
    pub id: String,
    pub step: String,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrchestrationHistoryEntry {
    pub step: String,
    pub status: StepStatus,
    pub summary: String,
    pub artifact_id: Option<String>,
    pub timestamp: DateTime<Utc>,
    pub duration_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BranchResult {
    pub worker: String,
    pub status: StepStatus,
    pub summary: String,
    pub artifact_id: Option<String>,
    pub error: Option<String>,
    pub duration_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrchestrationTimings {
    pub total_ms: u64,
    pub planner_ms: u64,
    pub reviewer_ms: u64,
    pub branch_ms: HashMap<String, u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrchestrationResult {
    pub run_id: String,
    pub goal: String,
    pub final_output: String,
    pub planner_output: String,
    pub branch_results: Vec<BranchResult>,
    pub blackboard: Value,
    pub history: Vec<OrchestrationHistoryEntry>,
    pub artifacts: Vec<OrchestrationArtifact>,
    pub timings: OrchestrationTimings,
}
