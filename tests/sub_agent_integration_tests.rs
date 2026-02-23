use std::pin::Pin;
use std::sync::Arc;

use async_trait::async_trait;
use futures::Stream;
use tokio::time::{Duration, timeout};

use agent_lib::AgentResult;
use agent_lib::model::{Message, ModelClient, ModelResponse, StreamChunk, TokenUsage};
use agent_lib::protocol::{Event, Op, SubAgentMode};
use agent_lib::session::{Session, SessionConfig};
use agent_lib::tools::ToolDef;

struct StaticModel;

#[async_trait]
impl ModelClient for StaticModel {
    async fn chat(
        &self,
        _messages: Vec<Message>,
        _tools: Vec<ToolDef>,
    ) -> AgentResult<ModelResponse> {
        Ok(ModelResponse {
            content: "sub-agent ready".to_string(),
            usage: TokenUsage::default(),
            tool_calls: vec![],
        })
    }

    async fn chat_stream(
        &self,
        _messages: Vec<Message>,
        _tools: Vec<ToolDef>,
    ) -> AgentResult<Pin<Box<dyn Stream<Item = StreamChunk> + Send>>> {
        Ok(Box::pin(futures::stream::empty()))
    }
}

#[tokio::test]
async fn run_sub_agent_emits_lifecycle_events() {
    let (_session, handle) = Session::with_config(
        16,
        SessionConfig {
            model: Some(Arc::new(StaticModel) as Arc<dyn ModelClient>),
            ..Default::default()
        },
    );

    handle
        .submit(Op::RunSubAgent {
            mode: SubAgentMode::Explore,
            input: "inspect architecture".to_string(),
        })
        .await
        .expect("submit should succeed");

    let mut has_started = false;
    let mut has_progress = false;
    let mut has_model_complete = false;
    let mut has_completed = false;
    let mut has_failed = false;

    for _ in 0..30 {
        let next = timeout(Duration::from_millis(500), handle.next_event()).await;
        let Ok(event) = next else {
            break;
        };
        let Some(event) = event else {
            continue;
        };

        match event {
            Event::SubAgentStarted { mode, input } => {
                assert_eq!(mode, SubAgentMode::Explore);
                assert_eq!(input, "inspect architecture");
                has_started = true;
            }
            Event::SubAgentProgress { mode, .. } => {
                assert_eq!(mode, SubAgentMode::Explore);
                has_progress = true;
            }
            Event::ModelComplete { content, .. } => {
                assert_eq!(content, "sub-agent ready");
                has_model_complete = true;
            }
            Event::SubAgentCompleted { mode, output } => {
                assert_eq!(mode, SubAgentMode::Explore);
                assert_eq!(output, "sub-agent ready");
                has_completed = true;
            }
            Event::SubAgentFailed { .. } => {
                has_failed = true;
                break;
            }
            _ => {}
        }

        if has_started && has_progress && has_model_complete && has_completed {
            break;
        }
    }

    assert!(has_started);
    assert!(has_progress);
    assert!(has_model_complete);
    assert!(has_completed);
    assert!(!has_failed);
}

#[tokio::test]
async fn run_sub_agent_emits_failed_event_when_model_missing() {
    let (_session, handle) = Session::with_config(16, SessionConfig::default());

    handle
        .submit(Op::RunSubAgent {
            mode: SubAgentMode::Plan,
            input: "build rollout plan".to_string(),
        })
        .await
        .expect("submit should succeed");

    let mut has_failed = false;
    let mut has_error = false;

    for _ in 0..30 {
        let next = timeout(Duration::from_millis(500), handle.next_event()).await;
        let Ok(event) = next else {
            break;
        };
        let Some(event) = event else {
            continue;
        };

        match event {
            Event::SubAgentFailed { mode, .. } => {
                assert_eq!(mode, SubAgentMode::Plan);
                has_failed = true;
            }
            Event::Error { .. } => {
                has_error = true;
            }
            _ => {}
        }

        if has_failed && has_error {
            break;
        }
    }

    assert!(has_failed);
    assert!(has_error);
}
