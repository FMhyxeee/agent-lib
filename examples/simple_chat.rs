use agent_lib::model::provider::LocalProvider;
use agent_lib::{AgentBuilder, AgentResult};

#[tokio::main]
async fn main() -> AgentResult<()> {
    let agent = AgentBuilder::new()
        .with_model(LocalProvider::new("local-model"))
        .with_instructions("You are a helpful assistant.")
        .build()?;

    match agent.run("Hello!").await {
        Ok(response) => println!("Response: {response}"),
        Err(err) => eprintln!("Run failed (expected in stub mode): {err}"),
    }

    Ok(())
}
