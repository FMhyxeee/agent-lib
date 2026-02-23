use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use serde_json::json;
use tokio::sync::Mutex;
use tokio::time::sleep;

use agent_lib::agent::{AgentDefinition, AgentRole, AgentRunner, HandoffReceiver};
use agent_lib::model::{Message, ModelClient, ModelResponse, StreamChunk, TokenUsage};
use agent_lib::{
    AgentBuilder, AgentError, AgentRegistry, AgentResult, GovernanceInjectionRequest,
    GovernanceInjectionSeverity, GovernedOrchestrator, ModelError, OrchestrationRequest,
    Orchestrator, OrchestratorOptions, StepStatus,
};

#[derive(Clone)]
struct MockModel;

#[async_trait]
impl ModelClient for MockModel {
    async fn chat(
        &self,
        _messages: Vec<Message>,
        _tools: Vec<agent_lib::tools::ToolDef>,
    ) -> AgentResult<ModelResponse> {
        Ok(ModelResponse {
            content: "mock-response".to_string(),
            usage: TokenUsage::default(),
            tool_calls: Vec::new(),
        })
    }

    async fn chat_stream(
        &self,
        _messages: Vec<Message>,
        _tools: Vec<agent_lib::tools::ToolDef>,
    ) -> AgentResult<std::pin::Pin<Box<dyn futures::Stream<Item = StreamChunk> + Send>>> {
        let stream = futures::stream::iter(vec![StreamChunk {
            delta: "mock".to_string(),
        }]);
        Ok(Box::pin(stream))
    }
}

#[derive(Clone, Copy)]
enum Scenario {
    Normal,
    WorkerFail,
    WorkerTimeout,
    ReviewerFail,
}

#[derive(Clone)]
struct ScenarioRunner {
    name: String,
    scenario: Scenario,
}

#[async_trait]
impl AgentRunner for ScenarioRunner {
    async fn run(&self, _prompt: &str) -> AgentResult<String> {
        match (self.scenario, self.name.as_str()) {
            (_, "planner") => Ok("planner: split task for three workers".to_string()),
            (Scenario::Normal, "worker_a") => {
                sleep(Duration::from_millis(250)).await;
                Ok("worker_a output".to_string())
            }
            (Scenario::Normal, "worker_b") => {
                sleep(Duration::from_millis(300)).await;
                Ok("worker_b output".to_string())
            }
            (Scenario::Normal, "worker_c") => {
                sleep(Duration::from_millis(350)).await;
                Ok("worker_c output".to_string())
            }
            (Scenario::WorkerFail, "worker_b") => {
                Err(AgentError::Model(ModelError::Other("worker_b failed".to_string())))
            }
            (Scenario::WorkerFail, "worker_a") | (Scenario::WorkerFail, "worker_c") => {
                Ok(format!("{} output", self.name))
            }
            (Scenario::WorkerTimeout, "worker_c") => {
                sleep(Duration::from_millis(200)).await;
                Ok("worker_c slow output".to_string())
            }
            (Scenario::WorkerTimeout, "worker_a") | (Scenario::WorkerTimeout, "worker_b") => {
                Ok(format!("{} output", self.name))
            }
            (Scenario::ReviewerFail, "worker_a")
            | (Scenario::ReviewerFail, "worker_b")
            | (Scenario::ReviewerFail, "worker_c") => Ok(format!("{} output", self.name)),
            (Scenario::ReviewerFail, "reviewer") => {
                Err(AgentError::Model(ModelError::Other("reviewer failed".to_string())))
            }
            (_, "reviewer") => Ok("reviewer merged output".to_string()),
            _ => Err(AgentError::InvalidConfig("unknown test runner".to_string())),
        }
    }
}

#[derive(Clone)]
struct PromptEchoRunner {
    name: String,
    prompts: Arc<Mutex<HashMap<String, String>>>,
}

#[async_trait]
impl AgentRunner for PromptEchoRunner {
    async fn run(&self, prompt: &str) -> AgentResult<String> {
        self.prompts
            .lock()
            .await
            .insert(self.name.clone(), prompt.to_string());

        match self.name.as_str() {
            "planner" => Ok("planner output".to_string()),
            "worker_a" | "worker_b" | "worker_c" => Ok(format!("{} output", self.name)),
            "reviewer" => Ok(prompt.to_string()),
            _ => Err(AgentError::InvalidConfig(
                "unknown prompt echo runner".to_string(),
            )),
        }
    }
}

#[tokio::test]
async fn test_agent_builder_run() {
    let agent = AgentBuilder::new().with_model(MockModel).build().unwrap();
    let result = agent.run("hello").await.unwrap();
    assert_eq!(result, "mock-response");
}

