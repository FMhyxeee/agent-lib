use std::collections::HashMap;
use std::sync::Arc;

use agent_lib::agent::{
    AgentDefinition, AgentRole, AgentRunner, OrchestrationRequest, OrchestratorOptions,
};
use agent_lib::{AgentError, AgentResult, Orchestrator};
use async_trait::async_trait;
use tokio::time::{Duration, sleep};

#[derive(Clone)]
struct DemoRunner {
    name: String,
}

#[async_trait]
impl AgentRunner for DemoRunner {
    async fn run(&self, _prompt: &str) -> AgentResult<String> {
        match self.name.as_str() {
            "planner" => Ok("Split task into API, UI, and testing workstreams.".to_string()),
            "worker_api" => Ok("API branch: expose orchestration execute endpoint.".to_string()),
            "worker_ui" => {
                sleep(Duration::from_millis(300)).await;
                Ok("UI branch: render branch status and blackboard summary.".to_string())
            }
            "worker_test" => Err(AgentError::Model(
                "simulated branch failure for demonstration".to_string(),
            )),
            "reviewer" => Ok("Final result: combine API+UI work, and flag missing testing branch due to failure."
                .to_string()),
            _ => Err(AgentError::InvalidConfig("unknown demo runner".to_string())),
        }
    }
}

fn build_agents() -> Vec<AgentDefinition> {
    vec![
        AgentDefinition {
            name: "planner".to_string(),
            instructions: "Plan parallel implementation".to_string(),
            tools: vec![],
            model: "local".to_string(),
            role: AgentRole::Planner,
            parallel_targets: vec![
                "worker_api".to_string(),
                "worker_ui".to_string(),
                "worker_test".to_string(),
            ],
        },
        AgentDefinition {
            name: "worker_api".to_string(),
            instructions: "Implement backend path".to_string(),
            tools: vec![],
            model: "local".to_string(),
            role: AgentRole::Worker,
            parallel_targets: vec![],
        },
        AgentDefinition {
            name: "worker_ui".to_string(),
            instructions: "Implement frontend path".to_string(),
            tools: vec![],
            model: "local".to_string(),
            role: AgentRole::Worker,
            parallel_targets: vec![],
        },
        AgentDefinition {
            name: "worker_test".to_string(),
            instructions: "Implement tests path".to_string(),
            tools: vec![],
            model: "local".to_string(),
            role: AgentRole::Worker,
            parallel_targets: vec![],
        },
        AgentDefinition {
            name: "reviewer".to_string(),
            instructions: "Merge branch outputs into final answer".to_string(),
            tools: vec![],
            model: "local".to_string(),
            role: AgentRole::Reviewer,
            parallel_targets: vec![],
        },
    ]
}

fn build_runners() -> HashMap<String, Arc<dyn AgentRunner>> {
    let mut runners: HashMap<String, Arc<dyn AgentRunner>> = HashMap::new();
    for name in [
        "planner",
        "worker_api",
        "worker_ui",
        "worker_test",
        "reviewer",
    ] {
        runners.insert(
            name.to_string(),
            Arc::new(DemoRunner {
                name: name.to_string(),
            }),
        );
    }
    runners
}

#[tokio::main]
async fn main() -> AgentResult<()> {
    let orchestrator = Orchestrator::new(
        build_agents(),
        build_runners(),
        OrchestratorOptions::default(),
    )?;

    let result = orchestrator
        .execute(OrchestrationRequest::new(
            "Build local multi-agent orchestration for coding tasks.",
        ))
        .await?;

    println!("Run ID: {}", result.run_id);
    println!("Final output: {}", result.final_output);
    println!(
        "Branches: {}",
        serde_json::to_string_pretty(&result.branch_results).unwrap_or_default()
    );
    Ok(())
}
