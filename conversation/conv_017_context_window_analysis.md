# Project Observation Record #017

**Date:** 2025-01-26
**Observer:** Claude (Agent)
**Status:** 📋 Specification Ready
**Topic:** Context Window 获取机制分析与改进方案

---

## 问题

Compact 功能需要知道模型的 context window 大小，当前项目是如何获取的？

---

## 当前实现方式

### 1. TurnContext.context_window 字段

```rust
// src/session/context.rs:49-50
/// 上下文窗口大小
pub context_window: usize,
```

**默认值:** 128000 tokens

### 2. Compact 判断逻辑

```rust
// src/tasks/regular.rs:49-51
let context_window = ctx.context_window;
if total_tokens > context_window {
    let keep_recent = ((context_window as f32) * 0.7) as usize;
    session.compact_history(keep_recent, summary).await;
}
```

### 3. 设置方式

| 方式 | 代码 |
|------|------|
| Builder 模式 | `TurnContext::new("gpt-4").with_context_window(128000)` |
| 直接设置 | `ctx.context_window = 200000` |
| 使用默认 | `TurnContext::default()` → 128000 |

---

## 当前架构图

```
┌─────────────────────────────────────────────────────────────┐
│                    Context Window 流程                      │
├─────────────────────────────────────────────────────────────┤
│                                                             │
│   用户/代码                          TurnContext             │
│      │                                    │                │
│      │  1. 手动设置                        │                │
│      └─────────────────────>  context_window: usize        │
│                                    (默认: 128000)          │
│                                         │                  │
│                                         ▼                  │
│                                  RegularTask                │
│                                         │                  │
│                                         │  2. 比较          │
│                                         ▼                  │
│                                   total_tokens > ?         │
│                                         │                  │
│                                         │  3. 超限则压缩    │
│                                         ▼                  │
│                                   compact_history(          │
│                                     context_window * 0.7    │
│                                   )                         │
└─────────────────────────────────────────────────────────────┘
```

---

## 问题分析

### ❌ 当前的问题

| 问题 | 说明 |
|------|------|
| 手动设置 | 需要在代码中硬编码 context_window |
| 不灵活 | 更换模型需要手动修改配置 |
| 易出错 | 可能设置错误的 context window |
| 无验证 | 没有检查模型名称和 context window 是否匹配 |

### 示例问题

```rust
// 问题 1: 手动设置容易出错
let ctx = TurnContext::new("gpt-4o-mini")
    .with_context_window(128000);  // 如果换成其他模型呢？

// 问题 2: 模型名称和 context window 不匹配
let ctx = TurnContext::new("gpt-3.5-turbo")  // 实际只有 16K
    .with_context_window(128000);  // 但设置了 128K ❌
```

---

## 改进方案

### 方案 1: 在 ModelClient trait 中添加方法

**优先级:** 低
**优点:** 简单直接
**缺点:** 每个 Provider 都需要实现

```rust
// src/model/client.rs
pub trait ModelClient: Send + Sync {
    async fn chat(&self, ...) -> AgentResult<ModelResponse>;
    async fn chat_stream(&self, ...) -> AgentResult<...>;

    /// 获取模型的上下文窗口大小
    fn context_window(&self) -> usize {
        128000  // 默认值
    }

    /// 获取模型名称
    fn model_name(&self) -> &str;
}
```

```rust
// src/model/provider/glm.rs
impl ModelClient for GlmProvider {
    fn context_window(&self) -> usize {
        match self.model.as_str() {
            "GLM-4-Flash" => 128000,
            "GLM-4" => 128000,
            "GLM-4-Air" => 128000,
            "GLM-4-Plus" => 128000,
            _ => 128000,
        }
    }

    fn model_name(&self) -> &str {
        &self.model
    }
}
```

---

### 方案 2: 模型信息注册表 (推荐)

**优先级:** 高
**优点:** 集中管理，易于扩展
**缺点:** 需要新建模块

