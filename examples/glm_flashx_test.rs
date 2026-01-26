// GLM-4.7-FlashX 模型测试

use agent_lib::model::{get_context_window, get_model_config, is_model_supported, list_models};
use agent_lib::model::provider::GlmProvider;
use agent_lib::session::Session;
use agent_lib::protocol::{Op, UserInputItem};

/// 从 .env 文件读取环境变量
fn read_env_var(file: &str, key: &str) -> Result<String, Box<dyn std::error::Error>> {
    let content = std::fs::read_to_string(file)?;
    for line in content.lines() {
        if let Some((k, v)) = line.split_once('=') {
            if k.trim() == key {
                return Ok(v.trim().to_string());
            }
        }
    }
    Err(format!("{} not found in {}", key, file).into())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 1. 测试固定模型配置
    println!("=== 测试固定模型配置 ===\n");

    println!("支持的模型列表:");
    for model in list_models() {
        println!("  - {} ({}): {}K context window",
            model.display_name, model.id, model.context_window / 1000
        );
    }

    // 2. 测试 GLM-4.7-FlashX 配置
    println!("\n=== GLM-4.7-FlashX 配置 ===\n");

    let model_id = "glm-4.7-flashx";
    println!("模型: {}", model_id);
    println!("支持: {}", is_model_supported(model_id));

    if let Some(config) = get_model_config(model_id) {
        println!("显示名称: {}", config.display_name);
        println!("提供商: {}", config.provider);
        println!("上下文窗口: {}K tokens", config.context_window / 1000);
        println!("支持流式: {}", config.supports_streaming);
        println!("支持工具: {}", config.supports_tools);
    }

    println!("\n上下文窗口: {}K tokens", get_context_window(model_id) / 1000);

    // 3. 从环境变量读取 API Key (或从 .env 文件)
    let api_key = std::env::var("GLM_API_KEY")
        .or_else(|_| read_env_var(".env", "GLM_API_KEY"))
        .expect("GLM_API_KEY not set in environment or .env file");
    let base_url = std::env::var("GLM_BASE_URL")
        .or_else(|_| read_env_var(".env", "GLM_BASE_URL"))
        .unwrap_or_else(|_| "https://open.bigmodel.cn/api/paas/v4/chat/completions".to_string());

    println!("\n=== GLM API 配置 ===\n");
    println!("Base URL: {}", base_url);
    println!("API Key: {}***", &api_key[..8]);

    // 4. 创建 Provider (GlmProvider::new 参数顺序: model, api_key)
    let provider = GlmProvider::new(model_id, api_key)
        .with_base_url(base_url);

    // 5. 创建 Session
    println!("\n=== 创建 Session ===\n");
    let (session, handle) = Session::with_config(
        64,
        agent_lib::session::SessionConfig {
            model: Some(std::sync::Arc::new(provider) as std::sync::Arc<dyn agent_lib::model::ModelClient>),
            ..Default::default()
        },
    );

    // 6. 发送测试消息
    println!("=== 发送测试消息 ===\n");

    let prompt = UserInputItem::text("你好！请用一句话介绍一下你自己。");

    handle.submit(Op::UserTurn {
        items: vec![prompt],
        cwd: std::path::PathBuf::from("."),
        approval_policy: agent_lib::protocol::ApprovalPolicy::NeverAsk,
        sandbox_policy: agent_lib::protocol::SandboxPolicy::Persistent,
        model: model_id.to_string(),
        effort: None,
        summary: agent_lib::protocol::ReasoningSummary {
            summary: String::new(),
            token_count: 0,
        },
        final_output_json_schema: None,
        collaboration_mode: None,
    }).await?;

    println!("消息已发送，等待响应...\n");

    // 7. 接收响应
    println!("=== 模型响应 ===\n");
    let mut full_response = String::new();
    let timeout_secs = 60;

    use tokio::time::{sleep, Duration};
    let start_time = std::time::Instant::now();

    while start_time.elapsed() < Duration::from_secs(timeout_secs) {
        match tokio::time::timeout(Duration::from_secs(1), handle.next_event()).await {
            Ok(Some(event)) => {
                println!("[DEBUG] Event: {:?}", std::mem::discriminant(&event));
                match event {
                    agent_lib::protocol::Event::ModelStreaming { chunk } => {
                        print!("{}", chunk);
                        full_response.push_str(&chunk);
                    }
                    agent_lib::protocol::Event::ModelComplete { content, .. } => {
                        println!("\n[Content]: {}", content);
                        println!("=== 完成 ===");
                        break;
                    }
                    agent_lib::protocol::Event::Error { error } => {
                        println!("\n错误: {:?}", error);
                        return Err(format!("Model error: {:?}", error).into());
                    }
                    agent_lib::protocol::Event::TurnStarted { .. } => {
                        println!("Turn started...");
                    }
                    agent_lib::protocol::Event::Warning { message } => {
                        println!("Warning: {}", message);
                    }
                    _ => {
                        println!("Other event: {:?}", event);
                    }
                }
            }
            Err(_) => {
                // 超时继续循环
            }
            Ok(None) => {
                sleep(Duration::from_millis(100)).await;
            }
        }
    }

    println!("\n响应长度: {} 字符", full_response.chars().count());
    println!("响应字节: {} bytes", full_response.len());

    // 8. 测试 TurnContext 的 context_window
    println!("\n=== TurnContext Context Window ===\n");
    let ctx = agent_lib::session::TurnContext::new(model_id);
    println!("TurnContext context_window: {}", ctx.context_window);

    Ok(())
}
