//! Token Counting Demo
//!
//! 演示近似计数与精确 tiktoken 计数的差异

use agent_lib::token::{approx_token_count, tiktoken_count, TokenCounter};

fn main() {
    let texts = [
        "hello world",
        "你好，世界",
        "The quick brown fox jumps over the lazy dog.",
        "这是一个测试文本，用于验证 token 计数的准确性。",
        "Function calculate_sum(a: i32, b: i32) -> i32 { a + b }",
    ];

    println!("=== Token Counting Comparison ===\n");

    for text in texts {
        let approx = approx_token_count(text);
        let precise = tiktoken_count(text);
        let diff = if precise > approx {
            precise as i32 - approx as i32
        } else {
            approx as i32 - precise as i32
        };
        let error_pct = (diff as f64 / precise as f64) * 100.0;

        println!("Text: \"{}\"", text);
        println!("  Approx:    {} tokens", approx);
        println!("  Tiktoken:  {} tokens", precise);
        println!("  Diff:      {} tokens ({:.1}%)", diff, error_pct);
        println!();
    }

    println!("=== TokenCounter Modes ===");
    let counter_approx = TokenCounter::with_approx();
    let counter_tiktoken = TokenCounter::with_tiktoken();
    let counter_auto = TokenCounter::auto();

    println!("  Approx mode:  {}", counter_approx.mode_name());
    println!("  Tiktoken mode: {}", counter_tiktoken.mode_name());
    println!("  Auto mode:    {}", counter_auto.mode_name());

    // 示例：计算对话历史
    println!("\n=== Conversation Example ===");
    let conversation = vec![
        "User: 你好！",
        "Assistant: 你好！有什么我可以帮助你的吗？",
        "User: 请解释一下 Rust 的所有权机制。",
        "Assistant: Rust 的所有权是一种内存管理机制...",
    ];

    let total_approx: usize = conversation.iter().map(|s| approx_token_count(s)).sum();
    let total_precise: usize = conversation.iter().map(|s| tiktoken_count(s)).sum();

    println!("  Total (approx):  {} tokens", total_approx);
    println!("  Total (precise): {} tokens", total_precise);
    println!("  Difference:      {} tokens", total_precise as i32 - total_approx as i32);
}