```rust
// 新建: src/model/registry.rs

use std::collections::HashMap;
use serde::{Deserialize, Serialize};

/// 模型信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelInfo {
    pub name: String,
    pub display_name: String,
    pub context_window: usize,
    pub supports_streaming: bool,
    pub supports_tools: bool,
    pub supports_vision: bool,
    pub provider: String,
}

/// 模型注册表
#[derive(Debug, Clone)]
pub struct ModelRegistry {
    models: HashMap<String, ModelInfo>,
}

impl ModelRegistry {
    pub fn new() -> Self {
        let mut models = HashMap::new();

        // === OpenAI 模型 ===
        models.insert("gpt-4o".to_string(), ModelInfo {
            name: "gpt-4o".to_string(),
            display_name: "GPT-4o".to_string(),
            context_window: 128000,
            supports_streaming: true,
            supports_tools: true,
            supports_vision: true,
            provider: "openai".to_string(),
        });

        models.insert("gpt-4o-mini".to_string(), ModelInfo {
            name: "gpt-4o-mini".to_string(),
            display_name: "GPT-4o Mini".to_string(),
            context_window: 128000,
            supports_streaming: true,
            supports_tools: true,
            supports_vision: true,
            provider: "openai".to_string(),
        });

        models.insert("gpt-4-turbo".to_string(), ModelInfo {
            name: "gpt-4-turbo".to_string(),
            display_name: "GPT-4 Turbo".to_string(),
            context_window: 128000,
            supports_streaming: true,
            supports_tools: true,
            supports_vision: true,
            provider: "openai".to_string(),
        });

        // === GLM 模型 ===
        models.insert("GLM-4-Flash".to_string(), ModelInfo {
            name: "GLM-4-Flash".to_string(),
            display_name: "GLM-4 Flash".to_string(),
            context_window: 128000,
            supports_streaming: true,
            supports_tools: true,
            supports_vision: true,
            provider: "glm".to_string(),
        });

        models.insert("GLM-4".to_string(), ModelInfo {
            name: "GLM-4".to_string(),
            display_name: "GLM-4".to_string(),
            context_window: 128000,
            supports_streaming: true,
            supports_tools: true,
            supports_vision: false,
            provider: "glm".to_string(),
        });

        models.insert("GLM-4-Air".to_string(), ModelInfo {
            name: "GLM-4-Air".to_string(),
            display_name: "GLM-4 Air".to_string(),
            context_window: 128000,
            supports_streaming: true,
            supports_tools: true,
            supports_vision: false,
            provider: "glm".to_string(),
        });

        models.insert("GLM-4-Plus".to_string(), ModelInfo {
            name: "GLM-4-Plus".to_string(),
            display_name: "GLM-4 Plus".to_string(),
            context_window: 128000,
            supports_streaming: true,
            supports_tools: true,
            supports_vision: true,
            provider: "glm".to_string(),
        });

        // === Anthropic 模型 ===
        models.insert("claude-3-5-sonnet-20241022".to_string(), ModelInfo {
            name: "claude-3-5-sonnet-20241022".to_string(),
            display_name: "Claude 3.5 Sonnet".to_string(),
            context_window: 200000,
            supports_streaming: true,
            supports_tools: true,
            supports_vision: true,
            provider: "anthropic".to_string(),
        });

        Self { models }
    }

    /// 获取模型信息
    pub fn get(&self, model: &str) -> Option<&ModelInfo> {
        self.models.get(model)
    }

    /// 获取 context window
    pub fn get_context_window(&self, model: &str) -> usize {
        self.models.get(model)
            .map(|info| info.context_window)
            .unwrap_or(128000)  // 默认值
    }

    /// 列出所有模型
    pub fn list_models(&self) -> Vec<&ModelInfo> {
        self.models.values().collect()
    }

    /// 按提供商过滤
    pub fn list_by_provider(&self, provider: &str) -> Vec<&ModelInfo> {
        self.models.values()
            .filter(|m| m.provider == provider)
            .collect()
    }

    /// 注册新模型
    pub fn register(&mut self, info: ModelInfo) {
        self.models.insert(info.name.clone(), info);
    }
}

impl Default for ModelRegistry {
    fn default() -> Self {
        Self::new()
    }
}

// 全局单例
use std::sync::OnceLock;
static REGISTRY: OnceLock<ModelRegistry> = OnceLock::new();

pub fn get_registry() -> &'static ModelRegistry {
    REGISTRY.get_or_init(|| ModelRegistry::new())
}

pub fn get_context_window(model: &str) -> usize {
    get_registry().get_context_window(model)
}
```