#[tokio::test]
async fn test_orchestrator_parallel_merge_success() {
    let orchestrator = Orchestrator::new(
        base_definitions(),
        base_runners(Scenario::Normal),
        OrchestratorOptions::default(),
    )
    .unwrap();

    let result = orchestrator
        .execute(OrchestrationRequest::new("ship multi-agent pipeline"))
        .await
        .unwrap();

    assert_eq!(result.branch_results.len(), 3);
    assert!(
        result
            .branch_results
            .iter()
            .all(|item| item.status == StepStatus::Success)
    );
    assert!(result.final_output.contains("reviewer merged output"));
    assert!(result.blackboard.get("run_id").is_some());
    assert!(result.blackboard.get("goal").is_some());
    assert!(result.blackboard.get("planner").is_some());
    assert!(result.blackboard.get("branches").is_some());
    assert!(result.blackboard.get("reviewer").is_some());
    assert!(result.blackboard.get("metrics").is_some());

    for entry in &result.history {
        if let Some(artifact_id) = &entry.artifact_id {
            assert!(
                result
                    .artifacts
                    .iter()
                    .any(|artifact| &artifact.id == artifact_id)
            );
        }
    }

    // sequential workers would be ~900ms, parallel should be significantly lower
    assert!(result.timings.total_ms < 800);
}

#[tokio::test]
async fn test_orchestrator_branch_failure_is_tolerated() {
    let orchestrator = Orchestrator::new(
        base_definitions(),
        base_runners(Scenario::WorkerFail),
        OrchestratorOptions::default(),
    )
    .unwrap();

    let result = orchestrator
        .execute(OrchestrationRequest::new("ship multi-agent pipeline"))
        .await
        .unwrap();

    let failed = result
        .branch_results
        .iter()
        .filter(|item| item.status == StepStatus::Failed)
        .count();
    assert_eq!(failed, 1);
    assert!(result.final_output.contains("reviewer merged output"));
}

#[tokio::test]
async fn test_orchestrator_branch_timeout_is_tolerated() {
    let orchestrator = Orchestrator::new(
        base_definitions(),
        base_runners(Scenario::WorkerTimeout),
        OrchestratorOptions {
            branch_timeout: Duration::from_millis(30),
        },
    )
    .unwrap();

    let result = orchestrator
        .execute(OrchestrationRequest::new("ship multi-agent pipeline"))
        .await
        .unwrap();

    let timeout_count = result
        .branch_results
        .iter()
        .filter(|item| item.status == StepStatus::Timeout)
        .count();
    assert_eq!(timeout_count, 1);
    assert!(!result.final_output.is_empty());
}

#[tokio::test]
async fn test_orchestrator_reviewer_failure_fallback() {
    let orchestrator = Orchestrator::new(
        base_definitions(),
        base_runners(Scenario::ReviewerFail),
        OrchestratorOptions::default(),
    )
    .unwrap();

    let result = orchestrator
        .execute(OrchestrationRequest::new("ship multi-agent pipeline"))
        .await
        .unwrap();

    assert!(
        result
            .final_output
            .contains("Reviewer failed. Returning deterministic fallback summary.")
    );
}

#[tokio::test]
async fn test_governed_orchestrator_injects_preflight_and_postrun_context() {
    let (runners, prompts) = prompt_echo_runners();
    let orchestrator =
        Orchestrator::new(base_definitions(), runners, OrchestratorOptions::default()).unwrap();
    let governed = GovernedOrchestrator::new(orchestrator);

    let governance = GovernanceInjectionRequest {
        preflight_summary: Some("block write operations on production configs".to_string()),
        issues: vec![agent_lib::GovernanceInjectionIssue {
            severity: GovernanceInjectionSeverity::Blocker,
            code: "cfg_write_prod".to_string(),
            message: "production config writes require approval".to_string(),
        }],
    };

    let result = governed
        .execute(
            OrchestrationRequest::new("ship multi-agent pipeline"),
            Some(governance),
        )
        .await
        .unwrap();

    assert!(result.governance.preflight_applied);
    assert!(result.governance.postrun_applied);
    assert!(
        result
            .governance
            .preflight_context
            .as_deref()
            .unwrap_or_default()
            .contains("production config writes require approval")
    );
    assert!(
        result
            .governance
            .postrun_context
            .as_deref()
            .unwrap_or_default()
            .contains("Branch metrics")
    );

    let prompts = prompts.lock().await;
    let planner_prompt = prompts.get("planner").cloned().unwrap_or_default();
    let reviewer_prompt = prompts.get("reviewer").cloned().unwrap_or_default();

    assert!(planner_prompt.contains("Governance preflight"));
    assert!(planner_prompt.contains("cfg_write_prod"));
    assert!(reviewer_prompt.contains("Governance postrun review"));
    assert!(reviewer_prompt.contains("Branch metrics"));
}

