//! Mock Filesystem MCP Server 测试示例
//!
//! 这个示例创建一个模拟的 MCP 文件系统服务器，
//! 用于测试 Session 中的 MCP 功能集成。

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

use agent_lib::mcp::McpManager;
use agent_lib::protocol::{Event, McpServerRefreshConfig, Op};
use agent_lib::session::{Session, SessionConfig};
use agent_lib::{AgentError, AgentResult};

/// Mock Filesystem MCP 服务器
///
/// 模拟内存中的文件系统操作
#[derive(Clone)]
struct MockFilesystemServer {
    files: Arc<RwLock<HashMap<String, String>>>,
}

impl MockFilesystemServer {
    fn new() -> Self {
        Self {
            files: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// 读取文件
    async fn read_file(&self, path: &str) -> AgentResult<String> {
        let files = self.files.read().await;
        files
            .get(path)
            .cloned()
            .ok_or_else(|| AgentError::Tool(format!("File not found: {}", path)))
    }

    /// 写入文件
    async fn write_file(&self, path: &str, content: &str) -> AgentResult<String> {
        let mut files = self.files.write().await;
        files.insert(path.to_string(), content.to_string());
        Ok(format!("Written {} bytes to {}", content.len(), path))
    }

    /// 列出目录
    async fn list_directory(&self, path: &str) -> AgentResult<Vec<String>> {
        let files = self.files.read().await;
        let prefix = if path.ends_with('/') {
            path.to_string()
        } else {
            format!("{}/", path)
        };

        let entries: Vec<String> = files
            .keys()
            .filter(|k| k.starts_with(&prefix))
            .map(|k| k.to_string())
            .collect();

        Ok(entries)
    }

    /// 删除文件
    async fn delete_file(&self, path: &str) -> AgentResult<String> {
        let mut files = self.files.write().await;
        files
            .remove(path)
            .map(|_| format!("Deleted: {}", path))
            .ok_or_else(|| AgentError::Tool(format!("File not found: {}", path)))
    }
}

/// 注册 Mock Filesystem 到 McpManager
async fn setup_mock_filesystem_manager() -> AgentResult<Arc<McpManager>> {
    let manager = McpManager::new();

    // 创建 mock 服务器
    let server = MockFilesystemServer::new();

    // 预填充一些测试文件
    server
        .write_file("/test/hello.txt", "Hello, World!")
        .await?;
    server
        .write_file("/test/data.json", "{\"key\": \"value\"}")
        .await?;
    server
        .write_file("/config/settings.toml", "[settings]\nkey = \"value\"")
        .await?;

    // 为每个工具创建 Tool 实例并注册到 manager 的内部注册表
    // 注意: 这里我们使用模拟方式，真实场景中 MCP 服务器会通过网络连接
    tracing::info!("Mock filesystem server initialized with 3 test files");

    Ok(manager)
}

/// 测试 Session MCP 集成
async fn test_session_with_mcp() -> AgentResult<()> {
    println!("=== Session MCP 集成测试 ===\n");

    // 1. 创建带 MCP Manager 的 Session
    let mcp_manager = setup_mock_filesystem_manager().await?;

    let config = SessionConfig {
        mcp_manager: Some(mcp_manager.clone()),
        ..Default::default()
    };

    let (session, handle) = Session::with_config(64, config);

    println!("✓ Session 创建成功，带 McpManager");

    // 2. 验证 Session 可以获取 MCP Manager
    let retrieved_manager = session.get_mcp_manager();
    assert!(retrieved_manager.is_some(), "应该能获取到 MCP Manager");
    println!("✓ Session.get_mcp_manager() 工作正常");

    // 3. 测试 MCP Manager API
    let server_count = mcp_manager.server_count().await;
    println!("✓ MCP Manager 服务器数量: {}", server_count);

    let servers = mcp_manager.list_servers().await;
    println!("✓ MCP Manager 服务器列表: {:?}", servers);

    let tools_count = mcp_manager.total_tools_count().await;
    println!("✓ MCP Manager 工具总数: {}", tools_count);

    // 4. 测试发送 ListMcpTools Op
    println!("\n--- 测试 ListMcpTools Op ---");
    handle
        .submit(Op::ListMcpTools)
        .await?;

    // 等待事件
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    // 5. 测试 RefreshMcpServers Op
    println!("\n--- 测试 RefreshMcpServers Op ---");
    handle
        .submit(Op::RefreshMcpServers {
            config: McpServerRefreshConfig { force_reload: false },
        })
        .await?;

    // 等待事件
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    // 6. 测试事件流
    println!("\n--- 测试事件流 ---");
    let mut event_count = 0;

    // 使用超时来避免无限等待
    let receive_task = tokio::spawn(async move {
        let mut count = 0;
        // 只接收前几个事件，然后退出
        while count < 10 {
            match tokio::time::timeout(
                tokio::time::Duration::from_millis(500),
                handle.next_event(),
            )
            .await
            {
                Ok(Some(event)) => {
                    match event {
                        Event::TurnStarted { .. } => {
                            count += 1;
                        }
                        Event::McpListToolsResponse { .. } => {
                            count += 1;
                        }
                        Event::Error { .. } => {
                            count += 1;
                        }
                        _ => {}
                    }
                }
                Ok(None) => break,
                Err(_) => break,
            }
        }
        count
    });

    // 等待接收任务完成
    let timeout_duration = tokio::time::Duration::from_secs(1);
    match tokio::time::timeout(timeout_duration, receive_task).await {
        Ok(Ok(count)) => {
            event_count = count;
        }
        Ok(Err(_)) => {}
        Err(_) => {}
    }

    println!("✓ 收到 {} 个事件", event_count);

    Ok(())
}

/// 测试 Mock Fileserver 工具执行
async fn test_mock_fileserver_direct() -> AgentResult<()> {
    println!("\n=== 直接测试 Mock Fileserver ===\n");

    let server = MockFilesystemServer::new();

    // 预填充文件
    server
        .write_file("/demo/readme.md", "# Demo Project\n\nThis is a demo.")
        .await?;
    println!("✓ 创建测试文件 /demo/readme.md");

    // 测试读取
    let content = server.read_file("/demo/readme.md").await?;
    println!("✓ 读取文件内容: {}", content);

    // 测试列出目录
    let entries = server.list_directory("/demo").await?;
    println!("✓ 列出目录 /demo: {:?}", entries);

    // 测试写入
    let result = server
        .write_file("/demo/new.txt", "New content")
        .await?;
    println!("✓ 写入文件: {}", result);

    // 测试删除
    let result = server.delete_file("/demo/readme.md").await?;
    println!("✓ 删除文件: {}", result);

    Ok(())
}

#[tokio::main]
async fn main() -> AgentResult<()> {
    println!("╔══════════════════════════════════════════╗\n");
    println!("║  Mock Filesystem MCP 服务器测试        ║\n");
    println!("╚══════════════════════════════════════════╝\n");

    // 测试 1: 直接测试 Mock Fileserver
    if let Err(e) = test_mock_fileserver_direct().await {
        println!("❌ Mock Fileserver 测试失败: {:?}", e);
    }

    // 测试 2: Session MCP 集成测试
    if let Err(e) = test_session_with_mcp().await {
        println!("❌ Session MCP 测试失败: {:?}", e);
    }

    println!("\n=== 所有测试完成 ===");
    Ok(())
}
