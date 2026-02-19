use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Instant;

use chrono::Utc;
use futures::future::join_all;
use serde_json::{Map, Value, json};
use tokio::time::timeout;
use uuid::Uuid;

use crate::agent::{
    AgentDefinition, AgentRole, AgentRunner, BranchResult, OrchestrationArtifact,
    OrchestrationHistoryEntry, OrchestrationRequest, OrchestrationResult, OrchestrationTimings,
    OrchestratorOptions, StepStatus,
};
use crate::error::{AgentError, AgentResult};

struct WorkerExecution {
    worker: String,
    status: StepStatus,
    output: Option<String>,
    error: Option<String>,
    duration_ms: u64,
}

pub struct Orchestrator {
    definitions: HashMap<String, AgentDefinition>,
    runners: HashMap<String, Arc<dyn AgentRunner>>,
    planner_name: String,
    reviewer_name: String,
    options: OrchestratorOptions,
}

impl Orchestrator {
    pub fn new(
        definitions: Vec<AgentDefinition>,
        runners: HashMap<String, Arc<dyn AgentRunner>>,
        options: OrchestratorOptions,
    ) -> AgentResult<Self> {
        let mut map = HashMap::new();
        for definition in definitions {
            if map.insert(definition.name.clone(), definition).is_some() {
                return Err(AgentError::InvalidConfig(
                    "duplicate agent definition name".to_string(),
                ));
            }
        }

        let planner_name = Self::find_unique_role(&map, AgentRole::Planner, "planner")?;
        let reviewer_name = Self::find_unique_role(&map, AgentRole::Reviewer, "reviewer")?;

        let planner = map
            .get(&planner_name)
            .ok_or_else(|| AgentError::InvalidConfig("planner definition missing".to_string()))?;
        if planner.parallel_targets.is_empty() {
            return Err(AgentError::InvalidConfig(
                "planner.parallel_targets cannot be empty".to_string(),
            ));
        }

        let mut seen_targets = HashSet::new();
        for target in &planner.parallel_targets {
            if !seen_targets.insert(target.clone()) {
                return Err(AgentError::InvalidConfig(format!(
                    "duplicate parallel target: {target}"
                )));
            }

            let target_def = map.get(target).ok_or_else(|| {
                AgentError::InvalidConfig(format!("parallel target not found: {target}"))
            })?;
            if target_def.role != AgentRole::Worker {
                return Err(AgentError::InvalidConfig(format!(
                    "parallel target must be worker role: {target}"
                )));
            }
            if target_def.name == reviewer_name {
                return Err(AgentError::InvalidConfig(
                    "reviewer cannot be in planner.parallel_targets".to_string(),
                ));
            }
        }

        for required in planner
            .parallel_targets
            .iter()
            .chain(std::iter::once(&planner_name))
            .chain(std::iter::once(&reviewer_name))
        {
            if !runners.contains_key(required) {
                return Err(AgentError::InvalidConfig(format!(
                    "runner missing for agent: {required}"
                )));
            }
        }

        Ok(Self {
            definitions: map,
            runners,
            planner_name,
            reviewer_name,
            options,
        })
    }

