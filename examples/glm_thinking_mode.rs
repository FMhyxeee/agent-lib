//! GLM Coding Plan 思考模式示例
//!
//! 这个示例展示了如何使用 GLM 的思考模式(Thinking Mode)功能,
//! 包括交错式思考(Interleaved Thinking)和保留式思考(Preserved Thinking)。
//!
//! # 运行
//! ```bash
//! set GLM_API_KEY=your_api_key
//! cargo run --example glm_thinking_mode
//! ```

use agent_lib::model::{Message, ModelClient};
use agent_lib::model::provider::GlmCodingPlanProvider;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 从环境变量获取 API Key
    dotenv::dotenv().ok();
    let api_key = std::env::var("GLM_API_KEY").expect("GLM_API_KEY not set");

    println!("🚀 GLM Coding Plan 思考模式示例\n");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n");

    // 示例 1: 默认思考模式 (启用 + 保留式思考)
    println!("📌 示例 1: 默认思考模式 (启用 + 保留式思考)");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n");

    let provider_enabled = GlmCodingPlanProvider::new("glm-4.7", &api_key);

    let messages = vec![
        Message::system("你是一个专业的编程助手。"),
        Message::user("请解释什么是 Rust 的所有权系统?"),
    ];

    match provider_enabled.chat(messages, vec![]).await {
        Ok(response) => {
            println!("✅ 响应成功!\n");

            // 显示推理内容
            if let Some(reasoning) = &response.reasoning_content {
                println!("🧠 推理内容 (Thinking Content):");
                println!("{}\n", reasoning);
                println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n");
            }

            // 显示最终回答
            println!("💡 最终回答:");
            println!("{}\n", response.content);

            // 显示 token 使用情况
            println!("📊 Token 使用:");
            println!("  - 输入: {}", response.usage.prompt_tokens);
            println!("  - 输出: {}", response.usage.completion_tokens);
            println!("  - 总计: {}", response.usage.total_tokens);
        }
        Err(e) => {
            println!("❌ 错误: {}\n", e);
        }
    }

    println!("\n{}\n", "━".repeat(50));

    // 示例 2: 多轮对话 - 保留式思考
    println!("📌 示例 2: 多轮对话 - 保留式思考");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n");

    let provider_preserved = GlmCodingPlanProvider::new("glm-4.7", &api_key)
        .with_preserved_thinking(false); // 启用保留式思考

    let mut messages = vec![
        Message::system("你是一个专业的编程助手。"),
        Message::user("我想写一个简单的计数器。"),
    ];

    // 第一轮对话
    println!("👤 用户: 我想写一个简单的计数器\n");

    match provider_preserved.chat(messages.clone(), vec![]).await {
        Ok(response) => {
            // 保存推理内容用于下一轮
            let reasoning = response.reasoning_content.clone();

            println!("🧠 推理过程:\n{}\n", reasoning.as_ref().unwrap_or(&"无".to_string()));
            println!("🤖 助手:\n{}\n", response.content);

            // 将助手消息添加到历史 (包含推理内容)
            messages.push(Message::assistant_with_reasoning(
                response.content.clone(),
                reasoning.unwrap_or_default(),
            ));

            // 第二轮对话
            println!("👤 用户: 现在我想让它支持递减功能\n");

            messages.push(Message::user("现在我想让它支持递减功能"));

            match provider_preserved.chat(messages.clone(), vec![]).await {
                Ok(response2) => {
                    println!("🧠 推理过程:\n{}\n", response2.reasoning_content.as_ref().unwrap_or(&"无".to_string()));
                    println!("🤖 助手:\n{}\n", response2.content);
                }
                Err(e) => println!("❌ 错误: {}\n", e),
            }
        }
        Err(e) => println!("❌ 错误: {}\n", e),
    }

    println!("\n{}\n", "━".repeat(50));

    // 示例 3: 禁用思考模式
    println!("📌 示例 3: 禁用思考模式");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n");

    let provider_disabled = GlmCodingPlanProvider::new("glm-4.7", &api_key)
        .with_thinking_enabled(false); // 禁用思考模式

    let messages = vec![
        Message::system("你是一个专业的编程助手。"),
        Message::user("请解释什么是 Rust 的所有权系统?"),
    ];

    println!("👤 用户: 请解释什么是 Rust 的所有权系统?\n");

    match provider_disabled.chat(messages, vec![]).await {
        Ok(response) => {
            println!("🤖 助手:\n{}\n", response.content);

            if response.reasoning_content.is_some() {
                println!("⚠️ 注意: 禁用思考模式下仍然返回了推理内容");
            } else {
                println!("✅ 确认: 禁用思考模式下没有返回推理内容");
            }

            println!("\n📊 Token 使用:");
            println!("  - 输入: {}", response.usage.prompt_tokens);
            println!("  - 输出: {}", response.usage.completion_tokens);
            println!("  - 总计: {}", response.usage.total_tokens);
        }
        Err(e) => {
            println!("❌ 错误: {}\n", e);
        }
    }

    println!("\n{}\n", "━".repeat(50));

    // 示例 4: 轮级思考控制 (Turn-level Thinking)
    println!("📌 示例 4: 轮级思考控制 (Turn-level Thinking)");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n");
    println!("在复杂任务中启用思考，在简单任务中关闭思考，以平衡性能和准确性。\n");

    let provider = GlmCodingPlanProvider::new("glm-4.7", &api_key);

    let mut messages = vec![
        Message::system("你是一个专业的编程助手。"),
    ];

    // 简单问题 - 禁用思考
    println!("👤 用户 (简单问题): 今天天气怎么样? (禁用思考)\n");

    let provider_no_thinking = provider.clone().with_thinking_enabled(false);
    messages.push(Message::user("今天天气怎么样?"));

    match provider_no_thinking.chat(messages.clone(), vec![]).await {
        Ok(response) => {
            messages.push(Message::assistant(response.content.clone()));
            println!("🤖 助手: {}\n", response.content);
            println!("⚡ 快速响应 - 无需深度思考\n");
        }
        Err(e) => println!("❌ 错误: {}\n", e),
    }

    // 复杂问题 - 启用思考
    println!("👤 用户 (复杂问题): 请帮我设计一个高性能的 RESTful API 架构 (启用思考)\n");

    let provider_with_thinking = provider.clone().with_thinking_enabled(true);
    messages.push(Message::user("请帮我设计一个高性能的 RESTful API 架构"));

    match provider_with_thinking.chat(messages.clone(), vec![]).await {
        Ok(response) => {
            if let Some(reasoning) = &response.reasoning_content {
                println!("🧠 推理过程:\n{}\n", reasoning);
            }
            println!("🤖 助手:\n{}\n", response.content);
            println!("🎯 深度思考 - 提供更准确的架构建议\n");
        }
        Err(e) => println!("❌ 错误: {}\n", e),
    }

    println!("{}\n", "━".repeat(50));
    println!("✨ 所有示例完成!");
    println!("\n💡 关键要点:");
    println!("  1. 默认情况下 GLM-4.7/5 启用思考模式");
    println!("  2. reasoning_content 包含模型的推理过程");
    println!("  3. 多轮对话中需要返回 reasoning_content 以保持推理连贯性");
    println!("  4. 可以通过 with_thinking_enabled() 和 with_preserved_thinking() 控制思考行为");
    println!("  5. 轮级思考控制让你灵活平衡性能和准确性");

    Ok(())
}
