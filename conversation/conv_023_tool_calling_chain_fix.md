# Project Observation Record #023

**Date**: 2025-01-27
**Observer**: Claude (Agent)
**Status:** ✅ Critical Fixes Applied
**Topic:** 工具调用链路完整修复

---

## 🔴 发现的关键问题

在完整的工具调用链路 review 中，发现消息映射存在严重问题，会导致工具调用功能无法正常工作。

---

## 问题分析

### 问题 1: GLM Provider - Tool/Assistant 消息不完整

**位置:** `src/model/provider/glm.rs:47-51`

**原代码:**
```rust
#[derive(Debug, Serialize)]
struct GlmMessage {
    role: String,
    content: String,
}
```

**问题:**
- 缺少 `tool_call_id` 字段 (Tool 角色需要)
- 缺少 `tool_calls` 字段 (Assistant 角色带工具调用时需要)

**影响:**
- 工具结果消息无法关联到原始工具调用
- 模型无法知道哪个工具调用了什么函数

---

### 问题 2: OpenAI Provider - 消息类型错误映射

**位置:** `src/model/provider/openai.rs:63-66`

**原代码:**
```rust
_ => ChatCompletionRequestMessage::User(ChatCompletionRequestUserMessage {
    content: ChatCompletionRequestUserMessageContent::Text(msg.content),
    name: None,
}),
```

**问题:**
- 将所有非 System 消息映射为 User 角色
- Assistant 和 Tool 角色消息被错误处理
- 工具调用循环会中断

---

## 修复方案

### GLM Provider 修复

**1. 扩展 `GlmMessage` 结构:**
```rust
#[derive(Debug, Serialize)]
struct GlmMessage {
    role: String,
    content: String,
    /// 工具调用 ID (仅 tool 角色使用)
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_call_id: Option<String>,
    /// 工具调用列表 (仅 assistant 角色使用)
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_calls: Option<Vec<GlmToolCallRequest>>,
}
```

**2. 新增请求时使用的工具调用格式:**
```rust
#[derive(Debug, Serialize)]
struct GlmToolCallRequest {
    id: String,
    r#type: String,
    function: GlmToolCallFunctionRequest,
}

#[derive(Debug, Serialize)]
struct GlmToolCallFunctionRequest {
    name: String,
    arguments: String,
}
```

**3. 更新 `map_message` 函数:**
```rust
fn map_message(message: Message) -> GlmMessage {
    match message.role {
        MessageRole::Tool => {
            // 工具结果消息需要 tool_call_id
            GlmMessage {
                role: "tool".to_string(),
                content: message.content,
                tool_call_id: message.tool_call_id,
                tool_calls: None,
            }
        }
        MessageRole::Assistant => {
            // 助手消息可能包含工具调用
            if let Some(tool_calls) = message.tool_calls {
                // ... 转换为 GlmToolCallRequest
                GlmMessage {
                    role: "assistant".to_string(),
                    content: message.content,
                    tool_call_id: None,
                    tool_calls: Some(request_tool_calls),
                }
            } else {
                // 普通助手消息
                // ...
            }
        }
        // ...
    }
}
```

---

### OpenAI Provider 修复

**完整的消息映射:**
```rust
let converted: Vec<ChatCompletionRequestMessage> = messages
    .into_iter()
    .map(|msg| match msg.role {
        MessageRole::System => { /* System 消息 */ }
        MessageRole::User => { /* User 消息 */ }
        MessageRole::Assistant => {
            if let Some(tool_calls) = msg.tool_calls {
                // 助手消息 + 工具调用
                ChatCompletionRequestMessage::Assistant(
                    ChatCompletionRequestAssistantMessage {
                        content: Some(...),
                        refusal: None,
                        tool_calls: Some(openai_tool_calls),
                        name: None,
                        function_call: None,
                    },
                )
            } else {
                // 普通助手消息
                // ...
            }
        }
        MessageRole::Tool => {
            // 工具结果消息
            ChatCompletionRequestMessage::Tool(ChatCompletionRequestToolMessage {
                content: ChatCompletionRequestToolMessageContent::Text(msg.content),
                tool_call_id: msg.tool_call_id.unwrap_or_default(),
            })
        }
    })
    .collect();
```

---

## 修复后的完整调用链

```
┌─────────────────────────────────────────────────────────────────────────┐
│                        工具调用完整流程                                  │
├─────────────────────────────────────────────────────────────────────────┤
│                                                                         │
│  1. 用户输入 → RegularTask                                              │
│     ↓                                                                  │
│  2. list_tools() → 获取可用工具定义 (Vec<ToolDef>)                      │
│     ↓                                                                  │
│  3. chat_model(messages, tools)                                        │
│     ↓                                                                  │
│  4. Provider 转换消息:                                                 │
│     ├─ Message::User → {"role": "user", "content": "..."}              │
│     ├─ Message::Assistant + tool_calls →                               │
│     │   {"role": "assistant", "content": "...", "tool_calls": [...]}   │
│     └─ Message::Tool → {"role": "tool", "tool_call_id": "xxx", ...}   │
│     ↓                                                                  │
│  5. API 调用返回 ModelResponse { content, tool_calls }                │
│     ↓                                                                  │
│  6. 如果有 tool_calls:                                                 │
│     a. 创建 Message::assistant_with_calls(content, tool_calls)         │
│     b. execute_tool(name, args) → ToolResult                           │
│     c. 应用输出截断 (max_size: 50,000 chars)                           │
│     d. 创建 Message::tool_result(tool_call_id, result)                 │
│     e. 添加到消息列表                                                   │
│     f. 再次调用模型 (回到步骤 4)                                        │
│     ↓                                                                  │
│  7. 循环直到没有更多 tool_calls (最多 10 次)                            │
│     ↓                                                                  │
│  8. 发送最终响应                                                         │
│                                                                         │
└─────────────────────────────────────────────────────────────────────────┘
```

---

## 修复的文件

| 文件 | 修改内容 |
|------|----------|
| `src/model/provider/glm.rs` | + `tool_call_id`, `tool_calls` 字段 |
| `src/model/provider/glm.rs` | + `GlmToolCallRequest` 结构 |
| `src/model/provider/glm.rs` | 更新 `map_message` 处理所有角色 |
| `src/model/provider/openai.rs` | 完整重写消息映射逻辑 |

---

## 测试结果

| 检查项 | 结果 |
|--------|------|
| 编译 | ✅ 通过 |
| 单元测试 | ✅ 76/78 通过 (2 忽略) |
| 消息序列化 | ✅ 正确 |
| 工具调用解析 | ✅ 正确 |

---

## 总结

| 组件 | 修复前 | 修复后 |
|------|--------|--------|
| GLM 消息映射 | ❌ 缺少 tool_call_id/tool_calls | ✅ 完整支持 |
| OpenAI 消息映射 | ❌ 错误映射所有为 User | ✅ 正确区分所有角色 |
| 工具调用循环 | ⚠️ 代码完整但 API 映射错误 | ✅ 完全可用 |

**工具调用链路现已完整修复！**

---

# done
