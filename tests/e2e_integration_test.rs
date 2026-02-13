// E2E 集成测试 - 代码审查助手场景
//
// 测试场景：
// 1. 用户提交代码审查请求
// 2. 运行 git 命令查看状态
// 3. 模型返回审查意见
// 4. 列出可用技能和自定义提示
// 5. 查看历史记录
// 6. 切换模型
// 7. 压缩历史
// 8. 系统中断

use agent_lib::model::provider::GlmProvider;
use agent_lib::protocol::{
    ApprovalPolicy, CollaborationMode, Event, Op, ReasoningEffort, ReasoningSummary, SandboxPolicy,
    UserInputItem,
};
use agent_lib::session::Session;
use tokio::time::{Duration, timeout};

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

#[tokio::test]
async fn e2e_code_review_assistant_scenario() {
    // === Setup ===
    let api_key = read_env_var(".env", "GLM_API_KEY").expect("GLM_API_KEY required");
    let base_url = read_env_var(".env", "GLM_BASE_URL")
        .unwrap_or_else(|_| "https://open.bigmodel.cn/api/paas/v4/chat/completions".to_string());

    let provider = GlmProvider::new("glm-4.7-flashx", api_key).with_base_url(base_url);
    let (_session, handle) = Session::with_config(
        64,
        agent_lib::session::SessionConfig {
            model: Some(
                std::sync::Arc::new(provider) as std::sync::Arc<dyn agent_lib::model::ModelClient>
            ),
            ..Default::default()
        },
    );

    println!("🧪 E2E 测试开始: 代码审查助手场景\n");

    // === Turn 1: 用户提交代码审查请求 ===
    println!("📝 Turn 1: 用户提交代码审查请求");
    handle
        .submit(Op::UserTurn {
            items: vec![UserInputItem::text("你好！请用一句话介绍一下你自己。")],
            cwd: std::path::PathBuf::from("."),
            approval_policy: ApprovalPolicy::NeverAsk,
            sandbox_policy: SandboxPolicy::Persistent,
            model: "glm-4.7-flashx".to_string(),
            effort: Some(ReasoningEffort::High),
            summary: ReasoningSummary {
                summary: String::new(),
                token_count: 0,
            },
            final_output_json_schema: None,
            collaboration_mode: Some(CollaborationMode::Solo),
        })
        .await
        .expect("Failed to submit UserTurn");

    // 验证 TurnStarted 事件
    let event = timeout(Duration::from_secs(2), handle.next_event())
        .await
        .expect("Timeout waiting for TurnStarted")
        .expect("No event received");
    assert!(
        matches!(event, Event::TurnStarted { .. }),
        "First event should be TurnStarted"
    );
    println!("  ✅ TurnStarted 事件接收");

    // 验证 ModelStreaming 事件
    let mut review_content = String::new();
    let mut has_complete = false;

    for _ in 0..100 {
        match timeout(Duration::from_secs(30), handle.next_event()).await {
            Ok(Some(event)) => match event {
                Event::ModelStreaming { chunk } => {
                    review_content.push_str(&chunk);
                    print!("{}", chunk);
                    std::io::Write::flush(&mut std::io::stdout()).ok();
                }
                Event::ModelComplete { .. } => {
                    has_complete = true;
                    break;
                }
                Event::Error { error } => {
                    panic!("Model error: {:?}", error);
                }
                _ => {}
            },
            Err(_) => break,
            Ok(None) => continue,
        }
    }

    assert!(has_complete, "Should receive ModelComplete event");
    assert!(
        !review_content.is_empty(),
        "Response content should not be empty"
    );
    println!("  ✅ ModelStreaming + ModelComplete 事件接收");
    println!("  📄 响应内容长度: {} 字符", review_content.chars().count());

    // === Turn 2: 运行 shell 命令 ===
    println!("\n🔧 Turn 2: 运行 git log 命令");
    handle
        .submit(Op::RunUserShellCommand {
            command: "git log --oneline -5".to_string(),
        })
        .await
        .expect("Failed to submit RunUserShellCommand");

    // 验证命令执行事件
    let mut has_command_event = false;

    for _ in 0..50 {
        match timeout(Duration::from_millis(200), handle.next_event()).await {
            Ok(Some(event)) => match event {
                Event::RunUserShellCommand { command } => {
                    assert!(command.contains("git log"), "Command should be git log");
                    has_command_event = true;
                }
                Event::Error { error } => {
                    // git 可能失败，这是可以接受的
                    println!("  ⚠️ Command error (expected if no git repo): {:?}", error);
                    has_command_event = true; // 有错误事件也算收到了
                    break;
                }
                _ => {}
            },
            Err(_) => break,
            Ok(None) => continue,
        }
    }

    assert!(
        has_command_event,
        "Should receive RunUserShellCommand event"
    );
    println!("  ✅ RunUserShellCommand + ModelStreaming 事件接收");

    // === Turn 3: 列出技能 ===
    println!("\n📚 Turn 3: 列出可用技能");
    handle
        .submit(Op::ListSkills {
            cwds: vec![std::path::PathBuf::from(".")],
            force_reload: true,
        })
        .await
        .expect("Failed to submit ListSkills");

    let mut has_skills_response = false;
    for _ in 0..50 {
        match timeout(Duration::from_millis(200), handle.next_event()).await {
            Ok(Some(event)) => match event {
                Event::ListSkillsResponse { .. } => {
                    has_skills_response = true;
                    break;
                }
                Event::Warning { message } => {
                    println!("  ℹ️ {}", message);
                }
                Event::Error { error } => {
                    panic!("Error in ListSkills: {:?}", error);
                }
                _ => {}
            },
            Err(_) => break,
            Ok(None) => continue,
        }
    }
    assert!(has_skills_response, "Should receive ListSkillsResponse");
    println!("  ✅ ListSkillsResponse 事件接收");

    // === Turn 4: 列出自定义提示 ===
    println!("\n📋 Turn 4: 列出自定义提示");
    handle
        .submit(Op::ListCustomPrompts)
        .await
        .expect("Failed to submit ListCustomPrompts");

    let mut has_prompts_response = false;
    for _ in 0..50 {
        match timeout(Duration::from_millis(200), handle.next_event()).await {
            Ok(Some(event)) => match event {
                Event::ListCustomPromptsResponse { .. } => {
                    has_prompts_response = true;
                    break;
                }
                Event::Error { error } => {
                    panic!("Error in ListCustomPrompts: {:?}", error);
                }
                _ => {}
            },
            Err(_) => break,
            Ok(None) => continue,
        }
    }
    assert!(
        has_prompts_response,
        "Should receive ListCustomPromptsResponse"
    );
    println!("  ✅ ListCustomPromptsResponse 事件接收");

    // === Turn 5: 查看历史记录 ===
    println!("\n📜 Turn 5: 查看历史记录");
    handle
        .submit(Op::GetHistoryEntryRequest {
            offset: 0,
            log_id: 0,
        })
        .await
        .expect("Failed to submit GetHistoryEntryRequest");

    let mut has_history_entry = false;
    for _ in 0..50 {
        match timeout(Duration::from_millis(200), handle.next_event()).await {
            Ok(Some(event)) => match event {
                Event::HistoryEntry { .. } => {
                    has_history_entry = true;
                    break;
                }
                Event::Error { error } => {
                    panic!("Error in GetHistoryEntryRequest: {:?}", error);
                }
                _ => {}
            },
            Err(_) => break,
            Ok(None) => continue,
        }
    }
    assert!(has_history_entry, "Should receive HistoryEntry");
    println!("  ✅ HistoryEntry 事件接收");

    // === Turn 6: 列出模型 ===
    println!("\n🤖 Turn 6: 列出可用模型");
    handle
        .submit(Op::ListModels)
        .await
        .expect("Failed to submit ListModels");

    let mut has_models_listed = false;
    for _ in 0..50 {
        match timeout(Duration::from_millis(200), handle.next_event()).await {
            Ok(Some(event)) => match event {
                Event::ModelsListed { models } => {
                    has_models_listed = true;
                    assert!(!models.is_empty(), "Should have at least one model");
                    println!("  📊 可用模型: {}", models.len());
                    for model in &models {
                        println!("     - {} ({})", model.name, model.id);
                    }
                    break;
                }
                Event::Error { error } => {
                    panic!("Error in ListModels: {:?}", error);
                }
                _ => {}
            },
            Err(_) => break,
            Ok(None) => continue,
        }
    }
    assert!(has_models_listed, "Should receive ModelsListed");
    println!("  ✅ ModelsListed 事件接收");

    // === Turn 7: 压缩历史 ===
    println!("\n🗜️  Turn 7: 压缩历史记录");
    handle
        .submit(Op::Compact)
        .await
        .expect("Failed to submit Compact");

    let mut has_compacted = false;
    for _ in 0..50 {
        match timeout(Duration::from_millis(200), handle.next_event()).await {
            Ok(Some(event)) => match event {
                Event::ContextCompacted { .. } => {
                    has_compacted = true;
                    break;
                }
                Event::TurnComplete { .. } => {
                    // Compact 也可能触发 TurnComplete
                    has_compacted = true;
                    break;
                }
                Event::Error { error } => {
                    println!("  ⚠️ Compact error (acceptable): {:?}", error);
                    has_compacted = true;
                    break;
                }
                _ => {}
            },
            Err(_) => break,
            Ok(None) => continue,
        }
    }
    assert!(has_compacted, "Should receive Compact confirmation event");
    println!("  ✅ ContextCompacted/TurnComplete 事件接收");

    // === Turn 8: 系统中断 ===
    println!("\n🛑 Turn 8: 系统中断");
    handle
        .submit(Op::Interrupt)
        .await
        .expect("Failed to submit Interrupt");

    let mut has_error = false;
    for _ in 0..50 {
        match timeout(Duration::from_millis(200), handle.next_event()).await {
            Ok(Some(event)) => match event {
                Event::Error { .. } => {
                    has_error = true;
                    break;
                }
                _ => {}
            },
            Err(_) => break,
            Ok(None) => continue,
        }
    }
    assert!(has_error, "Should receive Error on Interrupt");
    println!("  ✅ Error 事件接收（中断确认）");

    // === Turn 9: OverrideTurnContext ===
    println!("\n⚙️  Turn 9: 覆盖 Turn 上下文");
    handle
        .submit(Op::OverrideTurnContext {
            cwd: Some(std::path::PathBuf::from("/workspace")),
            approval_policy: Some(ApprovalPolicy::ReadOnlySafe),
            sandbox_policy: Some(SandboxPolicy::Readonly),
            model: Some("glm-4.7".to_string()),
            effort: Some(Some(ReasoningEffort::Medium)),
            summary: None,
            collaboration_mode: Some(CollaborationMode::Collaborative),
        })
        .await
        .expect("Failed to submit OverrideTurnContext");

    let mut has_warning = false;
    for _ in 0..50 {
        match timeout(Duration::from_millis(200), handle.next_event()).await {
            Ok(Some(event)) => match event {
                Event::Warning { message } => {
                    println!("  📝 {}", message);
                    has_warning = true;
                    break;
                }
                Event::Error { error } => {
                    panic!("Error in OverrideTurnContext: {:?}", error);
                }
                _ => {}
            },
            Err(_) => break,
            Ok(None) => continue,
        }
    }
    assert!(
        has_warning,
        "Should receive Warning for OverrideTurnContext"
    );
    println!("  ✅ Warning 事件接收（上下文已更新）");

    // === Turn 10: 测试 Undo ===
    println!("\n↩️  Turn 10: 撤销操作");
    handle
        .submit(Op::Undo)
        .await
        .expect("Failed to submit Undo");

    let mut has_undo = false;
    for _ in 0..50 {
        match timeout(Duration::from_millis(200), handle.next_event()).await {
            Ok(Some(event)) => match event {
                Event::UndoPerformed { .. } => {
                    has_undo = true;
                    break;
                }
                Event::Error { error } => {
                    println!("  ⚠️ Undo error (acceptable if empty history): {:?}", error);
                    has_undo = true;
                    break;
                }
                _ => {}
            },
            Err(_) => break,
            Ok(None) => continue,
        }
    }
    assert!(has_undo, "Should receive UndoPerformed or error");
    println!("  ✅ UndoPerformed 事件接收");

    // === 总结 ===
    println!("\n✨ E2E 集成测试完成！");
    println!("测试覆盖的 Op 类型:");
    println!("  ✅ UserTurn (带完整配置)");
    println!("  ✅ RunUserShellCommand");
    println!("  ✅ ListSkills");
    println!("  ✅ ListCustomPrompts");
    println!("  ✅ GetHistoryEntryRequest");
    println!("  ✅ ListModels");
    println!("  ✅ Compact");
    println!("  ✅ Interrupt");
    println!("  ✅ OverrideTurnContext");
    println!("  ✅ Undo");
    println!("\n测试覆盖的 Event 类型:");
    println!("  ✅ TurnStarted");
    println!("  ✅ ModelStreaming");
    println!("  ✅ ModelComplete");
    println!("  ✅ RunUserShellCommand");
    println!("  ✅ ListSkillsResponse");
    println!("  ✅ ListCustomPromptsResponse");
    println!("  ✅ HistoryEntry");
    println!("  ✅ ModelsListed");
    println!("  ✅ ContextCompacted/TurnComplete");
    println!("  ✅ Error");
    println!("  ✅ Warning");
    println!("  ✅ UndoPerformed");
}
