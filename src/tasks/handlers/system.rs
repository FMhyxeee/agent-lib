//! 系统操作相关handlers
//!
//! 处理Agent移交、历史查询、自定义提示、模型列表等系统级操作。

use std::path::Path;
use tracing::debug;

use crate::protocol::Event;
use crate::session::Session;

/// 处理 Agent 移交
pub async fn handle_handoff(sess: &Session, target_agent: String, context: serde_json::Value) {
    debug!(target_agent = %target_agent, "Handling handoff");

    // 获取当前状态
    let current_state = sess.history().await;
    let state_json =
        serde_json::to_value(&current_state).unwrap_or_else(|_| serde_json::json!({}));

    // 构建移交上下文
    let handoff_context = serde_json::json!({
        "source": "current_session",
        "target": target_agent,
        "timestamp": chrono::Utc::now().to_rfc3339(),
        "history": state_json,
        "user_context": context,
    });

    // 发送移交发起事件
    sess.emit_event(Event::HandoffInitiated {
        from: "current_session".to_string(),
        to: target_agent.clone(),
    })
    .await;

    // 记录移交日志
    debug!(
        from = "current_session",
        to = target_agent,
        context = ?handoff_context,
        "Handoff initiated"
    );

    // 如果有 Agent 注册表，通知目标 Agent
    if let Some(receiver) = crate::agent::global_agent_registry()
        .get(&target_agent)
        .await
    {
        if let Err(err) = receiver.receive_handoff(handoff_context.clone()).await {
            sess.emit_event(Event::Warning {
                message: format!("handoff notify failed: {err}"),
            })
            .await;
        }
    } else {
        sess.emit_event(Event::Warning {
            message: format!("handoff target not registered: {target_agent}"),
        })
        .await;
    }

    // 发送完成事件
    sess.emit_event(Event::TurnComplete {
        result: serde_json::json!({"handoff": target_agent}),
    })
    .await;
}

/// 处理历史条目请求
pub async fn handle_get_history_entry_request(sess: &Session, offset: usize, log_id: u64) {
    debug!(
        offset = offset,
        log_id = log_id,
        "Handling get history entry request"
    );

    let history = sess.history().await;
    let entries = history.all();

    if offset < entries.len() {
        let entry = &entries[offset];
        sess.emit_event(Event::HistoryEntry {
            offset,
            log_id,
            entry: serde_json::to_string(entry).unwrap_or_else(|_| format!("{:?}", entry)),
        })
        .await;
    } else {
        sess.emit_event(Event::Error {
            error: crate::error::AgentError::Tool(format!(
                "History entry not found at offset {}",
                offset
            )),
        })
        .await;
    }
}

/// 处理列出自定义提示请求
pub async fn handle_list_custom_prompts(sess: &Session) {
    debug!("Handling list custom prompts");

    let mut prompts: Vec<crate::protocol::CustomPromptInfo> = Vec::new();

    // 扫描自定义提示目录
    let prompts_dir = Path::new(".claude").join("prompts");
    if let Ok(found) = scan_prompts_directory(&prompts_dir).await {
        prompts = found;
    }

    // 添加内置提示
    prompts.push(crate::protocol::CustomPromptInfo {
        name: "code_review".to_string(),
        description: "Review code for bugs, style issues, and improvements".to_string(),
    });
    prompts.push(crate::protocol::CustomPromptInfo {
        name: "debug_helper".to_string(),
        description: "Help debug code issues by analyzing errors and suggesting fixes"
            .to_string(),
    });
    prompts.push(crate::protocol::CustomPromptInfo {
        name: "documentation".to_string(),
        description: "Generate comprehensive documentation for code".to_string(),
    });

    sess.emit_event(Event::ListCustomPromptsResponse { prompts })
        .await;
}

/// 扫描提示目录
async fn scan_prompts_directory(
    dir: &Path,
) -> std::io::Result<Vec<crate::protocol::CustomPromptInfo>> {
    let mut prompts = Vec::new();

    if !dir.exists() {
        return Ok(prompts);
    }

    let mut entries = tokio::fs::read_dir(dir).await?;
    while let Some(entry) = entries.next_entry().await? {
        let path = entry.path();

        // 支持的提示文件扩展名
        let is_prompt_file = path
            .extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| matches!(ext, "md" | "txt" | "prompt"))
            .unwrap_or(false);

        if is_prompt_file {
            let name = path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("unknown")
                .to_string();

            // 尝试读取文件获取描述
            let description = if let Ok(content) = tokio::fs::read_to_string(&path).await {
                // 获取第一行作为描述
                content
                    .lines()
                    .next()
                    .unwrap_or("Custom prompt")
                    .to_string()
            } else {
                "Custom prompt file".to_string()
            };

            prompts.push(crate::protocol::CustomPromptInfo { name, description });
        }
    }

    Ok(prompts)
}

/// 处理列出模型请求
pub async fn handle_list_models(sess: &Session) {
    debug!("Handling list models");

    // 返回固定配置的模型列表
    let fixed_models = crate::model::list_models();
    let models = fixed_models
        .iter()
        .map(|m| crate::protocol::ModelInfo {
            id: m.id.to_string(),
            name: m.display_name.to_string(),
            provider: m.provider.to_string(),
        })
        .collect();

    sess.emit_event(Event::ModelsListed { models }).await;
}

/// 处理 StartTurn - 开始新的 Turn
pub async fn handle_start_turn(sess: &Session, prompt: String) {
    debug!(prompt = %prompt, "Handling start turn");

    let turn_id = uuid::Uuid::new_v4().to_string();
    sess.emit_event(Event::TurnStarted { turn_id }).await;

    sess.emit_event(Event::ModelComplete {
        content: prompt,
        usage: Default::default(),
    })
    .await;
}
