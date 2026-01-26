# Project Observation Record #014

**Date:** 2025-01-26
**Observer:** Claude (Agent)
**Status:** 📋 Specification Ready
**Topic:** RefreshMcpServers 实现规范

---

## 任务概述

**优先级:** 🟡 中
**文件:** `src/tasks/loop.rs:401`
**当前状态:** 有 TODO

---

## 当前代码

```rust
// src/tasks/loop.rs:401-404
async fn handle_refresh_mcp_servers(_sess: &Session, config: McpServerRefreshConfig) {
    debug!(force = config.force_reload, "Handling refresh MCP servers");
    // TODO: 实现刷新 MCP 服务器逻辑
}
```

---

## 需要实现的功能

### 1. 配置结构

```rust
// src/protocol/types.rs 已存在
pub struct McpServerRefreshConfig {
    pub force_reload: bool,
}
```

### 2. 完整处理器

```rust
async fn handle_refresh_mcp_servers(sess: &Session, config: McpServerRefreshConfig) {
    use crate::mcp::McpManager;

    debug!(force = config.force_reload, "Handling refresh MCP servers");

    let Some(manager) = sess.get_mcp_manager() else {
        sess.emit_event(Event::Warning {
            message: "No MCP manager configured".to_string(),
        }).await;
        return;
    };

    // 获取刷新前的状态
    let before_count = manager.server_count().await;

    if config.force_reload {
        // 强制重新加载所有服务器
        debug!("Force reloading all MCP servers");
        // manager.reload_all().await;
    } else {
        // 只刷新不健康的连接
        debug!("Refreshing unhealthy MCP connections");
        // manager.refresh_unhealthy().await;
    }

    // 获取刷新后的状态
    let after_count = manager.server_count().await;
    let tools_count = manager.total_tools_count().await;

    sess.emit_event(Event::McpServersRefreshed {
        server_count: after_count,
        tools_count,
        force_reload: config.force_reload,
    }).await;
}
```

### 3. 新增 Event

```rust
// src/protocol/event.rs 添加
McpServersRefreshed {
    server_count: usize,
    tools_count: usize,
    force_reload: bool,
}
```

---

## McpManager 需要的方法

```rust
// src/mcp/manager.rs 需要添加
impl McpManager {
    /// 强制重新加载所有服务器
    pub async fn reload_all(&self) -> AgentResult<()> {
        for server in self.servers.iter() {
            server.reconnect().await?;
        }
        Ok(())
    }

    /// 只刷新不健康的连接
    pub async fn refresh_unhealthy(&self) -> AgentResult<()> {
        for server in self.servers.iter() {
            if !server.is_healthy().await {
                server.reconnect().await?;
            }
        }
        Ok(())
    }

    /// 检查服务器健康状态
    pub async fn is_healthy(&self) -> bool {
        // 实现健康检查
        true
    }
}
```

---

## 伪代码

```rust
async fn handle_refresh_mcp_servers(sess: &Session, config: McpServerRefreshConfig) {
    debug!(force = config.force_reload, "Handling refresh MCP servers");

    let manager = match sess.get_mcp_manager() {
        Some(m) => m,
        None => {
            sess.emit_event(Event::Warning {
                message: "No MCP manager configured".to_string(),
            }).await;
            return;
        }
    };

    let servers = manager.list_servers().await;
    let mut reconnected = 0;
    let mut failed = 0;

    for server_name in &servers {
        let result = if config.force_reload {
            manager.reconnect_server(server_name).await
        } else {
            manager.check_and_reconnect(server_name).await
        };

        match result {
            Ok(_) => reconnected += 1,
            Err(e) => {
                failed += 1;
                debug!(server = %server_name, error = %e, "Failed to refresh server");
            }
        }
    }

    sess.emit_event(Event::McpServersRefreshed {
        server_count: servers.len(),
        tools_count: manager.total_tools_count().await,
        force_reload: config.force_reload,
        reconnected,
        failed,
    }).await;
}
```

---

## 测试用例

```rust
#[tokio::test]
async fn test_refresh_mcp_servers() {
    let manager = Arc::new(McpManager::new());
    let config = SessionConfig {
        mcp_manager: Some(manager),
        ..Default::default()
    };
    let (session, handle) = Session::with_config(64, config);

    handle.submit(Op::RefreshMcpServers {
        config: McpServerRefreshConfig { force_reload: false },
    }).await;

    let event = handle.next_event().await;
    assert!(matches!(event, Event::McpServersRefreshed { .. }));
}

#[tokio::test]
async fn test_refresh_mcp_servers_force() {
    // 测试强制重新加载
}
```

---

## 相关文件

- `src/tasks/loop.rs:401` - 更新处理器
- `src/protocol/event.rs` - 添加 McpServersRefreshed Event
- `src/mcp/manager.rs` - 添加 reload/reconnect 方法
- `tests/mcp_integration_test.rs` - 添加测试

---

**规范完成，等待实现。**

---