    pub async fn execute(
        &self,
        mut request: OrchestrationRequest,
    ) -> AgentResult<OrchestrationResult> {
        if request.goal.trim().is_empty() {
            return Err(AgentError::InvalidConfig(
                "goal cannot be empty".to_string(),
            ));
        }

        let run_id = request
            .run_id
            .take()
            .unwrap_or_else(|| Uuid::new_v4().to_string());
        let total_start = Instant::now();

        let mut artifacts = Vec::new();
        let mut history = Vec::new();

        let planner_definition = self.definition(&self.planner_name)?.clone();
        let planner_runner = self.runner(&self.planner_name)?;
        let planner_prompt = build_planner_prompt(&request.goal, &planner_definition.instructions);
        let planner_start = Instant::now();
        let planner_output = planner_runner.run(&planner_prompt).await?;
        let planner_ms = elapsed_ms(planner_start);

        let planner_artifact_id = push_artifact(&mut artifacts, "planner", planner_output.clone());
        let planner_summary = summarize_text(&planner_output, 220);
        history.push(OrchestrationHistoryEntry {
            step: "planner".to_string(),
            status: StepStatus::Success,
            summary: planner_summary.clone(),
            artifact_id: Some(planner_artifact_id.clone()),
            timestamp: Utc::now(),
            duration_ms: Some(planner_ms),
        });

        let mut worker_tasks = Vec::new();
        for target in &planner_definition.parallel_targets {
            let worker_name = target.clone();
            let worker_definition = self.definition(&worker_name)?.clone();
            let worker_runner = self.runner(&worker_name)?;
            let goal = request.goal.clone();
            let planner_output_for_worker = planner_output.clone();
            let timeout_duration = self.options.branch_timeout;

            worker_tasks.push(async move {
                let worker_prompt = build_worker_prompt(
                    &goal,
                    &planner_output_for_worker,
                    &worker_name,
                    &worker_definition.instructions,
                );
                let started = Instant::now();
                match timeout(timeout_duration, worker_runner.run(&worker_prompt)).await {
                    Ok(Ok(output)) => WorkerExecution {
                        worker: worker_name,
                        status: StepStatus::Success,
                        output: Some(output),
                        error: None,
                        duration_ms: elapsed_ms(started),
                    },
                    Ok(Err(err)) => WorkerExecution {
                        worker: worker_name,
                        status: StepStatus::Failed,
                        output: None,
                        error: Some(err.to_string()),
                        duration_ms: elapsed_ms(started),
                    },
                    Err(_) => WorkerExecution {
                        worker: worker_name,
                        status: StepStatus::Timeout,
                        output: None,
                        error: Some(format!(
                            "branch exceeded timeout ({}s)",
                            timeout_duration.as_secs()
                        )),
                        duration_ms: elapsed_ms(started),
                    },
                }
            });
        }

        let worker_executions = join_all(worker_tasks).await;
        let mut branch_results = Vec::new();
        let mut branch_ms = HashMap::new();
        for execution in worker_executions {
            let summary;
            let artifact_id;
            match (&execution.status, &execution.output) {
                (StepStatus::Success, Some(output)) => {
                    artifact_id = Some(push_artifact(
                        &mut artifacts,
                        &execution.worker,
                        output.clone(),
                    ));
                    summary = summarize_text(output, 180);
                }
                _ => {
                    artifact_id = None;
                    summary = execution
                        .error
                        .clone()
                        .unwrap_or_else(|| "worker failed without error".to_string());
                }
            }

            history.push(OrchestrationHistoryEntry {
                step: format!("worker:{}", execution.worker),
                status: execution.status,
                summary: summary.clone(),
                artifact_id: artifact_id.clone(),
                timestamp: Utc::now(),
                duration_ms: Some(execution.duration_ms),
            });

            branch_ms.insert(execution.worker.clone(), execution.duration_ms);
            branch_results.push(BranchResult {
                worker: execution.worker,
                status: execution.status,
                summary,
                artifact_id,
                error: execution.error,
                duration_ms: execution.duration_ms,
            });
        }

        let reviewer_definition = self.definition(&self.reviewer_name)?.clone();
        let reviewer_runner = self.runner(&self.reviewer_name)?;
        let reviewer_prompt = build_reviewer_prompt(
            &request.goal,
            &planner_summary,
            &planner_artifact_id,
            &branch_results,
            &reviewer_definition.instructions,
        );
        let reviewer_start = Instant::now();
        let (final_output, reviewer_status, reviewer_error) =
            match reviewer_runner.run(&reviewer_prompt).await {
                Ok(output) => (output, StepStatus::Success, None),
                Err(err) => {
                    let fallback = build_reviewer_fallback(&request.goal, &branch_results);
                    (fallback, StepStatus::Failed, Some(err.to_string()))
                }
            };
        let reviewer_ms = elapsed_ms(reviewer_start);

        let reviewer_artifact_id = push_artifact(&mut artifacts, "reviewer", final_output.clone());
        history.push(OrchestrationHistoryEntry {
            step: "reviewer".to_string(),
            status: reviewer_status,
            summary: summarize_text(&final_output, 220),
            artifact_id: Some(reviewer_artifact_id.clone()),
            timestamp: Utc::now(),
            duration_ms: Some(reviewer_ms),
        });

        let success_count = branch_results
            .iter()
            .filter(|item| item.status == StepStatus::Success)
            .count();
        let failed_count = branch_results
            .iter()
            .filter(|item| item.status == StepStatus::Failed)
            .count();
        let timeout_count = branch_results
            .iter()
            .filter(|item| item.status == StepStatus::Timeout)
            .count();
        let total_ms = elapsed_ms(total_start);
        let blackboard = build_blackboard(
            &run_id,
            &request.goal,
            &planner_summary,
            &planner_artifact_id,
            &branch_results,
            reviewer_status,
            &reviewer_artifact_id,
            reviewer_error.as_deref(),
            success_count,
            failed_count,
            timeout_count,
            planner_ms,
            reviewer_ms,
            total_ms,
        );

        Ok(OrchestrationResult {
            run_id,
            goal: request.goal,
            final_output,
            planner_output,
            branch_results,
            blackboard,
            history,
            artifacts,
            timings: OrchestrationTimings {
                total_ms,
                planner_ms,
                reviewer_ms,
                branch_ms,
            },
        })
    }

