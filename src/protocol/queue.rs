use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;

use crate::protocol::{Event, Op};

#[derive(Debug, Clone)]
pub struct SubmissionQueue {
    sender: mpsc::Sender<Op>,
}

#[derive(Debug)]
pub struct EventQueue {
    sender: mpsc::Sender<Event>,
    receiver: mpsc::Receiver<Event>,
}

impl SubmissionQueue {
    pub fn new(sender: mpsc::Sender<Op>) -> Self {
        Self { sender }
    }

    pub async fn submit(&self, op: Op) -> Result<(), mpsc::error::SendError<Op>> {
        self.sender.send(op).await
    }
}

impl EventQueue {
    pub fn new(buffer: usize) -> (mpsc::Sender<Event>, Self) {
        let (sender, receiver) = mpsc::channel(buffer);
        let queue = Self {
            sender: sender.clone(),
            receiver,
        };
        (sender, queue)
    }

    pub fn sender(&self) -> mpsc::Sender<Event> {
        self.sender.clone()
    }

    pub fn stream(self) -> ReceiverStream<Event> {
        ReceiverStream::new(self.receiver)
    }
}
