# Project Observation Record #015

**Date:** 2025-01-26
**Observer:** Claude (Agent)
**Status:** 📋 Specification Ready
**Topic:** 低优先级 Op 实现规范

---

## 概述

本文档包含低优先级 Op 的实现规范。这些功能不是核心流程必需的，可以在后续版本中实现。

---

## 任务 #5: UserInputAnswer

**优先级:** 🟡 中

### Op 定义

```rust
// src/protocol/op.rs:141
UserInputAnswer {
    id: String,
    response: UserInputResponse,
}
```

### 处理器

```rust
async fn handle_user_input_answer(
    sess: &Session,
    id: String,
    response: UserInputResponse,
) {
    debug!(id = %id, response = ?response, "Handling user input answer");

    // 将响应添加到历史
    match response {
        UserInputResponse::Text { text } => {
            sess.emit_event(Event::ModelStreaming { chunk: text }).await;
        }
        UserInputResponse::File { path } => {
            sess.emit_event(Event::Warning {
                message: format!("File response: {:?}", path),
            }).await;
        }
    }

    sess.emit_event(Event::TurnComplete).await;
}
```

### submission_loop 匹配

```rust
Op::UserInputAnswer { id, response } => {
    handle_user_input_answer(&sess, id, response).await;
}
```

---

## 任务 #6: GetHistoryEntryRequest

**优先级:** 🟢 低

### Op 定义

```rust
// src/protocol/op.rs:175
GetHistoryEntryRequest {
    offset: usize,
    log_id: u64,
}
```

### 处理器

```rust
async fn handle_get_history_entry_request(sess: &Session, offset: usize, log_id: u64) {
    debug!(offset, log_id, "Handling get history entry request");

    let history = sess.history().await;

    let entry = history.all().get(offset).and_then(|msg| {
        Some(crate::protocol::HistoryEntry {
            offset,
            log_id,
            entry: msg.content.clone(),
            role: msg.role.clone(),
        })
    });

    if let Some(entry) = entry {
        sess.emit_event(Event::HistoryEntry { entry }).await;
    } else {
        sess.emit_event(Event::Error {
            error: AgentError::Session(format!("No entry at offset {}", offset)),
        }).await;
    }
}
```

### submission_loop 匹配

```rust
Op::GetHistoryEntryRequest { offset, log_id } => {
    handle_get_history_entry_request(&sess, offset, log_id).await;
}
```

---

## 任务 #7: ListSkills

**优先级:** 🟢 低

### Op 定义

```rust
// src/protocol/op.rs:202
ListSkills {
    cwds: Vec<PathBuf>,
    force_reload: bool,
}
```

### 处理器

```rust
async fn handle_list_skills(sess: &Session, cwds: Vec<PathBuf>, force_reload: bool) {
    debug!(cwds = ?cwds, force_reload, "Handling list skills");

    // 技能是预定义的代码片段或工具集合
    // 从指定目录扫描技能文件

    let skills = if cwds.is_empty() {
        // 使用默认目录
        vec![]
    } else {
        // 扫描指定目录
        let mut found = vec![];
        for cwd in &cwds {
            // TODO: 扫描目录获取技能
            let skills_in_dir = scan_skills_dir(cwd).await.unwrap_or_default();
            found.extend(skills_in_dir);
        }
        found
    };

    sess.emit_event(Event::ListSkillsResponse {
        skills,
    }).await;
}

async fn scan_skills_dir(dir: &PathBuf) -> AgentResult<Vec<SkillEntry>> {
    // 扫描目录中的技能定义文件
    Ok(vec![])
}
```

### submission_loop 匹配

```rust
Op::ListSkills { cwds, force_reload } => {
    handle_list_skills(&sess, cwds, force_reload).await;
}
```

---

## 任务 #8: ListCustomPrompts

**优先级:** 🟢 低

### Op 定义

```rust
// src/protocol/op.rs:200
ListCustomPrompts
```

### 处理器

```rust
async fn handle_list_custom_prompts(sess: &Session) {
    debug!("Handling list custom prompts");

    // 自定义提示是预定义的系统提示模板
    let prompts = vec![
        CustomPromptInfo {
            id: "code_review".to_string(),
            name: "Code Review".to_string(),
            description: "Review code for bugs and improvements".to_string(),
        },
        CustomPromptInfo {
            id: "debug_helper".to_string(),
            name: "Debug Helper".to_string(),
            description: "Help debug code issues".to_string(),
        },
    ];

    sess.emit_event(Event::ListCustomPromptsResponse {
        prompts,
    }).await;
}
```

### submission_loop 匹配

```rust
Op::ListCustomPrompts => {
    handle_list_custom_prompts(&sess).await;
}
```

---

## 任务 #9: ListModels

**优先级:** 🟢 低

### Op 定义

```rust
// src/protocol/op.rs:209
ListModels
```

### 处理器

```rust
async fn handle_list_models(sess: &Session) {
    debug!("Handling list models");

    // 返回支持的模型列表
    let models = vec![
        ModelInfo {
            id: "gpt-4o".to_string(),
            name: "GPT-4o".to_string(),
            provider: "openai".to_string(),
            context_window: 128000,
        },
        ModelInfo {
            id: "GLM-4-Flash".to_string(),
            name: "GLM-4-Flash".to_string(),
            provider: "glm".to_string(),
            context_window: 128000,
        },
    ];

    sess.emit_event(Event::ListModelsResponse {
        models,
    }).await;
}
```

### submission_loop 匹配

```rust
Op::ListModels => {
    handle_list_models(&sess).await;
}
```

### Event 扩展

需要添加新的 Event：
```rust
// src/protocol/event.rs
ListModelsResponse {
    models: Vec<ModelInfo>,
}
```

---

## 任务 #10: Review

**优先级:** 🟡 中

### Op 定义

```rust
// src/protocol/op.rs:186
Review {
    review_request: ReviewRequest,
}
```

### 处理器

```rust
async fn handle_review(sess: &Session, review_request: ReviewRequest) {
    debug!(request = ?review_request, "Handling review request");

    match review_request {
        ReviewRequest::Exec { code } => {
            // 代码执行审查
            sess.emit_event(Event::ApprovalRequired {
                request_id: uuid::Uuid::new_v4().to_string(),
                tool: "exec".to_string(),
                args: json!({ "code": code }),
            }).await;
        }
        ReviewRequest::Patch { diff } => {
            // 补丁审查
            sess.emit_event(Event::ApprovalRequired {
                request_id: uuid::Uuid::new_v4().to_string(),
                tool: "patch".to_string(),
                args: json!({ "diff": diff }),
            }).await;
        }
    }
}
```

### submission_loop 匹配

```rust
Op::Review { review_request } => {
    handle_review(&sess, review_request).await;
}
```

---

## 总结

| 任务 | 优先级 | 复杂度 | 新增 Event |
|------|--------|--------|------------|
| UserInputAnswer | 🟡 中 | 低 | 无 |
| GetHistoryEntryRequest | 🟢 低 | 低 | 无 (已存在) |
| ListSkills | 🟢 低 | 中 | 无 (已存在) |
| ListCustomPrompts | 🟢 低 | 低 | 无 (已存在) |
| ListModels | 🟢 低 | 低 | **需要新增** |
| Review | 🟡 中 | 中 | 无 (已存在) |

---

**规范完成，等待实现。**

---
