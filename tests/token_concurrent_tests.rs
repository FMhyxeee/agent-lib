#[cfg(test)]
mod concurrent_tests {
    use agent_lib::model::Message;
    use agent_lib::session::ConversationHistory;
    use agent_lib::token::{TokenCounter, TruncationPolicy};
    use std::sync::Arc;
    use tokio::sync::Mutex;

    #[tokio::test]
    async fn test_concurrent_push_and_read() {
        let history = Arc::new(Mutex::new(ConversationHistory::new()));
        let mut handles = vec![];

        // 100个并发写入任务
        for i in 0..100 {
            let h = Arc::clone(&history);
            handles.push(tokio::spawn(async move {
                let mut hist = h.lock().await;
                hist.push(Message::user(format!("Concurrent message {}", i)));
            }));
        }

        // 50个并发读取任务
        for _ in 0..50 {
            let h = Arc::clone(&history);
            handles.push(tokio::spawn(async move {
                let hist = h.lock().await;
                let _count = hist.total_tokens();
                let _len = hist.len();
            }));
        }

        // 等待所有任务完成
        for handle in handles {
            handle.await.unwrap();
        }

        // 验证最终状态
        let final_history = history.lock().await;
        assert_eq!(final_history.all().len(), 100);
        assert!(final_history.total_tokens() > 0);
        assert!(final_history.is_cache_valid());
    }

    #[tokio::test]
    async fn test_concurrent_compact_and_read() {
        let history = Arc::new(Mutex::new(ConversationHistory::new()));
        let mut handles = vec![];

        // 先添加消息
        {
            let mut hist = history.lock().await;
            for i in 0..1000 {
                hist.push(Message::user(format!("Message {}", i)));
            }
        }

        // 并发压缩和读取
        for i in 0..20 {
            let h = Arc::clone(&history);
            handles.push(tokio::spawn(async move {
                let mut hist = h.lock().await;
                // 每隔几个任务执行一次压缩
                if i % 4 == 0 {
                    hist.compact(500, format!("Compact operation {}", i));
                } else {
                    // 读取操作
                    let _count = hist.total_tokens();
                    let _len = hist.len();
                    let _summaries_count = hist.summaries().len();
                }
            }));
        }

        // 等待所有任务完成
        for handle in handles {
            handle.await.unwrap();
        }

        // 验证最终状态
        let final_history = history.lock().await;
        assert!(final_history.total_tokens() > 0);
        // 可能有多个压缩操作
        assert!(final_history.summaries().len() >= 0);
    }

    #[tokio::test]
    async fn test_concurrent_token_counter() {
        let counter = Arc::new(TokenCounter::auto());
        let mut handles = vec![];
        let results = Arc::new(Mutex::new(Vec::new()));

        // 100个并发计数任务
        for i in 0..100 {
            let c = Arc::clone(&counter);
            let r = Arc::clone(&results);
            handles.push(tokio::spawn(async move {
                let text = format!("Test message {}", i);
                let count = c.count(&text);
                let mut results = r.lock().await;
                results.push(count);
            }));
        }

        // 等待所有任务完成
        for handle in handles {
            handle.await.unwrap();
        }

        // 验证结果
        let final_results = results.lock().await;
        assert_eq!(final_results.len(), 100);
        assert!(final_results.iter().all(|&c| c > 0));

        // 测试批量计数
        let texts: Vec<String> = (0..50).map(|i| format!("Batch test {}", i)).collect();
        let texts_refs: Vec<&str> = texts.iter().map(|s| s.as_str()).collect();
        let batch_count = counter.count_batch(&texts_refs);
        let individual_count: usize = texts_refs.iter().map(|t| counter.count(t)).sum();
        assert_eq!(batch_count, individual_count);
    }

    #[tokio::test]
    async fn test_concurrent_truncation_policy() {
        let policy = Arc::new(TruncationPolicy::tokens(100000));
        let mut handles = vec![];

        // 并发调用策略方法
        for i in 0..50 {
            let p = Arc::clone(&policy);
            handles.push(tokio::spawn(async move {
                let different_counts = vec![1000, 5000, 10000, 20000, 50000];
                for count in different_counts {
                    let exceeds = p.exceeds(count);
                    let remaining = p.remaining(count);
                    let has_enough = p.has_enough(count);

                    // 验证逻辑一致性
                    assert_eq!(exceeds, count > 100000);
                    assert_eq!(has_enough, remaining >= 0);
                    assert_eq!(exceeds, !has_enough);
                }
            }));
        }

        // 等待所有任务完成
        for handle in handles {
            handle.await.unwrap();
        }
    }

