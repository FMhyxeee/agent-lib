use serde::{Deserialize, Serialize};

use crate::model::Message;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ConversationHistory {
    messages: Vec<Message>,
}

impl ConversationHistory {
    pub fn new() -> Self {
        Self { messages: Vec::new() }
    }

    pub fn push(&mut self, message: Message) {
        self.messages.push(message);
    }

    pub fn all(&self) -> &[Message] {
        &self.messages
    }
}
