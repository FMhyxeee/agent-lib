# Project Observation Record #013

**Date:** 2025-01-26
**Observer:** Claude (Agent)
**Status:** 📋 Specification Ready
**Topic:** Handoff 实现规范

---

## 任务概述

**优先级:** 🟡 中
**文件:** `src/tasks/loop.rs`

---

## Op 定义

```rust
// src/protocol/op.rs:213
Handoff {
    target_agent: String,
    context: Value,
}
```

---

## 需要实现的功能

### 1. 处理器函数

```rust
async fn handle_handoff(sess: &Session, target_agent: String, context: Value) {
    debug!(
        target_agent = %target_agent,
        context = ?context,
        "Handling handoff"
    );

    // 1. 验证目标 Agent 是否存在
    // 2. 传递当前状态到目标 Agent
    // 3. 发送移交事件
}
```

### 2. submission_loop 匹配

```rust
Op::Handoff { target_agent, context } => {
    handle_handoff(&sess, target_agent, context).await;
}
```

---

## 设计考虑

### Agent 注册表

需要一个 Agent 注册表来管理可用的 Agent：

```rust
// 新增: src/agent/registry.rs
pub struct AgentRegistry {
    agents: HashMap<String, Arc<dyn AgentInterface>>,
}

impl AgentRegistry {
    pub fn register(&mut self, name: String, agent: Arc<dyn AgentInterface>) {
        self.agents.insert(name, agent);
    }

    pub fn get(&self, name: &str) -> Option<&Arc<dyn AgentInterface>> {
        self.agents.get(name)
    }
}
```

### 上下文传递

```rust
#[async_trait]
pub trait AgentInterface: Send + Sync {
    async fn receive_handoff(&self, context: Value) -> AgentResult<()>;
    async fn get_state(&self) -> AgentResult<Value>;
}
```

---

## 伪代码

```rust
async fn handle_handoff(sess: &Session, target_agent: String, context: Value) {
    debug!(target_agent = %target_agent, "Handling handoff");

    // 1. 获取当前状态
    let current_state = sess.history().await;
    let state_json = serde_json::to_value(current_state).unwrap_or_else(|_| json!({}));

    // 2. 构建移交上下文
    let handoff_context = json!({
        "source": "current_session",
        "target": target_agent,
        "history": state_json,
        "user_context": context,
    });

    // 3. 发送移交事件
    sess.emit_event(Event::HandoffInitiated {
        from: "current_session".to_string(),
        to: target_agent.clone(),
    }).await;

    // 4. 通知目标 Agent (如果有注册表)
    // if let Some(target) = AGENT_REGISTRY.get(&target_agent) {
    //     target.receive_handoff(handoff_context).await?;
    // }

    sess.emit_event(Event::TurnComplete).await;
}
```

---

## Event 扩展

当前 Event 已存在：
```rust
Event::HandoffInitiated {
    from: String,
    to: String,
}
```

可能需要添加：
```rust
Event::HandoffCompleted {
    from: String,
    to: String,
    context: Value,
}
```

---

## 测试用例

```rust
#[tokio::test]
async fn test_handoff_event() {
    let (session, handle) = Session::new(64);
    handle.submit(Op::Handoff {
        target_agent: "code_reviewer".to_string(),
        context: json!({"task": "review this code"}),
    }).await;

    let event = handle.next_event().await;
    assert!(matches!(event, Event::HandoffInitiated { .. }));
}
```

---

## 相关文件

- `src/tasks/loop.rs` - 添加处理器
- `src/protocol/op.rs:213` - Op 定义已存在
- `src/protocol/event.rs` - HandoffInitiated Event 已存在
- 可能需要新建 `src/agent/registry.rs` - Agent 注册表

---

**规范完成，等待实现。**

---
