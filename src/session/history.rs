use serde::{Deserialize, Serialize};

use crate::model::Message;
use crate::token::TokenCounter;

/// 压缩摘要
///
/// 存储被压缩的对话历史的摘要信息。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompactedSummary {
    pub turn_id: String,
    pub summary: String,
    pub original_token_count: usize,
    pub timestamp: i64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ConversationHistory {
    messages: Vec<Message>,
    compacted_summaries: Vec<CompactedSummary>,
    #[serde(skip)]
    token_counter: TokenCounter,
}

impl ConversationHistory {
    pub fn new() -> Self {
        Self {
            messages: Vec::new(),
            compacted_summaries: Vec::new(),
            token_counter: TokenCounter::default(),
        }
    }

    /// 使用指定的 Token 计数器创建历史
    pub fn with_token_counter(token_counter: TokenCounter) -> Self {
        Self {
            messages: Vec::new(),
            compacted_summaries: Vec::new(),
            token_counter,
        }
    }

    pub fn push(&mut self, message: Message) {
        self.messages.push(message);
    }

    pub fn all(&self) -> &[Message] {
        &self.messages
    }

    /// 计算总 token 数
    ///
    /// 包括所有消息和压缩摘要的 token 数量。
    pub fn total_tokens(&self) -> usize {
        let messages_tokens: usize = self
            .messages
            .iter()
            .map(|m| self.token_counter.count(&m.content))
            .sum();

        let summaries_tokens: usize = self
            .compacted_summaries
            .iter()
            .map(|s| self.token_counter.count(&s.summary))
            .sum();

        messages_tokens + summaries_tokens
    }

    /// 计算消息的 token 数（不包括压缩摘要）
    pub fn message_tokens(&self) -> usize {
        self.messages
            .iter()
            .map(|m| self.token_counter.count(&m.content))
            .sum()
    }

    /// 压缩历史
    ///
    /// 保留最近 `keep_recent` 条消息，其余部分被摘要替换。
    pub fn compact(&mut self, keep_recent: usize, summary: String) {
        if self.messages.len() > keep_recent {
            let original_count = self.message_tokens();

            self.messages = self.messages.split_off(self.messages.len() - keep_recent);

            self.compacted_summaries.push(CompactedSummary {
                turn_id: uuid::Uuid::new_v4().to_string(),
                summary,
                original_token_count: original_count,
                timestamp: chrono::Utc::now().timestamp(),
            });
        }
    }

    /// 获取用于模型提示的消息
    ///
    /// 将压缩摘要作为系统消息插入，然后返回所有消息。
    pub fn for_prompt(&self) -> Vec<Message> {
        let mut result = Vec::new();

        // 添加压缩摘要作为系统消息
        for summary in &self.compacted_summaries {
            result.push(Message::system(format!(
                "[Previous conversation summary: {}]",
                summary.summary
            )));
        }

        result.extend(self.messages.iter().cloned());
        result
    }

    /// 获取压缩摘要列表
    pub fn summaries(&self) -> &[CompactedSummary] {
        &self.compacted_summaries
    }

    /// 清空所有历史
    pub fn clear(&mut self) {
        self.messages.clear();
        self.compacted_summaries.clear();
    }

    /// 获取消息数量
    pub fn len(&self) -> usize {
        self.messages.len()
    }

    /// 检查是否为空
    pub fn is_empty(&self) -> bool {
        self.messages.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::MessageRole;

    #[test]
    fn test_history_compact() {
        let mut history = ConversationHistory::new();

        // 添加 20 条消息
        for i in 0..20 {
            history.push(Message {
                role: MessageRole::User,
                content: format!("Message {}", i),
            });
        }

        assert_eq!(history.len(), 20);

        // 压缩到保留最近 5 条
        history.compact(5, "Summary of messages 0-14".to_string());

        assert_eq!(history.len(), 5);
        assert_eq!(history.summaries().len(), 1);
        assert_eq!(history.summaries()[0].summary, "Summary of messages 0-14");
    }

    #[test]
    fn test_for_prompt() {
        let mut history = ConversationHistory::new();

        for i in 0..10 {
            history.push(Message {
                role: MessageRole::User,
                content: format!("Message {}", i),
            });
        }

        // 压缩
        history.compact(3, "Summary".to_string());

        // 获取用于提示的消息
        let prompt_messages = history.for_prompt();

        // 应该有 1 条摘要 + 3 条消息 = 4 条
        assert_eq!(prompt_messages.len(), 4);
        assert!(prompt_messages[0].content.contains("Summary"));
    }

    #[test]
    fn test_total_tokens() {
        let history = ConversationHistory::new();
        assert_eq!(history.total_tokens(), 0);

        let mut history = ConversationHistory::new();
        history.push(Message::user("hello world"));

        // 近似计数: "hello world" 约 3 tokens
        assert!(history.total_tokens() > 0);
    }
}