#[test]
fn test_orchestrator_validation_requires_single_planner() {
    let mut definitions = base_definitions();
    definitions[0].role = AgentRole::Worker;

    let result = Orchestrator::new(
        definitions,
        base_runners(Scenario::Normal),
        OrchestratorOptions::default(),
    );
    assert!(result.is_err());
    let err = result.err().unwrap();
    assert!(err.to_string().contains("expected exactly one planner"));
}

#[test]
fn test_orchestrator_validation_rejects_invalid_parallel_target() {
    let mut definitions = base_definitions();
    definitions[0].parallel_targets = vec!["reviewer".to_string()];

    let result = Orchestrator::new(
        definitions,
        base_runners(Scenario::Normal),
        OrchestratorOptions::default(),
    );
    assert!(result.is_err());
    let err = result.err().unwrap();
    assert!(
        err.to_string()
            .contains("parallel target must be worker role")
    );
}

#[test]
fn test_orchestrator_validation_rejects_missing_runner() {
    let mut runners = base_runners(Scenario::Normal);
    runners.remove("worker_a");

    let result = Orchestrator::new(base_definitions(), runners, OrchestratorOptions::default());
    assert!(result.is_err());
    let err = result.err().unwrap();
    assert!(err.to_string().contains("runner missing for agent"));
}

#[derive(Default)]
struct MockReceiver {
    last_context: Mutex<Option<serde_json::Value>>,
}

#[async_trait]
impl HandoffReceiver for MockReceiver {
    async fn receive_handoff(&self, context: serde_json::Value) -> AgentResult<()> {
        let mut guard = self.last_context.lock().await;
        *guard = Some(context);
        Ok(())
    }
}

#[tokio::test]
async fn test_agent_registry_notify() {
    let registry = AgentRegistry::new();
    let receiver = Arc::new(MockReceiver::default());
    registry.register("agent_x", receiver.clone()).await;

    let payload = json!({"hello": "world"});
    registry.notify("agent_x", payload.clone()).await.unwrap();

    let stored = receiver.last_context.lock().await.clone();
    assert_eq!(stored, Some(payload));
}

fn base_definitions() -> Vec<AgentDefinition> {
    vec![
        AgentDefinition {
            name: "planner".to_string(),
            instructions: "plan".to_string(),
            tools: vec![],
            model: "mock".to_string(),
            role: AgentRole::Planner,
            parallel_targets: vec![
                "worker_a".to_string(),
                "worker_b".to_string(),
                "worker_c".to_string(),
            ],
        },
        AgentDefinition {
            name: "worker_a".to_string(),
            instructions: "worker a".to_string(),
            tools: vec![],
            model: "mock".to_string(),
            role: AgentRole::Worker,
            parallel_targets: vec![],
        },
        AgentDefinition {
            name: "worker_b".to_string(),
            instructions: "worker b".to_string(),
            tools: vec![],
            model: "mock".to_string(),
            role: AgentRole::Worker,
            parallel_targets: vec![],
        },
        AgentDefinition {
            name: "worker_c".to_string(),
            instructions: "worker c".to_string(),
            tools: vec![],
            model: "mock".to_string(),
            role: AgentRole::Worker,
            parallel_targets: vec![],
        },
        AgentDefinition {
            name: "reviewer".to_string(),
            instructions: "review".to_string(),
            tools: vec![],
            model: "mock".to_string(),
            role: AgentRole::Reviewer,
            parallel_targets: vec![],
        },
    ]
}

fn base_runners(scenario: Scenario) -> HashMap<String, Arc<dyn AgentRunner>> {
    let mut runners: HashMap<String, Arc<dyn AgentRunner>> = HashMap::new();
    for name in ["planner", "worker_a", "worker_b", "worker_c", "reviewer"] {
        runners.insert(
            name.to_string(),
            Arc::new(ScenarioRunner {
                name: name.to_string(),
                scenario,
            }),
        );
    }
    runners
}

fn prompt_echo_runners() -> (
    HashMap<String, Arc<dyn AgentRunner>>,
    Arc<Mutex<HashMap<String, String>>>,
) {
    let prompts = Arc::new(Mutex::new(HashMap::new()));
    let mut runners: HashMap<String, Arc<dyn AgentRunner>> = HashMap::new();

    for name in ["planner", "worker_a", "worker_b", "worker_c", "reviewer"] {
        runners.insert(
            name.to_string(),
            Arc::new(PromptEchoRunner {
                name: name.to_string(),
                prompts: Arc::clone(&prompts),
            }),
        );
    }

    (runners, prompts)
}
