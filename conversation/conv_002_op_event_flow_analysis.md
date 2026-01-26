# Project Observation Record #002

**Date**: 2025-01-26
**Observer**: Claude (Agent)
**Status**: Pending
**Topic**: Agent 输入输出到 Event/Op 的转换流程分析

---

## 概述

agent-lib 使用 **SQ/EQ 协议**（Submission Queue / Event Queue）实现事件驱动的异步通信架构。

```
┌─────────────────────────────────────────────────────────────────────────┐
│                        Op → 处理 → Event 流程                           │
├─────────────────────────────────────────────────────────────────────────┤
│                                                                         │
│   用户/Agent                  Submission Loop                   Event   │
│      │                              │                          Stream  │
│      │                              │                             │     │
│      │   ┌──────────────────────┐   │    ┌─────────────────┐    │     │
│      └──→│   Op (命令/SQ)       │───┼───→│   处理器         │────┼───→│
│          │                      │   │    │   (Handler)      │    │     │
│          │ • UserTurn           │   │    │                 │    │     │
│          │ • Interrupt          │   │    │ • 路由 Op        │    │     │
│          │ • Compact            │   │    │ • 执行 Task      │    │     │
│          │ • Undo               │   │    │ • 发送 Event     │    │     │
│          │ • OverrideTurnContext│   │    └─────────────────┘    │     │
│          └──────────────────────┘   │                           │     │
│                                     └───────────────────────────┘     │
└─────────────────────────────────────────────────────────────────────────┘
```

---

## Op (Submission Queue) - 命令类型

Op 定义在 `src/protocol/op.rs`，分为以下几类：

### 1. 基础会话操作
| Op 变体 | 说明 | 生成场景 |
|---------|------|----------|
| `StartTurn` | 开始新的 Turn | Agent 启动新对话 |
| `UserInput` | 简单用户输入 | 用户发送文本 |
| `Interrupt` | 中断当前操作 | 用户请求中断 |

### 2. 用户交互操作
| Op 变体 | 说明 | 生成场景 |
|---------|------|----------|
| `UserTurn` | 完整用户输入上下文 | 用户提交带上下文的输入 |
| `UserInputLegacy` | 遗留格式支持 | 向后兼容 |
| `UserInputAnswer` | 用户输入回答 | 响应输入请求 |

### 3. 审查和批准操作
| Op 变体 | 说明 | 生成场景 |
|---------|------|----------|
| `ApprovalResponse` | 批准响应 | 工具批准 |
| `ExecApproval` | 执行代码审查批准 | Exec 审查结果 |
| `PatchApproval` | 补丁审查批准 | Patch 审查结果 |

### 4. 上下文管理操作
| Op 变体 | 说明 | 生成场景 |
|---------|------|----------|
| `OverrideTurnContext` | 覆盖 Turn 上下文 | 动态修改配置 |
| `AddToHistory` | 添加到历史 | 手动添加对话 |
| `GetHistoryEntryRequest` | 获取历史条目 | 查询历史 |

### 5. 历史管理操作
| Op 变体 | 说明 | 生成场景 |
|---------|------|----------|
| `Compact` | 压缩历史 | Token 超限时 |
| `Undo` | 撤销操作 | 撤销上一步 |
| `ThreadRollback` | 线程回滚 | 回滚多个回合 |
| `Review` | 代码审查 | 审查请求 |

### 6. 系统控制操作
| Op 变体 | 说明 | 生成场景 |
|---------|------|----------|
| `Shutdown` | 关闭系统 | 优雅退出 |
| `RunUserShellCommand` | 运行 Shell 命令 | 执行命令 |

### 7. MCP 协议操作
| Op 变体 | 说明 | 生成场景 |
|---------|------|----------|
| `ListMcpTools` | 列出 MCP 工具 | 查询工具 |
| `RefreshMcpServers` | 刷新 MCP 服务器 | 重新加载 |
| `ListCustomPrompts` | 列出自定义提示 | 查询提示 |
| `ListSkills` | 列出技能 | 查询技能 |

