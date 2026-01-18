use agent_lib::agent::AgentDefinition;
use agent_lib::{AgentResult, Orchestrator};

fn build_agents() -> Vec<AgentDefinition> {
    vec![
        AgentDefinition {
            name: "planner".to_string(),
            instructions: "Plan the task.".to_string(),
            tools: vec![],
            model: "local".to_string(),
            handoff_targets: vec!["coder".to_string()],
        },
        AgentDefinition {
            name: "coder".to_string(),
            instructions: "Write code for the task.".to_string(),
            tools: vec![],
            model: "local".to_string(),
            handoff_targets: vec!["reviewer".to_string()],
        },
        AgentDefinition {
            name: "reviewer".to_string(),
            instructions: "Review the code.".to_string(),
            tools: vec![],
            model: "local".to_string(),
            handoff_targets: vec![],
        },
    ]
}

fn main() -> AgentResult<()> {
    let agents = build_agents();
    let mut orchestrator = Orchestrator::new(agents, "planner")?;

    println!("Current agent: {}", orchestrator.current_agent().name);
    orchestrator.handoff_to("coder")?;
    println!("After handoff: {}", orchestrator.current_agent().name);
    Ok(())
}
