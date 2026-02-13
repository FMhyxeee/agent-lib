//! Token Counting Demo
//!
//! 演示精确的 tiktoken token 计数功能

use agent_lib::token::{TokenCounter, count_tokens};

fn main() {
    let texts = [
        "hello world",
        "你好，世界",
        "The quick brown fox jumps over the lazy dog.",
        "这是一个测试文本，用于验证 token 计数的准确性。",
        "Function calculate_sum(a: i32, b: i32) -> i32 { a + b }",
    ];

    println!("=== Precise Token Counting (cl100k_base BPE) ===\n");

    for text in texts {
        let count = count_tokens(text);
        println!("Text: \"{}\"", text);
        println!("  Tokens: {}\n", count);
    }

    println!("=== TokenCounter API ===");
    let counter = TokenCounter::new();
    println!("  Mode: {}", counter.mode_name());

    // 示例：计算对话历史
    println!("\n=== Conversation Example ===");
    let conversation = vec![
        "User: 你好！",
        "Assistant: 你好！有什么我可以帮助你的吗？",
        "User: 请解释一下 Rust 的所有权机制。",
        "Assistant: Rust 的所有权是一种内存管理机制...",
    ];

    let total: usize = conversation.iter().map(|s| count_tokens(s)).sum();
    println!("  Total tokens: {}", total);

    // 字节估算 vs 实际计数
    println!("\n=== Byte Estimation vs Actual ===");
    let text = "这是一个测试文本，用于比较字节数和实际 token 数的差异。";
    let byte_count = text.len();
    let estimated = counter.estimate_from_bytes(byte_count);
    let actual = count_tokens(text);

    println!("  Text: \"{}\"", text);
    println!("  Bytes: {}", byte_count);
    println!("  Estimated: {} tokens", estimated);
    println!("  Actual: {} tokens", actual);
    println!(
        "  Error: {:.1}%",
        (estimated.abs_diff(actual) as f64 / actual as f64) * 100.0
    );
}
