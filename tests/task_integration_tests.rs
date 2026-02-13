use std::sync::Arc;

use async_trait::async_trait;
use serde_json::json;
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;

use agent_lib::model::{Message, ModelClient, ModelResponse, StreamChunk, TokenUsage, ToolCall};
use agent_lib::protocol::Event;
use agent_lib::session::{ConversationHistory, TaskSession, TurnContext};
use agent_lib::tasks::{RegularTask, SessionTask};
use agent_lib::tools::{ToolDef, ToolResult};
use agent_lib::{AgentError, AgentResult};

#[derive(Default)]
struct MockModel {
    calls: Mutex<usize>,
}

#[async_trait]
impl ModelClient for MockModel {
    async fn chat(
        &self,
        _messages: Vec<Message>,
        _tools: Vec<ToolDef>,
    ) -> AgentResult<ModelResponse> {
        let mut calls = self.calls.lock().await;
        *calls += 1;
        if *calls == 1 {
            Ok(ModelResponse {
                content: "need tool".to_string(),
                usage: TokenUsage::default(),
                tool_calls: vec![ToolCall {
                    id: "tool-1".to_string(),
                    name: "echo".to_string(),
                    arguments: json!({"text": "ping"}),
                }],
            })
        } else {
            Ok(ModelResponse {
                content: "final answer".to_string(),
                usage: TokenUsage::default(),
                tool_calls: Vec::new(),
            })
        }
    }

    async fn chat_stream(
        &self,
        _messages: Vec<Message>,
        _tools: Vec<ToolDef>,
    ) -> AgentResult<std::pin::Pin<Box<dyn futures::Stream<Item = StreamChunk> + Send>>> {
        let stream = futures::stream::iter(vec![StreamChunk {
            delta: "mock".to_string(),
        }]);
        Ok(Box::pin(stream))
    }
}

struct FakeSession {
    history: Mutex<ConversationHistory>,
    events: Mutex<Vec<Event>>,
    model: Arc<dyn ModelClient>,
}

impl FakeSession {
    fn new(model: Arc<dyn ModelClient>) -> Self {
        let mut history = ConversationHistory::new();
        history.push(Message::system("system"));
        history.push(Message::user("user"));
        Self {
            history: Mutex::new(history),
            events: Mutex::new(Vec::new()),
            model,
        }
    }
}

#[async_trait]
impl TaskSession for FakeSession {
    async fn push_message(&self, message: Message) {
        let mut history = self.history.lock().await;
        history.push(message);
        // 修复测试：确保 push_message 写回历史
    }

    async fn history(&self) -> ConversationHistory {
        self.history.lock().await.clone()
    }

    async fn compact_history(&self, keep_recent: usize, summary: String) {
        let mut history = self.history.lock().await;
        history.compact(keep_recent, summary);
    }

    async fn emit_event(&self, event: Event) {
        self.events.lock().await.push(event);
    }

    async fn undo_last_messages(&self, num_messages: usize) {
        let mut history = self.history.lock().await;
        if num_messages > 0 && history.len() > num_messages {
            let new_messages: Vec<Message> = history
                .all()
                .iter()
                .take(history.len() - num_messages)
                .cloned()
                .collect();
            history.clear();
            for msg in new_messages {
                history.push(msg);
            }
        }
    }

    async fn chat_model(
        &self,
        messages: Vec<Message>,
        tools: Vec<ToolDef>,
    ) -> AgentResult<ModelResponse> {
        self.model.chat(messages, tools).await
    }

    async fn list_tools(&self) -> Vec<ToolDef> {
        vec![ToolDef {
            name: "echo".to_string(),
            description: "echo".to_string(),
            schema: json!({"type": "object"}),
        }]
    }

    async fn execute_tool(&self, name: &str, args: serde_json::Value) -> AgentResult<ToolResult> {
        if name == "echo" {
            Ok(ToolResult::text(args.to_string()))
        } else {
            Err(AgentError::Tool("unknown tool".to_string()))
        }
    }
}

#[tokio::test]
async fn test_regular_task_tool_loop() {
    let model = Arc::new(MockModel::default());
    let session = Arc::new(FakeSession::new(model));
    let ctx = Arc::new(TurnContext::default());
    let task = Arc::new(RegularTask::default());
    let token = CancellationToken::new();

    let result = task.run(session.clone(), ctx, token).await;
    assert!(result.is_some());

    let events = session.events.lock().await;
    let has_tool_request = events
        .iter()
        .any(|e| matches!(e, Event::ToolCallRequested { .. }));
    let has_tool_result = events
        .iter()
        .any(|e| matches!(e, Event::ToolCallResult { .. }));
    let has_complete = events
        .iter()
        .any(|e| matches!(e, Event::ModelComplete { .. }));
    assert!(has_tool_request);
    assert!(has_tool_result);
    assert!(has_complete);
}

#[tokio::test]
async fn test_regular_task_cancelled() {
    let model = Arc::new(MockModel::default());
    let session = Arc::new(FakeSession::new(model));
    let ctx = Arc::new(TurnContext::default());
    let task = Arc::new(RegularTask::default());
    let token = CancellationToken::new();
    token.cancel();

    let result = task.run(session, ctx, token).await;
    assert!(result.is_none());
}