---

### 方案 3: 自动从 TurnContext 获取

```rust
// src/session/context.rs
impl TurnContext {
    /// 获取模型的 context window
    /// 优先使用注册表查询，其次使用字段值
    pub fn get_effective_context_window(&self) -> usize {
        // 方案 A: 使用注册表
        // crate::model::registry::get_context_window(&self.model)

        // 方案 B: 当前直接返回字段值
        self.context_window
    }

    /// 根据模型名称自动设置 context window
    pub fn with_auto_context_window(mut self) -> Self {
        self.context_window = crate::model::registry::get_context_window(&self.model);
        self
    }
}
```

---

## 使用示例

### 当前方式 (手动)

```rust
let ctx = TurnContext::new("gpt-4o-mini")
    .with_context_window(128000);  // 需要手动查文档
```

### 改进后方式 (自动)

```rust
// 方式 1: 使用注册表
let ctx = TurnContext::new("gpt-4o-mini")
    .with_auto_context_window();  // 自动从注册表获取

// 方式 2: 直接查询
let cw = get_context_window("gpt-4o-mini");  // 返回 128000

// 方式 3: 列出所有可用模型
let models = get_registry().list_models();
for model in models {
    println!("{}: {} tokens", model.display_name, model.context_window);
}
```

---

## 实现步骤

### 阶段 1: 创建 ModelRegistry

1. 新建 `src/model/registry.rs`
2. 定义 `ModelInfo` 结构
3. 实现 `ModelRegistry`
4. 添加常见模型数据

### 阶段 2: 集成到 TurnContext

1. 添加 `with_auto_context_window()` 方法
2. 添加 `get_effective_context_window()` 方法

### 阶段 3: 更新现有代码

1. 更新 `RegularTask` 使用自动获取
2. 更新 `SessionConfig` 默认值
3. 添加测试

---

## 相关文件

| 文件 | 操作 |
|------|------|
| `src/model/registry.rs` | **新建** |
| `src/model/mod.rs` | 添加 `pub mod registry;` |
| `src/session/context.rs` | 添加 `with_auto_context_window()` |
| `src/tasks/regular.rs` | 使用自动获取 |
| `tests/model_registry_test.rs` | **新建** 测试 |

---

## 常见模型 Context Window 参考

| 模型 | Context Window |
|------|---------------|
| **OpenAI** | |
| gpt-4o | 128K |
| gpt-4o-mini | 128K |
| gpt-4-turbo | 128K |
| gpt-3.5-turbo | 16K |
| **Anthropic** | |
| claude-3-5-sonnet | 200K |
| claude-3-opus | 200K |
| **GLM (智谱)** | |
| GLM-4-Flash | 128K |
| GLM-4 | 128K |
| GLM-4-Air | 128K |
| GLM-4-Plus | 128K |

---

## 总结

| 方面 | 当前状态 | 建议状态 |
|------|----------|----------|
| Context Window 获取 | 手动设置 | 自动从注册表获取 |
| 默认值 | 128000 (硬编码) | 根据模型名称查询 |
| 可扩展性 | 需要修改代码 | 添加到注册表即可 |
| 错误风险 | 高 (手动可能填错) | 低 (自动查询) |

---

**规范完成，等待实现。**

---
