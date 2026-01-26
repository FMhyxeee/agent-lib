#[cfg(test)]
mod large_scale_tests {
    use agent_lib::model::Message;
    use agent_lib::session::ConversationHistory;
    use agent_lib::token::{TokenCounter, TruncationPolicy};

    #[test]
    fn test_large_history_push_performance() {
        let mut history = ConversationHistory::new();
        let start = std::time::Instant::now();

        for i in 0..10_000 {
            history.push(Message::user(format!("Message {}", i)));
        }

        let elapsed = start.elapsed();
        assert!(
            elapsed.as_millis() < 1000,
            "Push 10,000 messages should be fast"
        );
        assert_eq!(history.len(), 10_000);
    }

    #[test]
    fn test_large_history_token_count_consistency() {
        let mut history = ConversationHistory::new();

        for i in 0..1000 {
            history.push(Message::user(format!("Message {}", i)));
        }

        // 多次调用应该返回相同结果
        let count1 = history.total_tokens();
        let count2 = history.total_tokens();
        let count3 = history.total_tokens();

        assert_eq!(count1, count2);
        assert_eq!(count2, count3);
        assert!(count1 > 0);
    }

    #[test]
    fn test_cache_invalidation_after_compact() {
        let mut history = ConversationHistory::new();

        for i in 0..100 {
            history.push(Message::user(format!("Message {}", i)));
        }

        let before_compact = history.total_tokens();
        history.compact(50, "Summary of first 50 messages".to_string());
        let after_compact = history.total_tokens();

        assert!(after_compact < before_compact);
        assert_eq!(history.len(), 50);
        assert_eq!(history.summaries().len(), 1);
    }

    #[test]
    fn test_incremental_cache_updates() {
        let mut history = ConversationHistory::new();

        // 初始状态
        assert_eq!(history.total_tokens(), 0);
        assert!(history.is_cache_valid());

        // 批量添加消息，缓存应该增量更新
        for i in 0..100 {
            history.push(Message::user(format!("Message {}", i)));

            // 每次添加后缓存应该有效
            assert!(history.is_cache_valid());

            // 多次调用total_tokens应该返回相同结果
            let count1 = history.total_tokens();
            let count2 = history.total_tokens();
            assert_eq!(count1, count2);
        }

        let final_count = history.total_tokens();
        assert!(final_count > 0);
    }

    #[test]
    fn test_multiple_compactions() {
        let mut history = ConversationHistory::new();

        // 添加大量消息
        for i in 0..500 {
            history.push(Message::user(format!("Message {}", i)));
        }

        let count_after_500 = history.total_tokens();

        // 第一次压缩
        history.compact(100, "Summary of messages 0-400".to_string());
        assert_eq!(history.len(), 100);
        assert_eq!(history.summaries().len(), 1);

        // 添加更多消息
        for i in 500..600 {
            history.push(Message::user(format!("Message {}", i)));
        }

        let count_after_adding = history.total_tokens();
        assert!(count_after_adding > count_after_500 / 5); // 应该显著减少

        // 第二次压缩
        history.compact(50, "Summary of messages 400-550".to_string());
        assert_eq!(history.len(), 50);
        assert_eq!(history.summaries().len(), 2);

        let final_count = history.total_tokens();
        assert!(final_count < count_after_adding);
    }

    #[test]
    fn test_truncation_policy_for_model_large_range() {
        let models = vec![
            ("gpt-4", 8192),
            ("gpt-4-turbo", 128000),
            ("gpt-3.5-turbo", 4097),
            ("claude-2", 100000),
            ("claude-3-haiku", 100000),
            ("claude-3-sonnet", 200000),
            ("gemini-pro", 30720),
            ("gemini-pro-1.5", 2097152),
            ("unknown-model", 128000), // 默认值
        ];

        for (model_name, expected_limit) in models {
            let policy = TruncationPolicy::for_model(model_name);
            assert_eq!(
                policy.token_budget(),
                expected_limit,
                "Model {} should have expected limit",
                model_name
            );
        }
    }