### 8. 模型管理操作
| Op 变体 | 说明 |
|---------|------|
| `ListModels` | 列出可用模型 |

### 9. 代理协作操作
| Op 变体 | 说明 |
|---------|------|
| `Handoff` | 移交到其他 Agent |

---

## Event (Event Queue) - 事件类型

Event 定义在 `src/protocol/event.rs`，分为以下几类：

### 1. 现有 Event (核心)
| Event 变体 | 触发时机 |
|------------|----------|
| `TurnStarted` | Turn 开始时 |
| `ModelStreaming` | 模型流式输出中 |
| `ModelComplete` | 模型完成响应 |
| `ToolCallRequested` | 请求工具调用 |
| `ToolCallResult` | 工具调用结果 |
| `ApprovalRequired` | 需要用户批准 |
| `HandoffInitiated` | Agent 移交发起 |
| `TurnComplete` | Turn 完成 |
| `Error` | 错误发生 |

### 2. 新增 Event (Codex 兼容)
| Event 变体 | 触发时机 |
|------------|----------|
| `SessionConfigured` | Session 配置完成 |
| `TurnAborted` | Turn 被中止 |
| `ContextCompacted` | 上下文已压缩 |
| `Warning` | 非致命警告 |
| `McpListToolsResponse` | MCP 工具列表响应 |
| `ListCustomPromptsResponse` | 自定义提示列表响应 |
| `ListSkillsResponse` | 技能列表响应 |
| `ThreadRolledBack` | 线程回滚完成 |
| `UndoPerformed` | 撤销完成 |
| `HistoryEntry` | 历史条目 |
| `RunUserShellCommand` | 运行用户命令 |

---

## 核心转换流程：submission_loop

`src/tasks/loop.rs` 中的 `submission_loop` 是核心处理入口：

```rust
pub async fn submission_loop(sess: Arc<Session>, mut rx_sub: mpsc::Receiver<Submission>) {
    while let Some(sub) = rx_sub.recv().await {
        match sub.op {
            Op::Interrupt => handle_interrupt(&sess).await,
            Op::UserTurn { .. } => handle_user_input_or_turn(&sess, sub.id, sub.op, ...).await,
            Op::Compact => { sess.spawn_task(Arc::clone(ctx), CompactTask).await; }
            Op::Shutdown => break,
            // ... 更多 Op 处理
        }
    }
}
```

### Op → Event 映射表

| Op 输入 | 处理函数 | 输出 Event |
|--------|----------|------------|
| `UserTurn` (Text) | `handle_user_input_or_turn` | `ModelStreaming { chunk }` |
| `UserTurn` (Image) | `handle_user_input_or_turn` | `Warning { message }` |
| `UserTurn` (File) | `handle_user_input_or_turn` | `Warning { message }` |
| `UserTurn` (Command) | `handle_user_input_or_turn` | `RunUserShellCommand { command }` |
| `ExecApproval(Approve)` | `handle_exec_approval` | `ToolCallResult { ... }` |
| `ExecApproval(Deny)` | `handle_exec_approval` | `Error { ... }` |
| `PatchApproval(Approve)` | `handle_patch_approval` | `ToolCallResult { ... }` |
| `ListMcpTools` | `handle_list_mcp_tools` | `McpListToolsResponse { tools }` |
| `Compact` | (spawn CompactTask) | `ContextCompacted { ... }` |
| `Undo` | `handle_undo` | `UndoPerformed { ... }` |
| `ThreadRollback` | `handle_thread_rollback` | `ThreadRolledBack { num_turns }` |
| `AddToHistory` | `handle_add_to_history` | `HistoryEntry { ... }` |
| `OverrideTurnContext` | `handle_override_turn_context` | `Warning { message }` |

---

## 数据流向图

