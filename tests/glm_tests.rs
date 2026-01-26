use std::env;

use agent_lib::model::provider::GlmProvider;
use agent_lib::{AgentBuilder, AgentResult};

#[tokio::test]
async fn glm_chat_smoke_test() -> AgentResult<()> {
    let base_url = match env::var("GLM_BASE_URL") {
        Ok(value) => value,
        Err(_) => return Ok(()),
    };
    let api_key = match env::var("GLM_API_KEY") {
        Ok(value) => value,
        Err(_) => return Ok(()),
    };

    let agent = AgentBuilder::new()
        .with_model(GlmProvider::new("GLM-4.7-flashX", api_key).with_base_url(base_url))
        .build()?;

    let response = agent.run("你好").await?;
    println!("response: {}", response);
    assert!(!response.trim().is_empty());
    Ok(())
}
