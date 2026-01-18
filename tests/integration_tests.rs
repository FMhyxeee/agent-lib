use agent_lib::model::provider::LocalProvider;
use agent_lib::{AgentBuilder, AgentError};

#[tokio::test]
async fn agent_run_returns_error_for_stub_provider() {
    let agent = AgentBuilder::new()
        .with_model(LocalProvider::new("local-model"))
        .build()
        .unwrap();

    let err = agent.run("hello").await.err().unwrap();
    assert!(matches!(err, AgentError::Model(_)));
}
