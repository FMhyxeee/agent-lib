use std::sync::Arc;

use tokio::sync::{mpsc, Mutex};
use tokio_stream::wrappers::ReceiverStream;
use tokio_stream::StreamExt;

use crate::error::{AgentError, AgentResult};
use crate::protocol::{Event, EventQueue, Op, SubmissionQueue};
use crate::session::{ConversationHistory, SessionState};

#[derive(Debug)]
pub struct Session {
    history: Arc<Mutex<ConversationHistory>>,
    state: Arc<Mutex<SessionState>>,
    submission: SubmissionQueue,
    event_sender: mpsc::Sender<Event>,
}

#[derive(Clone, Debug)]
pub struct SessionHandle {
    submission: SubmissionQueue,
    event_stream: Arc<Mutex<ReceiverStream<Event>>>,
}

impl Session {
    pub fn new(buffer: usize) -> (Self, SessionHandle) {
        let (op_sender, op_receiver) = mpsc::channel(buffer);
        let submission = SubmissionQueue::new(op_sender);

        let (event_sender, event_queue) = EventQueue::new(buffer);
        let event_stream = event_queue.stream();

        let session = Self {
            history: Arc::new(Mutex::new(ConversationHistory::new())),
            state: Arc::new(Mutex::new(SessionState::Idle)),
            submission,
            event_sender,
        };

        let handle = SessionHandle {
            submission: session.submission.clone(),
            event_stream: Arc::new(Mutex::new(event_stream)),
        };

        tokio::spawn(session_loop(op_receiver, session.event_sender.clone()));

        (session, handle)
    }

    pub async fn history(&self) -> ConversationHistory {
        self.history.lock().await.clone()
    }

    pub async fn state(&self) -> SessionState {
        self.state.lock().await.clone()
    }
}

impl SessionHandle {
    pub async fn submit(&self, op: Op) -> AgentResult<()> {
        self.submission
            .submit(op)
            .await
            .map_err(|err| AgentError::Session(err.to_string()))
    }

    pub async fn next_event(&self) -> Option<Event> {
        self.event_stream.lock().await.next().await
    }
}

async fn session_loop(mut op_receiver: mpsc::Receiver<Op>, event_sender: mpsc::Sender<Event>) {
    while let Some(op) = op_receiver.recv().await {
        let _ = event_sender
            .send(Event::TurnStarted {
                turn_id: uuid::Uuid::new_v4().to_string(),
            })
            .await;

        match op {
            Op::StartTurn { prompt, .. } => {
                let _ = event_sender
                    .send(Event::ModelComplete {
                        content: prompt,
                        usage: Default::default(),
                    })
                    .await;
            }
            Op::UserInput { content } => {
                let _ = event_sender
                    .send(Event::ModelStreaming { chunk: content })
                    .await;
            }
            Op::ApprovalResponse { request_id, .. } => {
                let _ = event_sender
                    .send(Event::ToolCallResult {
                        tool: "approval".to_string(),
                        result: crate::tools::ToolResult::text(format!(
                            "approval response: {request_id}"
                        )),
                    })
                    .await;
            }
            Op::Interrupt => {
                let _ = event_sender
                    .send(Event::Error {
                        error: AgentError::Session("session interrupted".to_string()),
                    })
                    .await;
            }
            Op::Handoff { target_agent, .. } => {
                let _ = event_sender
                    .send(Event::HandoffInitiated {
                        from: "session".to_string(),
                        to: target_agent,
                    })
                    .await;
            }
        }
    }
}

