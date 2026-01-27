use serde::{Deserialize, Serialize};
use std::cell::Cell;

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

    // Token 缓存字段
    #[serde(skip)]
    cached_token_count: Cell<usize>,
    #[serde(skip)]
    cache_dirty: Cell<bool>,
}

impl ConversationHistory {
    pub fn new() -> Self {
        Self {
            messages: Vec::new(),
            compacted_summaries: Vec::new(),
            token_counter: TokenCounter::new(),
            cached_token_count: Cell::new(0),
            cache_dirty: Cell::new(false),
        }
    }

    /// 使用指定的 Token 计数器创建历史
    pub fn with_token_counter(token_counter: TokenCounter) -> Self {
        Self {
            messages: Vec::new(),
            compacted_summaries: Vec::new(),
            token_counter,
            cached_token_count: Cell::new(0),
            cache_dirty: Cell::new(false),
        }
    }

    pub fn push(&mut self, message: Message) {
        // 增量更新缓存
        let added_tokens = self.token_counter.count(&message.content);
        if !self.cache_dirty.get() {
            let current = self.cached_token_count.get();
            self.cached_token_count.set(current + added_tokens);
        }
        self.messages.push(message);
    }

    pub fn all(&self) -> &[Message] {
        &self.messages
    }

    /// 计算总 token 数
    ///
    /// 包括所有消息和压缩摘要的 token 数量。
    /// 使用缓存机制，只在必要时重新计算。
    pub fn total_tokens(&self) -> usize {
        if self.cache_dirty.get() {
            // 缓存失效，重新计算并更新缓存
            let count = self.recalculate_tokens();
            self.cached_token_count.set(count);
            self.cache_dirty.set(false);
            count
        } else {
            // 使用缓存值
            self.cached_token_count.get()
        }
    }

    /// 重新计算所有 token 数量（强制重新计算）
    ///
    /// 这个方法会忽略缓存，强制重新计算所有消息和摘要的 token 数量。
    /// 主要用于调试或验证缓存正确性。
    pub fn recalculate_tokens(&self) -> usize {
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

            // 标记缓存失效
            self.cache_dirty.set(true);
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
    /// 获取消息数量
    pub fn len(&self) -> usize {
        self.messages.len()
    }

    /// 检查是否为空
    pub fn is_empty(&self) -> bool {
        self.messages.is_empty()
    }

    /// 清空所有历史
    pub fn clear(&mut self) {
        self.messages.clear();
        self.compacted_summaries.clear();
        // 清空时重置缓存
        self.cached_token_count.set(0);
        self.cache_dirty.set(false);
    }

    /// 手动标记缓存失效
    ///
    /// 强制下一次调用 `total_tokens()` 时重新计算所有 token 数量。
    pub fn invalidate_cache(&mut self) {
        self.cache_dirty.set(true);
    }

    /// 检查缓存是否有效
    ///
    /// # 返回
    /// 如果缓存有效（即与当前内容同步）返回 true，否则返回 false。
    pub fn is_cache_valid(&self) -> bool {
        !self.cache_dirty.get()
    }

    /// 获取缓存的 token 数量
    ///
    /// 注意：这个方法返回的是缓存的值，不保证是最新的。
    /// 如果需要最新值，请使用 `total_tokens()`。
    ///
    /// # 返回
    /// 缓存的 token 数量
    pub fn cached_tokens(&self) -> usize {
        self.cached_token_count.get()
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
                tool_call_id: None,
                tool_calls: None,
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
                tool_call_id: None,
                tool_calls: None,
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

    #[test]
    fn test_cache_incremental_update() {
        let mut history = ConversationHistory::new();

        // 初始状态
        assert_eq!(history.total_tokens(), 0);
        assert!(history.is_cache_valid());

        // 添加第一条消息
        history.push(Message::user("hello"));
        let count_after_hello = history.total_tokens();
        assert!(count_after_hello > 0);

        // 添加第二条消息，缓存应该是增量更新的
        history.push(Message::user("world"));
        let count_after_world = history.total_tokens();
        assert!(count_after_world > count_after_hello);

        // 缓存应该仍然有效
        assert!(history.is_cache_valid());
    }

    #[test]
    fn test_cache_invalidation_on_compact() {
        let mut history = ConversationHistory::new();

        // 添加多条长消息
        history.push(Message::user("This is a very long message that should definitely use more tokens than a short summary."));
        history.push(Message::user("Another long message with lots of content that will be compressed into a much shorter summary."));
        let before_compact = history.total_tokens();

        // 压缩会标记缓存失效
        history.compact(1, "Summary of previous long messages.".to_string());

        // 缓存应该失效
        assert!(!history.is_cache_valid());

        // 再次计算，应该返回正确的结果（压缩后应该明显减少）
        let after_compact = history.total_tokens();
        assert!(
            after_compact < before_compact,
            "After: {}, Before: {}",
            after_compact,
            before_compact
        );
    }

    #[test]
    fn test_cache_consistency() {
        let mut history = ConversationHistory::new();

        // 添加消息
        history.push(Message::user("test message"));

        // 缓存有效时多次调用应该返回相同结果
        let count1 = history.total_tokens();
        let count2 = history.total_tokens();
        let count3 = history.total_tokens();

        assert_eq!(count1, count2);
        assert_eq!(count2, count3);
    }

    #[test]
    fn test_manual_cache_invalidation() {
        let mut history = ConversationHistory::new();

        history.push(Message::user("hello"));
        let count1 = history.total_tokens();

        // 手动失效缓存
        history.invalidate_cache();
        assert!(!history.is_cache_valid());

        // 缓存失效后调用total_tokens应该重新计算
        let count2 = history.total_tokens();
        assert_eq!(count1, count2); // 数值应该相同
        assert!(history.is_cache_valid()); // 缓存应该重新有效
    }

    #[test]
    fn test_clear_resets_cache() {
        let mut history = ConversationHistory::new();

        history.push(Message::user("hello"));
        assert!(history.total_tokens() > 0);

        history.clear();
        assert_eq!(history.total_tokens(), 0);
        assert!(history.is_cache_valid());
        assert_eq!(history.cached_tokens(), 0);
    }

    #[test]
    fn test_cached_tokens() {
        let mut history = ConversationHistory::new();

        history.push(Message::user("hello"));
        let expected = history.total_tokens();

        // cached_tokens()应该返回缓存的值
        assert_eq!(history.cached_tokens(), expected);

        // 手动失效缓存后，cached_tokens不应该立即改变
        history.invalidate_cache();
        assert_eq!(history.cached_tokens(), expected);
        assert!(!history.is_cache_valid());
    }
}
