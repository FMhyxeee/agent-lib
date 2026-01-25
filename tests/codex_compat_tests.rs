/// Codex 兼容功能集成测试
///
/// 测试新添加的 Codex 兼容功能，包括：
/// - Token 管理与压缩
/// - submission_loop
/// - Op/Event 扩展
/// - TurnContext 增强
use agent_lib::protocol::{
    ApprovalPolicy, CollaborationMode, CompactedItem, Event, McpServerRefreshConfig, McpToolInfo,
    Op, ReasoningEffort, ReasoningSummary, ReviewDecision, SandboxPolicy, UserInputItem,
    UserInputResponse,
};
use agent_lib::session::{Session, SessionConfig, TurnContext};
use agent_lib::tasks::{CompactTask, Submission, TaskKind};
use agent_lib::token::{
    TokenCounter, TruncationMode, TruncationPolicy, approx_token_count, tiktoken_count,
};
use tokio::sync::mpsc;

#[tokio::test]
async fn test_submission_queue_extended_ops() {
    let (tx, mut rx) = mpsc::channel(10);

    // 测试各种新的 Op 类型
    let ops = vec![
        Op::Interrupt,
        Op::Undo,
        Op::Compact,
        Op::Shutdown,
        Op::ListMcpTools,
        Op::ListModels,
        Op::ListCustomPrompts,
        Op::RunUserShellCommand {
            command: "echo test".to_string(),
        },
    ];

    for op in ops {
        tx.send(Submission::new("test", op)).await.unwrap();
    }

    // 关闭发送端，这样 recv 可以正确结束
    drop(tx);

    // 验证所有 Op 都能正确发送
    let mut count = 0;
    while (rx.recv().await).is_some() {
        count += 1;
    }
    assert_eq!(count, 8);
}

#[tokio::test]
async fn test_op_serialization() {
    // 测试 Op 序列化/反序列化
    let op = Op::UserTurn {
        items: vec![UserInputItem::text("hello")],
        cwd: std::path::PathBuf::from("/home"),
        approval_policy: ApprovalPolicy::AlwaysAsk,
        sandbox_policy: SandboxPolicy::Persistent,
        model: "gpt-4".to_string(),
        effort: Some(ReasoningEffort::High),
        summary: ReasoningSummary {
            summary: "test".to_string(),
            token_count: 100,
        },
        final_output_json_schema: None,
        collaboration_mode: Some(CollaborationMode::Solo),
    };

    let json = serde_json::to_string(&op).unwrap();
    let parsed: Op = serde_json::from_str(&json).unwrap();

    match parsed {
        Op::UserTurn { model, effort, .. } => {
            assert_eq!(model, "gpt-4");
            assert_eq!(effort, Some(ReasoningEffort::High));
        }
        _ => panic!("Expected UserTurn variant"),
    }
}

#[tokio::test]
async fn test_event_serialization() {
    // 测试 Event 序列化/反序列化
    let event = Event::ContextCompacted {
        compacted_items: vec![CompactedItem {
            turn_id: "turn-1".to_string(),
            summary: "test".to_string(),
            original_token_count: 1000,
        }],
    };

    let json = serde_json::to_string(&event).unwrap();
    let parsed: Event = serde_json::from_str(&json).unwrap();

    match parsed {
        Event::ContextCompacted { compacted_items } => {
            assert_eq!(compacted_items.len(), 1);
            assert_eq!(compacted_items[0].turn_id, "turn-1");
        }
        _ => panic!("Expected ContextCompacted variant"),
    }
}

#[test]
fn test_approval_policy_serde() {
    let policy = ApprovalPolicy::AlwaysAsk;
    let json = serde_json::to_string(&policy).unwrap();
    assert_eq!(json, "\"always-ask\"");

    let parsed: ApprovalPolicy = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed, ApprovalPolicy::AlwaysAsk);
}

#[test]
fn test_sandbox_policy_serde() {
    let policy = SandboxPolicy::InMemory;
    let json = serde_json::to_string(&policy).unwrap();
    assert_eq!(json, "\"in-memory\"");

    let parsed: SandboxPolicy = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed, SandboxPolicy::InMemory);
}

#[test]
fn test_reasoning_effort_serde() {
    let effort = ReasoningEffort::High;
    let json = serde_json::to_string(&effort).unwrap();
    assert_eq!(json, "\"high\"");

    let parsed: ReasoningEffort = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed, ReasoningEffort::High);
}

#[test]
fn test_truncation_policy_tokens() {
    let policy = TruncationPolicy::tokens(1000);
    assert_eq!(policy.token_budget(), 1000);
    assert_eq!(policy.limit, 1000);
    assert!(matches!(policy.mode, TruncationMode::Tokens(1000)));
}

#[test]
fn test_truncation_policy_bytes() {
    let policy = TruncationPolicy::bytes(4000);
    assert_eq!(policy.token_budget(), 1000); // 4000 bytes / 4
    assert_eq!(policy.limit, 4000);
    assert!(matches!(policy.mode, TruncationMode::Bytes(4000)));
}

#[test]
fn test_token_counter_consistency() {
    let counter = TokenCounter::with_approx();
    let text = "Hello, world! This is a test.";

    let count1 = counter.count(text);
    let count2 = counter.count(text);

    assert_eq!(count1, count2);
    assert!(count1 > 0);
}

#[test]
fn test_approx_token_count_with_empty() {
    assert_eq!(approx_token_count(""), 0);
}

