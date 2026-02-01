use std::sync::Arc;

use async_trait::async_trait;
use serde_json::json;
use tokio::sync::Mutex;

use agent_lib::agent::{AgentDefinition, HandoffReceiver};
use agent_lib::model::{Message, ModelClient, ModelResponse, StreamChunk, TokenUsage};
use agent_lib::{AgentBuilder, AgentRegistry, AgentResult, Orchestrator};

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

#[tokio::test]
async fn test_agent_builder_run() {
    let agent = AgentBuilder::new().with_model(MockModel).build().unwrap();
    let result = agent.run("hello").await.unwrap();
    assert_eq!(result, "mock-response");
}

#[test]
fn test_orchestrator_handoff() {
    let agent_a = AgentDefinition {
        name: "agent_a".to_string(),
        instructions: "A".to_string(),
        tools: vec![],
        model: "mock".to_string(),
        handoff_targets: vec!["agent_b".to_string()],
    };
    let agent_b = AgentDefinition {
        name: "agent_b".to_string(),
        instructions: "B".to_string(),
        tools: vec![],
        model: "mock".to_string(),
        handoff_targets: vec![],
    };

    let mut orchestrator = Orchestrator::new(vec![agent_a, agent_b], "agent_a").unwrap();
    assert_eq!(orchestrator.current_agent().name, "agent_a");
    orchestrator.handoff_to("agent_b").unwrap();
    assert_eq!(orchestrator.current_agent().name, "agent_b");
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