    fn find_unique_role(
        definitions: &HashMap<String, AgentDefinition>,
        role: AgentRole,
        name: &str,
    ) -> AgentResult<String> {
        let matches: Vec<&AgentDefinition> = definitions
            .values()
            .filter(|definition| definition.role == role)
            .collect();
        if matches.len() != 1 {
            return Err(AgentError::InvalidConfig(format!(
                "expected exactly one {name} agent, found {}",
                matches.len()
            )));
        }
        Ok(matches[0].name.clone())
    }

    fn definition(&self, name: &str) -> AgentResult<&AgentDefinition> {
        self.definitions
            .get(name)
            .ok_or_else(|| AgentError::InvalidConfig(format!("agent definition not found: {name}")))
    }

    fn runner(&self, name: &str) -> AgentResult<Arc<dyn AgentRunner>> {
        self.runners
            .get(name)
            .cloned()
            .ok_or_else(|| AgentError::InvalidConfig(format!("agent runner not found: {name}")))
    }
}

fn elapsed_ms(started: Instant) -> u64 {
    started.elapsed().as_millis() as u64
}

fn push_artifact(
    artifacts: &mut Vec<OrchestrationArtifact>,
    step: &str,
    content: String,
) -> String {
    let id = format!("{step}-{}", Uuid::new_v4());
    artifacts.push(OrchestrationArtifact {
        id: id.clone(),
        step: step.to_string(),
        content,
    });
    id
}

fn summarize_text(content: &str, max_chars: usize) -> String {
    let mut result = String::new();
    for (idx, ch) in content.chars().enumerate() {
        if idx >= max_chars {
            result.push_str("...");
            break;
        }
        result.push(ch);
    }
    if result.is_empty() {
        "[empty]".to_string()
    } else {
        result
    }
}

fn build_planner_prompt(goal: &str, planner_instructions: &str) -> String {
    format!(
        "Role: Planner\nInstructions:\n{planner_instructions}\n\nGoal:\n{goal}\n\nOutput requirement:\nProvide a concise plan optimized for parallel execution by multiple workers."
    )
}