#[test]
fn test_approx_token_count_with_unicode() {
    let text = "你好世界"; // 12 bytes
    let count = approx_token_count(text);
    assert!(count > 0);
}

#[test]
fn test_tiktoken_count_non_empty() {
    let count = tiktoken_count("Hello, world!");
    assert!(count > 0);
}

#[tokio::test]
async fn test_session_config() {
    let config = SessionConfig {
        queue_buffer: 128,
        event_buffer: 256,
        default_model: "gpt-4".to_string(),
        default_cwd: Some("/home/user".to_string()),
        default_approval_policy: Some(ApprovalPolicy::NeverAsk),
    };

    assert_eq!(config.queue_buffer, 128);
    assert_eq!(config.event_buffer, 256);
    assert_eq!(config.default_model, "gpt-4");
    assert_eq!(config.default_cwd, Some("/home/user".to_string()));
    assert_eq!(
        config.default_approval_policy,
        Some(ApprovalPolicy::NeverAsk)
    );
}

#[tokio::test]
async fn test_session_config_default() {
    let config = SessionConfig::default();
    assert_eq!(config.queue_buffer, 64);
    assert_eq!(config.event_buffer, 64);
    assert_eq!(config.default_model, "default");
    assert!(config.default_cwd.is_none());
    assert!(config.default_approval_policy.is_none());
}

#[tokio::test]
async fn test_turn_context_with_all_options() {
    let ctx = TurnContext::new("gpt-4")
        .with_cwd("/home/user")
        .with_approval_policy(ApprovalPolicy::AlwaysAsk)
        .with_sandbox_policy(SandboxPolicy::Readonly)
        .with_reasoning_effort(ReasoningEffort::Medium)
        .with_context_window(200000)
        .with_auto_compact_limit(50000);

    assert_eq!(ctx.model, "gpt-4");
    assert_eq!(ctx.cwd, Some("/home/user".to_string()));
    assert_eq!(ctx.get_approval_policy(), ApprovalPolicy::AlwaysAsk);
    assert_eq!(ctx.get_sandbox_policy(), SandboxPolicy::Readonly);
    assert_eq!(ctx.reasoning_effort, Some(ReasoningEffort::Medium));
    assert_eq!(ctx.context_window, 200000);
    assert_eq!(ctx.auto_compact_token_limit, Some(50000));
}

#[test]
fn test_compacted_item() {
    let item = CompactedItem {
        turn_id: "turn-123".to_string(),
        summary: "Test summary".to_string(),
        original_token_count: 5000,
    };

    assert_eq!(item.turn_id, "turn-123");
    assert_eq!(item.summary, "Test summary");
    assert_eq!(item.original_token_count, 5000);

    // 测试序列化
    let json = serde_json::to_string(&item).unwrap();
    let parsed: CompactedItem = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.turn_id, "turn-123");
}

#[test]
fn test_mcp_server_refresh_config() {
    let config = McpServerRefreshConfig::default();
    assert!(!config.force_reload);

    let config = McpServerRefreshConfig { force_reload: true };
    assert!(config.force_reload);
}

#[test]
fn test_mcp_tool_info() {
    let info = McpToolInfo {
        name: "test-tool".to_string(),
        description: "A test tool".to_string(),
        server: "test-server".to_string(),
    };

    assert_eq!(info.name, "test-tool");
    assert_eq!(info.description, "A test tool");
    assert_eq!(info.server, "test-server");
}

#[test]
fn test_review_decision() {
    let decision = ReviewDecision::Approve;
    assert_eq!(decision, ReviewDecision::Approve);

    let decision_with_edits = ReviewDecision::ApproveWithEdits {
        edits: "some edits".to_string(),
    };
    assert_ne!(decision_with_edits, ReviewDecision::Approve);
}

#[test]
fn test_user_input_response() {
    let response = UserInputResponse::Text("answer".to_string());
    assert!(matches!(response, UserInputResponse::Text(_)));

    let cancel = UserInputResponse::Cancel;
    assert!(matches!(cancel, UserInputResponse::Cancel));
}

#[test]
fn test_task_kind_copy() {
    let kind = TaskKind::Regular;
    let kind2 = kind;
    assert_eq!(kind, kind2);
}

#[tokio::test]
async fn test_session_new_default_turn() {
    let config = SessionConfig {
        default_model: "gpt-4".to_string(),
        ..Default::default()
    };
    let (session, _) = Session::with_config(64, config);

    let ctx = session.new_default_turn().await;
    assert_eq!(ctx.model, "gpt-4");
    assert!(!ctx.sub_id.is_empty());
}

#[tokio::test]
async fn test_session_emit_event() {
    let (session, handle) = Session::new(10);

    session
        .emit_event(Event::Warning {
            message: "test warning".to_string(),
        })
        .await;

    let event = handle.next_event().await;
    assert!(matches!(event, Some(Event::Warning { .. })));
}

#[tokio::test]
async fn test_session_spawn_task() {
    let (session, _) = Session::new(10);

    let ctx = session.new_default_turn().await;
    session.spawn_task(ctx, CompactTask).await;

    // 给任务一些时间启动
    tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
}

#[tokio::test]
async fn test_session_abort_all_tasks() {
    let (session, _) = Session::new(10);

    let ctx = session.new_default_turn().await;
    session.spawn_task(ctx, CompactTask).await;

    session.abort_all_tasks().await;

    // 等待一下确保任务被中断
    tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
}
