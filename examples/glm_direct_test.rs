// 直接测试 GLM API 调用

use agent_lib::model::ModelClient;
use agent_lib::model::provider::GlmProvider;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 读取 API Key
    let api_key = read_env_var(".env", "GLM_API_KEY")?;
    let base_url = read_env_var(".env", "GLM_BASE_URL")?;

    println!("=== GLM API 直接测试 ===\n");
    println!("API Key: {}***", &api_key[..8]);
    println!("Base URL: {}", base_url);

    // 创建 Provider
    let provider = GlmProvider::new("glm-4.7-flashx", api_key)
        .with_base_url(base_url);

    // 准备消息
    let messages = vec![
        agent_lib::model::Message {
            role: agent_lib::model::MessageRole::User,
            content: "你好！请用一句话介绍一下你自己。".to_string(),
        }
    ];

    println!("\n=== 调用模型 ===\n");

    // 调用模型
    match provider.chat(messages, vec![]).await {
        Ok(response) => {
            println!("成功!");
            println!("\n响应内容:\n{}", response.content);
            println!("\nToken 使用: {:?}", response.usage);
        }
        Err(e) => {
            println!("错误: {:?}", e);
            return Err(e.into());
        }
    }

    Ok(())
}

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