fn build_worker_prompt(
    goal: &str,
    planner_output: &str,
    worker_name: &str,
    worker_instructions: &str,
) -> String {
    format!(
        "Role: Worker\nWorker: {worker_name}\nInstructions:\n{worker_instructions}\n\nGlobal goal:\n{goal}\n\nPlanner output:\n{planner_output}\n\nOutput requirement:\nProvide an executable branch contribution for this worker only."
    )
}

fn build_reviewer_prompt(
    goal: &str,
    planner_summary: &str,
    planner_artifact_id: &str,
    branch_results: &[BranchResult],
    reviewer_instructions: &str,
) -> String {
    let mut branch_lines = Vec::new();
    for result in branch_results {
        let artifact = result.artifact_id.as_deref().unwrap_or("none");
        let error = result.error.as_deref().unwrap_or("none");
        branch_lines.push(format!(
            "- worker={} status={:?} artifact={} summary={} error={}",
            result.worker, result.status, artifact, result.summary, error
        ));
    }
    format!(
        "Role: Reviewer\nInstructions:\n{reviewer_instructions}\n\nGlobal goal:\n{goal}\n\nPlanner summary: {planner_summary}\nPlanner artifact: {planner_artifact_id}\n\nBranch results:\n{}\n\nOutput requirement:\nProduce a final merged answer and explicitly mention impacts from failed or timed-out branches.",
        branch_lines.join("\n")
    )
}

fn build_reviewer_fallback(goal: &str, branch_results: &[BranchResult]) -> String {
    let mut text = String::new();
    text.push_str("Reviewer failed. Returning deterministic fallback summary.\n");
    text.push_str(&format!("Goal: {goal}\n"));
    text.push_str("Successful branches:\n");
    let mut has_success = false;
    for result in branch_results {
        if result.status == StepStatus::Success {
            has_success = true;
            text.push_str(&format!(
                "- {}: {}\n",
                result.worker,
                summarize_text(&result.summary, 200)
            ));
        }
    }
    if !has_success {
        text.push_str("- none\n");
    }

    text.push_str("Failed/timeout branches:\n");
    let mut has_failure = false;
    for result in branch_results {
        if result.status != StepStatus::Success {
            has_failure = true;
            text.push_str(&format!(
                "- {} ({:?}): {}\n",
                result.worker,
                result.status,
                result.error.as_deref().unwrap_or("unknown error")
            ));
        }
    }
    if !has_failure {
        text.push_str("- none\n");
    }
    text
}

#[allow(clippy::too_many_arguments)]
fn build_blackboard(
    run_id: &str,
    goal: &str,
    planner_summary: &str,
    planner_artifact_id: &str,
    branch_results: &[BranchResult],
    reviewer_status: StepStatus,
    reviewer_artifact_id: &str,
    reviewer_error: Option<&str>,
    success_count: usize,
    failed_count: usize,
    timeout_count: usize,
    planner_ms: u64,
    reviewer_ms: u64,
    total_ms: u64,
) -> Value {
    let mut branches = Map::new();
    for result in branch_results {
        branches.insert(
            result.worker.clone(),
            json!({
                "status": result.status,
                "summary": result.summary,
                "artifact_id": result.artifact_id,
                "error": result.error,
                "duration_ms": result.duration_ms,
            }),
        );
    }

    json!({
        "run_id": run_id,
        "goal": goal,
        "planner": {
            "summary": planner_summary,
            "artifact_id": planner_artifact_id,
        },
        "branches": Value::Object(branches),
        "reviewer": {
            "status": reviewer_status,
            "artifact_id": reviewer_artifact_id,
            "error": reviewer_error,
        },
        "metrics": {
            "branch_count": branch_results.len(),
            "success_count": success_count,
            "failed_count": failed_count,
            "timeout_count": timeout_count,
            "planner_ms": planner_ms,
            "reviewer_ms": reviewer_ms,
            "total_ms": total_ms,
        }
    })
}