    #[test]
    fn test_truncation_policy_exceeds_large_numbers() {
        let policy = TruncationPolicy::tokens(100000);

        assert!(!policy.exceeds(50000));
        assert!(!policy.exceeds(100000));
        assert!(policy.exceeds(100001));
        assert!(policy.exceeds(200000));
        assert!(policy.exceeds(1_000_000));
    }

    #[test]
    fn test_truncation_policy_with_reserve_edge_cases() {
        // 测试边界情况
        let policy = TruncationPolicy::with_reserve(10000, 0); // 不保留
        assert_eq!(policy.token_budget(), 10000);

        let policy = TruncationPolicy::with_reserve(10000, 100); // 保留100%
        assert_eq!(policy.token_budget(), 0);

        let policy = TruncationPolicy::with_reserve(10000, 50); // 保留50%
        assert_eq!(policy.token_budget(), 5000);

        let policy = TruncationPolicy::with_reserve(10000, 75); // 保留75%
        assert_eq!(policy.token_budget(), 2500);
    }

    #[test]
    fn test_truncation_policy_remaining_large_values() {
        let policy = TruncationPolicy::tokens(100000);

        assert_eq!(policy.remaining(0), 100000);
        assert_eq!(policy.remaining(50000), 50000);
        assert_eq!(policy.remaining(100000), 0);
        assert_eq!(policy.remaining(150000), -50000);
        assert_eq!(policy.remaining(1_000_000), -900000);
    }

    #[test]
    fn test_unicode_content_large() {
        let mut history = ConversationHistory::new();

        // 添加大量Unicode内容
        for i in 0..100 {
            history.push(Message::user(format!(
                "你好世界 {} 🌍 Hello World {} 🚀",
                i, i
            )));
            history.push(Message::assistant(format!("回复 {} Response {} ✨", i, i)));
        }

        let count = history.total_tokens();
        assert!(count > 0);

        // 确保缓存一致性
        let count2 = history.total_tokens();
        assert_eq!(count, count2);

        // 压缩后应该仍然正确
        history.compact(50, "Unicode summary".to_string());
        let count_after = history.total_tokens();
        assert!(count_after > 0);
        assert!(count_after < count);
    }

    #[test]
    fn test_token_counter_batch_large() {
        let counter = TokenCounter::new();
        let texts: Vec<String> = (0..1000)
            .map(|i| format!("Test message number {}", i))
            .collect();
        let texts_refs: Vec<&str> = texts.iter().map(|s| s.as_str()).collect();

        let count = counter.count_batch(&texts_refs);
        assert!(count > 0);

        // 验证批量计数等于逐个计数
        let individual_count: usize = texts_refs.iter().map(|t| counter.count(t)).sum();
        assert_eq!(count, individual_count);
    }

    #[test]
    fn test_estimate_message_capacity_various() {
        let policy = TruncationPolicy::tokens(10000);

        // 测试不同的平均消息长度
        assert_eq!(policy.estimate_message_capacity(100), 100); // 10000 / 100 = 100
        assert_eq!(policy.estimate_message_capacity(200), 50); // 10000 / 200 = 50
        assert_eq!(policy.estimate_message_capacity(500), 20); // 10000 / 500 = 20
        assert_eq!(policy.estimate_message_capacity(1000), 10); // 10000 / 1000 = 10
        assert_eq!(policy.estimate_message_capacity(0), 0); // 避免除零
        assert_eq!(policy.estimate_message_capacity(15000), 0); // 10000 / 15000 = 0
    }

    #[test]
    fn test_history_clear_performance() {
        let mut history = ConversationHistory::new();

        // 添加大量消息
        for i in 0..1000 {
            history.push(Message::user(format!("Message {}", i)));
        }

        let count_before = history.total_tokens();
        assert!(count_before > 0);

        let start = std::time::Instant::now();
        history.clear();
        let elapsed = start.elapsed();

        assert_eq!(history.len(), 0);
        assert_eq!(history.total_tokens(), 0);
        assert!(history.is_cache_valid());
        assert!(elapsed.as_millis() < 10, "Clear should be very fast");
    }
}