    #[tokio::test]
    async fn test_concurrent_cache_operations() {
        let history = Arc::new(Mutex::new(ConversationHistory::new()));
        let mut handles = vec![];

        // 先添加一些消息
        {
            let mut hist = history.lock().await;
            for i in 0..100 {
                hist.push(Message::user(format!("Initial message {}", i)));
            }
        }

        // 并发进行各种操作
        for i in 0..30 {
            let h = Arc::clone(&history);
            handles.push(tokio::spawn(async move {
                let mut hist = h.lock().await;

                match i % 5 {
                    0 => {
                        // 添加消息
                        hist.push(Message::user(format!("Concurrent add {}", i)));
                    }
                    1 => {
                        // 检查缓存
                        let _is_valid = hist.is_cache_valid();
                        let _count = hist.total_tokens();
                    }
                    2 => {
                        // 手动失效缓存
                        hist.invalidate_cache();
                        let _count = hist.total_tokens(); // 这会重新计算
                    }
                    3 => {
                        // 获取缓存值（可能过期）
                        let _cached = hist.cached_tokens();
                    }
                    4 => {
                        // 压缩（较少频率）
                        if hist.len() > 50 {
                            hist.compact(25, format!("Concurrent compact {}", i));
                        }
                    }
                    _ => unreachable!(),
                }
            }));
        }

        // 等待所有任务完成
        for handle in handles {
            handle.await.unwrap();
        }

        // 验证最终状态
        let final_history = history.lock().await;
        assert!(final_history.total_tokens() > 0);
        assert!(final_history.len() > 0);
    }

    #[tokio::test]
    async fn test_concurrent_for_model() {
        let models = vec![
            "gpt-4",
            "gpt-4-turbo",
            "gpt-3.5-turbo",
            "claude-3-haiku",
            "claude-3-sonnet",
            "gemini-pro",
            "unknown-model",
        ];

        let mut handles = vec![];

        for model in models {
            let m = model.to_string();
            handles.push(tokio::spawn(async move {
                let policy = TruncationPolicy::for_model(&m);
                let budget = policy.token_budget();
                assert!(budget > 0, "Model {} should have positive budget", m);

                // 测试各种方法
                assert!(!policy.exceeds(0));
                assert!(policy.has_enough(0));
                assert_eq!(policy.remaining(0), budget as isize);
            }));
        }

        // 等待所有任务完成
        for handle in handles {
            handle.await.unwrap();
        }
    }

    #[tokio::test]
    async fn test_concurrent_history_for_prompt() {
        let history = Arc::new(Mutex::new(ConversationHistory::new()));
        let mut handles = vec![];

        // 添加一些消息和摘要
        {
            let mut hist = history.lock().await;
            for i in 0..20 {
                hist.push(Message::user(format!("Message {}", i)));
            }
            // 添加一个摘要
            hist.compact(10, "Summary of first 10 messages".to_string());
        }

        // 并发获取用于提示的消息
        for _ in 0..10 {
            let h = Arc::clone(&history);
            handles.push(tokio::spawn(async move {
                let hist = h.lock().await;
                let prompt_messages = hist.for_prompt();
                assert!(!prompt_messages.is_empty());

                // 检查是否包含摘要
                let has_summary = prompt_messages
                    .iter()
                    .any(|msg| msg.content.contains("Summary"));
                assert!(has_summary);
            }));
        }

        // 等待所有任务完成
        for handle in handles {
            handle.await.unwrap();
        }
    }

    #[tokio::test]
    async fn test_stress_concurrent_operations() {
        let history = Arc::new(Mutex::new(ConversationHistory::new()));
        let num_operations = 200;

        let start = std::time::Instant::now();
        let mut handles = vec![];

        // 创建大量并发操作
        for i in 0..num_operations {
            let h = Arc::clone(&history);
            handles.push(tokio::spawn(async move {
                let mut hist = h.lock().await;

                // 随机操作
                match i % 3 {
                    0 => {
                        hist.push(Message::user(format!("Stress message {}", i)));
                    }
                    1 => {
                        let _count = hist.total_tokens();
                        let _len = hist.len();
                    }
                    2 => {
                        if hist.len() > 10 {
                            hist.compact(5, format!("Stress compact {}", i));
                        }
                    }
                    _ => unreachable!(),
                }
            }));
        }

        // 等待所有任务完成
        for handle in handles {
            handle.await.unwrap();
        }

        let elapsed = start.elapsed();
        println!(
            "Stress test with {} operations completed in {:?}",
            num_operations, elapsed
        );

        // 验证最终状态
        let final_history = history.lock().await;
        assert!(final_history.total_tokens() > 0);
        assert!(final_history.len() > 0);

        // 性能断言：200个操作应该在合理时间内完成
        assert!(elapsed.as_secs() < 5, "Stress test should complete quickly");
    }
}
