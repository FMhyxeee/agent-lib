# Project Observation Record #010

**Date**: 2025-01-26
**Observer**: Claude (Agent)
**Status**: 🟡 In Progress - Feature Implementation
**Topic:** 缺失功能任务清单

---

## Op 处理器实现状态

### ✅ 已实现的 Op (14 个)

| Op | 处理器 | 状态 |
|----|--------|------|
| `Interrupt` | `handle_interrupt` | ✅ 完整 |
| `OverrideTurnContext` | `handle_override_turn_context` | ✅ 完整 |
| `UserTurn` | `handle_user_input_or_turn` | ✅ 完整 |
| `UserInputLegacy` | `handle_user_input_or_turn` | ✅ 完整 |
| `ExecApproval` | `handle_exec_approval` | ✅ 完整 |
| `PatchApproval` | `handle_patch_approval` | ✅ 完整 |
| `Compact` | `CompactTask` | ✅ 完整 |
| `Shutdown` | (break loop) | ✅ 完整 |
| `ListMcpTools` | `handle_list_mcp_tools` | ✅ 完整 |
| `RefreshMcpServers` | `handle_refresh_mcp_servers` | ⚠️ TODO |
| `Undo` | `handle_undo` | ✅ 完整 |
| `ThreadRollback` | `handle_thread_rollback` | ✅ 完整 |
| `AddToHistory` | `handle_add_to_history` | ✅ 完整 |
| `RunUserShellCommand` | (emit Event) | ⚠️ 部分实现 |

---

### ❌ 未实现的 Op (9 个)

| Op | 优先级 | 复杂度 | 说明 |
|----|--------|--------|------|
| `StartTurn` | 低 | 低 | 被 UserTurn 覆盖 |
| `UserInput` | 低 | 低 | 被 UserTurn 覆盖 |
| `UserInputAnswer` | 中 | 中 | 用户输入响应 |
| `ApprovalResponse` | 中 | 低 | 工具批准响应 |
| `Handoff` | 中 | 中 | Agent 移交 |
| `GetHistoryEntryRequest` | 低 | 低 | 历史条目查询 |
| `ListCustomPrompts` | 低 | 低 | 自定义提示列表 |
| `ListSkills` | 低 | 低 | 技能列表 |
| `ListModels` | 低 | 低 | 模型列表 |
| `Review` | 中 | 中 | 代码审查 |

---

## 实现任务清单

### 任务 #1: RunUserShellCommand 完整实现

**优先级: 高** 🔴
**文件:** `src/tasks/loop.rs`

当前状态：只发送 Event，不执行命令

**需要实现:**
```rust
async fn handle_run_user_shell_command(sess: &Session, command: String) {
    debug!("Handling run user shell command: {}", command);

    // 1. 解析命令
    // 2. 在沙箱中执行
    // 3. 捕获输出
    // 4. 发送 Event

    sess.emit_event(Event::RunUserShellCommand {
        command: command.clone(),
        output,
        exit_code,
    }).await;
}
```

---

### 任务 #2: ApprovalResponse 处理器

**优先级: 中** 🟡
**文件:** `src/tasks/loop.rs`

```rust
Op::ApprovalResponse { request_id, approved } => {
    handle_approval_response(&sess, request_id, approved).await;
}
```

---

### 任务 #3: Handoff 处理器

**优先级: 中** 🟡
**文件:** `src/tasks/loop.rs`

```rust
Op::Handoff { target_agent, context } => {
    handle_handoff(&sess, target_agent, context).await;
}
```

需要实现 Agent 之间的状态移交。

---

### 任务 #4: RefreshMcpServers 完整实现

**优先级: 中** 🟡
**文件:** `src/tasks/loop.rs:401`

当前状态：有 TODO

**需要实现:**
```rust
async fn handle_refresh_mcp_servers(sess: &Session, config: McpServerRefreshConfig) {
    debug!(force = config.force_reload, "Handling refresh MCP servers");

    if let Some(manager) = sess.get_mcp_manager() {
        if config.force_reload {
            // 重新加载所有服务器
            manager.reload_all().await;
        } else {
            // 检查并刷新不健康的连接
            manager.refresh_unhealthy().await;
        }
    }

    sess.emit_event(Event::Warning {
        message: "MCP servers refreshed".to_string(),
    }).await;
}
```

---

### 任务 #5: UserInputAnswer 处理器

**优先级: 中** 🟡
**文件:** `src/tasks/loop.rs`

```rust
Op::UserInputAnswer { id, response } => {
    handle_user_input_answer(&sess, id, response).await;
}
```

---

### 任务 #6: GetHistoryEntryRequest 处理器

**优先级: 低** 🟢
**文件:** `src/tasks/loop.rs`

```rust
Op::GetHistoryEntryRequest { offset, log_id } => {
    handle_get_history_entry_request(&sess, offset, log_id).await;
}
```

---

### 任务 #7: ListSkills 处理器

**优先级: 低** 🟢
**文件:** `src/tasks/loop.rs`

```rust
Op::ListSkills { cwds, force_reload } => {
    handle_list_skills(&sess, cwds, force_reload).await;
}
```

---

### 任务 #8: ListCustomPrompts 处理器

**优先级: 低** 🟢
**文件:** `src/tasks/loop.rs`

```rust
Op::ListCustomPrompts => {
    handle_list_custom_prompts(&sess).await;
}
```

---

### 任务 #9: ListModels 处理器

**优先级: 低** 🟢
**文件:** `src/tasks/loop.rs`

```rust
Op::ListModels => {
    handle_list_models(&sess).await;
}
```

---

### 任务 #10: Review 处理器

**优先级: 中** 🟡
**文件:** `src/tasks/loop.rs`

```rust
Op::Review { review_request } => {
    handle_review(&sess, review_request).await;
}
```

---

## 实现顺序建议

### 阶段 1: 高优先级 (必要)

1. **RunUserShellCommand** - 执行用户命令
2. **ApprovalResponse** - 工具批准响应
3. **RefreshMcpServers** - MCP 服务器刷新

### 阶段 2: 中优先级 (推荐)

4. **Handoff** - Agent 移交
5. **UserInputAnswer** - 用户输入响应
6. **Review** - 代码审查

### 阶段 3: 低优先级 (可选)

7. **GetHistoryEntryRequest** - 历史查询
8. **ListSkills** - 技能列表
9. **ListCustomPrompts** - 提示列表
10. **ListModels** - 模型列表

---

## TODO 汇总

| 位置 | TODO 内容 |
|------|----------|
| `src/tasks/loop.rs:403` | 实现刷新 MCP 服务器逻辑 |

---

**开始实现?** 请告诉我从哪个任务开始。

---
