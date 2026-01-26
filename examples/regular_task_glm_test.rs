//! RegularTask 端到端测试示例
//!
//! 使用 GLM 模型测试 RegularTask 的完整功能。
//! 需要设置环境变量:
//!   - GLM_BASE_URL: GLM API 端点
//!   - GLM_API_KEY: GLM API 密钥
//!
//! 运行方式:
//!   cargo run --example regular_task_glm_test

use std::env;
use std::sync::Arc;

use agent_lib::model::Message;
use agent_lib::model::provider::GlmProvider;
use agent_lib::protocol::Event;
use agent_lib::session::{ConversationHistory, TaskSession, TurnContext};
use agent_lib::tasks::{RegularTask, SessionTask};
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;

// 从 .env 文件读取配置的辅助函数
fn load_config() -> (String, String) {
    // 尝试从 .env 文件读取
    if let Ok(content) = std::fs::read_to_string(".env") {
        let mut api_key = None;
        let mut base_url = None;

        for line in content.lines() {
            if let Some((key, value)) = line.split_once('=') {
                match key.trim() {
                    "GLM_API_KEY" => api_key = Some(value.trim().to_string()),
                    "GLM_BASE_URL" => base_url = Some(value.trim().to_string()),
                    _ => {}
                }
            }
        }

        if let (Some(key), Some(url)) = (api_key, base_url) {
            return (key, url);
        }
    }

    // 回退到环境变量
    let api_key = env::var("GLM_API_KEY").unwrap_or_else(|_| {
        eprintln!("⚠️  请在 .env 文件中设置 GLM_API_KEY 或设置环境变量");
        eprintln!("   GLM_API_KEY=your-api-key");
        std::process::exit(1);
    });

    let base_url = env::var("GLM_BASE_URL")
        .unwrap_or_else(|_| "https://open.bigmodel.cn/api/paas/v4/chat/completions".to_string());

    (api_key, base_url)
}

// 简单的 Mock TaskSession，用于测试
struct MockTaskSession {
    history: Arc<Mutex<ConversationHistory>>,
    event_sender: tokio::sync::mpsc::Sender<Event>,
    model: Option<Arc<dyn agent_lib::model::ModelClient>>,
}

#[async_trait::async_trait]
impl TaskSession for MockTaskSession {
    async fn history(&self) -> ConversationHistory {
        self.history.lock().await.clone()
    }

    async fn compact_history(&self, keep_recent: usize, summary: String) {
        let mut history = self.history.lock().await;
        history.compact(keep_recent, summary);
    }

    async fn emit_event(&self, event: Event) {
        let _ = self.event_sender.send(event).await;
    }

    async fn undo_last_messages(&self, num_messages: usize) {
        let mut history = self.history.lock().await;
        if num_messages > 0 && history.len() > num_messages {
            let new_messages: Vec<agent_lib::model::Message> = history
                .all()
                .iter()
                .take(history.len() - num_messages)
                .cloned()
                .collect();
            history.clear();
            for msg in new_messages {
                history.push(msg);
            }
        }
    }

    async fn chat_model(
        &self,
        messages: Vec<Message>,
        tools: Vec<agent_lib::tools::ToolDef>,
    ) -> agent_lib::error::AgentResult<agent_lib::model::ModelResponse> {
        if let Some(model) = &self.model {
            model.chat(messages, tools).await
        } else {
            Err(agent_lib::error::AgentError::NotImplemented(
                "model not configured".to_string(),
            ))
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== RegularTask 端到端测试 (GLM) ===\n");

    // 加载配置
    let (api_key, base_url) = load_config();

    println!("📋 配置信息:");
    println!("   Base URL: {}", base_url);
    println!("   Model: glm-4-plus\n");

    // 创建 GLM Provider
    let glm_provider = Arc::new(GlmProvider::new("glm-4-plus", api_key).with_base_url(base_url));

    // 创建事件通道
    let (event_sender, mut event_receiver) = tokio::sync::mpsc::channel(64);

    // 准备测试对话历史
    let mut history = ConversationHistory::new();
    history.push(Message::system(
        "你是一个友好的助手，用简洁的语言回答问题。",
    ));
    history.push(Message::user("什么是 Rust 编程语言？请用一句话回答。"));

    println!("📝 测试输入:");
    println!("   System: 你是一个友好的助手，用简洁的语言回答问题。");
    println!("   User: 什么是 Rust 编程语言？请用一句话回答。\n");

    // 创建 TaskSession
    let task_session: Arc<dyn TaskSession> = Arc::new(MockTaskSession {
        history: Arc::new(Mutex::new(history)),
        event_sender: event_sender.clone(),
        model: Some(glm_provider),
    });

    // 创建 TurnContext
    let ctx = Arc::new(TurnContext {
        sub_id: "test-turn-001".to_string(),
        model: "glm-4-plus".to_string(),
        context_window: 128000,
        ..Default::default()
    });

    // 创建并运行 RegularTask
    println!("▶️  开始执行 RegularTask\n");

    let task = Arc::new(RegularTask);
    let token = CancellationToken::new();

    // 在后台监听事件流
    let event_handle = tokio::spawn(async move {
        let mut full_response = String::new();
        let mut response_started = false;

        while let Some(event) = event_receiver.recv().await {
            match event {
                Event::ModelStreaming { chunk } => {
                    if !response_started {
                        print!("🤖 响应: ");
                        response_started = true;
                    }
                    print!("{}", chunk);
                    full_response.push_str(&chunk);
                }
                Event::ModelComplete { content, usage } => {
                    if !response_started {
                        print!("🤖 响应: ");
                    }
                    println!("\n\n✅ 完成");
                    println!("   Token 使用: {:?}", usage);
                    println!("   响应长度: {} 字符", content.len());
                }
                Event::Error { error } => {
                    println!("\n❌ 错误: {:?}", error);
                }
                Event::Warning { message } => {
                    println!("⚠️  警告: {}", message);
                }
                Event::TurnAborted { reason } => {
                    println!("🛑 中止: {:?}", reason);
                }
                Event::ContextCompacted { .. } => {
                    println!("📦 历史已压缩");
                }
                _ => {}
            }
        }

        full_response
    });

    // 运行任务
    let result = task.run(task_session, ctx, token).await;

    // 等待事件流处理完成
    let _ = tokio::time::timeout(tokio::time::Duration::from_secs(30), event_handle).await;

    println!("\n\n=== 测试完成 ===");
    if let Some(msg) = result {
        println!("📊 任务结果: {}", msg);
    }

    Ok(())
}
