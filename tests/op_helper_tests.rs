#[cfg(test)]
mod op_helper_tests {
    use agent_lib::protocol::ApprovalPolicy;
    use agent_lib::protocol::UserInputItem;
    use agent_lib::protocol::{
        Op, compact, interrupt, shutdown, undo, user_turn, user_turn_with_config,
    };
    use std::path::PathBuf;

    #[test]
    fn test_user_turn_constructor() {
        let items = vec![UserInputItem::text("Hello")];
        let op = user_turn(items, "gpt-4");

        if let Op::UserTurn {
            ref items,
            ref model,
            ..
        } = op
        {
            assert_eq!(items.len(), 1);
            assert_eq!(model, "gpt-4");
        } else {
            panic!("Expected UserTurn variant");
        }
    }

    #[test]
    fn test_user_turn_with_config_constructor() {
        let items = vec![UserInputItem::text("Hello")];
        let cwd = PathBuf::from("/tmp");
        let op = user_turn_with_config(
            items,
            "gpt-4",
            cwd.clone(),
            ApprovalPolicy::NeverAsk,
            agent_lib::protocol::SandboxPolicy::Readonly,
        );

        if let Op::UserTurn {
            ref cwd,
            ref approval_policy,
            ref sandbox_policy,
            ..
        } = op
        {
            assert_eq!(cwd, &cwd.clone());
            assert_eq!(*approval_policy, ApprovalPolicy::NeverAsk);
            assert_eq!(
                *sandbox_policy,
                agent_lib::protocol::SandboxPolicy::Readonly
            );
        } else {
            panic!("Expected UserTurn variant");
        }
    }

    #[test]
    fn test_simple_constructors() {
        let interrupt_op = interrupt();
        assert!(matches!(interrupt_op, Op::Interrupt));

        let undo_op = undo();
        assert!(matches!(undo_op, Op::Undo));

        let shutdown_op = shutdown();
        assert!(matches!(shutdown_op, Op::Shutdown));

        let compact_op = compact();
        assert!(matches!(compact_op, Op::Compact));
    }

    #[test]
    fn test_op_classification_functions() {
        // 测试需要用户交互的操作
        let user_turn_op = user_turn(vec![UserInputItem::text("Hello")], "gpt-4");
        let user_input_op = Op::UserInput {
            content: "test".to_string(),
        };
        let review_op = Op::Review {
            review_request: agent_lib::protocol::ReviewRequest {
                content: "diff".to_string(),
                context: None,
            },
        };

        assert!(agent_lib::protocol::requires_user_interaction(
            &user_turn_op
        ));
        assert!(agent_lib::protocol::requires_user_interaction(
            &user_input_op
        ));
        assert!(agent_lib::protocol::requires_user_interaction(&review_op));

        // 测试不需要用户交互的操作
        let system_op = Op::Interrupt;
        assert!(!agent_lib::protocol::requires_user_interaction(&system_op));

        // 测试系统控制操作
        assert!(agent_lib::protocol::is_system_control(&Op::Interrupt));
        assert!(agent_lib::protocol::is_system_control(&undo()));
        assert!(agent_lib::protocol::is_system_control(&shutdown()));
        assert!(agent_lib::protocol::is_system_control(&compact()));

        // 测试非系统控制操作
        assert!(!agent_lib::protocol::is_system_control(&user_turn_op));

        // 测试 MCP 相关操作
        let mcp_op = Op::ListMcpTools;
        let mcp_refresh_op = Op::RefreshMcpServers {
            config: agent_lib::protocol::McpServerRefreshConfig::default(),
        };

        assert!(agent_lib::protocol::is_mcp_related(&mcp_op));
        assert!(agent_lib::protocol::is_mcp_related(&mcp_refresh_op));

        // 测试非 MCP 相关操作
        assert!(!agent_lib::protocol::is_mcp_related(&user_turn_op));
    }

    #[test]
    fn test_op_serialization_still_works() {
        let op = user_turn(vec![UserInputItem::text("test")], "gpt-4");
        let json = serde_json::to_string(&op).unwrap();

        println!("Serialized JSON: {}", json);

        // 验证 JSON 包含必要的字段
        assert!(json.contains("\"items\":["));
        assert!(json.contains("\"model\":\"gpt-4\""));
        assert!(json.contains("\"UserTurn\"") || json.contains("\"user-turn\""));

        // 验证反序列化
        let parsed: Op = serde_json::from_str(&json).unwrap();
        assert!(matches!(parsed, Op::UserTurn { .. }));
    }
}