```
┌─────────────────────────────────────────────────────────────────────────┐
│                              完整数据流                                  │
└─────────────────────────────────────────────────────────────────────────┘

  用户输入
     │
     ▼
  ┌─────────────┐
  │  Op::UserTurn│
  │  { items:   │
  │    Text,    │
  │    Image,   │
  │    File,    │
  │    Command  │
  │  }          │
  └──────┬──────┘
         │
         ▼
  ┌──────────────────────────────────┐
  │     Submission Queue (SQ)        │
  │   submission_loop 接收并路由     │
  └──────────────────────────────────┘
         │
         ▼
  ┌──────────────────────────────────┐
  │   handle_user_input_or_turn()    │
  │                                  │
  │   1. 创建/更新 TurnContext       │
  │   2. 处理每个 UserInputItem      │
  │   3. 发送对应的 Event            │
  └──────────────────────────────────┘
         │
         ▼
  ┌──────────────────────────────────┐
  │       Event Queue (EQ)           │
  │                                  │
  │   ┌──────────────────────────┐   │
  │   │ ModelStreaming           │   │
  │   │ ToolCallResult           │   │
  │   │ Warning                  │   │
  │   │ RunUserShellCommand      │   │
  │   │ ...                      │   │
  │   └──────────────────────────┘   │
  └──────────────────────────────────┘
         │
         ▼
  ┌──────────────────────────────────┐
  │      订阅者接收 Event            │
  │                                  │
  │   while let Some(event) =        │
  │       event_stream.recv().await  │
  │                                  │
  └──────────────────────────────────┘
```

---

## 关键文件位置

| 组件 | 文件路径 |
|------|----------|
| Op 定义 | `src/protocol/op.rs:108-222` |
| Event 定义 | `src/protocol/event.rs:11-69` |
| submission_loop | `src/tasks/loop.rs:31-121` |
| Session | `src/session/session.rs:131-328` |
| Agent | `src/agent/mod.rs:22-153` |
| Queue | `src/protocol/queue.rs` |

---

## 辅助函数

`src/protocol/op.rs` 提供了便捷构造函数：

```rust
// 创建简单的 UserTurn
user_turn(items, model)

// 创建带配置的 UserTurn
user_turn_with_config(items, model, cwd, approval_policy, sandbox_policy)

// 创建系统控制 Op
interrupt()
undo()
shutdown()
compact()
```

---

## 问题和建议

### 当前状态

1. ✅ **Op 枚举完整** - 20+ 操作类型
2. ✅ **Event 枚举完整** - Codex 兼容
3. ✅ **submission_loop 实现** - 核心路由逻辑
4. ⚠️ **RegularTask 未实现** - `src/tasks/regular.rs:26` 显示 `TODO`

### 建议

#### #1: 实现 RegularTask

`RegularTask::run()` 目前返回 `None`，需要实现完整的模型调用循环：

```rust
// src/tasks/regular.rs
async fn run(
    self: Arc<Self>,
    session: Arc<dyn TaskSession>,
    ctx: Arc<TurnContext>,
    cancellation_token: CancellationToken,
) -> Option<String> {
    // 1. 获取对话历史
    let history = session.history().await;

    // 2. 检查是否需要压缩
    if session.should_compact(ctx.context_window).await {
        session.compact_history(...).await;
    }

    // 3. 调用模型
    // 4. 处理工具调用
    // 5. 发送 Event
    // 6. 返回结果
}
```

#### #2: 完善 session_loop

`src/session/session.rs:449` 的 `session_loop` 目前处理有限，可以增强。

---

## Notes

- [ ] 理解 Op/Event 架构
- [ ] 实现 RegularTask
- [ ] 完善文档

---

## 相关测试文件

- `tests/session_tests.rs` - Session 测试
- `tests/protocol_tests.rs` - 协议测试
- `tests/codex_compat_tests.rs` - Codex 兼容测试

---
