use std::pin::Pin;

use async_trait::async_trait;
use futures::Stream;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::error::{AgentError, AgentResult};
use crate::model::Message;
use crate::tools::ToolDef;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TokenUsage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
}

/// 工具调用信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    /// 调用 ID (用于关联工具结果)
    pub id: String,
    /// 工具名称
    pub name: String,
    /// 工具参数 (JSON 对象)
    pub arguments: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelResponse {
    /// 响应文本内容
    pub content: String,
    /// Token 使用情况
    pub usage: TokenUsage,
    /// 工具调用列表 (如果有)
    pub tool_calls: Vec<ToolCall>,
}

impl ModelResponse {
    /// 检查是否有工具调用
    pub fn has_tool_calls(&self) -> bool {
        !self.tool_calls.is_empty()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamChunk {
    pub delta: String,
}

/// ModelClient - LLM 提供商的核心接口
///
/// 此 trait 定义了与不同大语言模型提供商交互的统一接口。
/// 实现此 trait 以支持新的模型提供商（如 OpenAI、GLM、Anthropic 等）。
///
/// # 设计原则
///
/// - **提供商标识**: 每个提供商有其独特的配置和API
/// - **统一接口**: 所有提供商实现相同的方法签名
/// - **流式支持**: 支持流式响应以获得更好的用户体验
/// - **工具调用**: 支持函数调用能力
///
/// # 示例
///
/// ```rust,ignore
/// use agent_lib::model::{ModelClient, Message, ModelResponse, ToolDef};
/// use agent_lib::error::AgentResult;
/// use futures::stream;
///
/// struct MyProvider;
///
/// #[async_trait::async_trait]
/// impl ModelClient for MyProvider {
///     async fn chat(&self, messages: Vec<Message>, tools: Vec<ToolDef>) -> AgentResult<ModelResponse> {
///         // 实现与模型 API 的交互
///         Ok(ModelResponse {
///             content: "Hello!".to_string(),
///             usage: TokenUsage::default(),
///             tool_calls: vec![],
///         })
///     }
///
///     async fn chat_stream(&self, messages: Vec<Message>, tools: Vec<ToolDef>) -> AgentResult<Stream> {
///         // 实现流式响应
///         Ok(Box::pin(stream::empty()))
///     }
/// }
/// ```
#[async_trait]
pub trait ModelClient: Send + Sync {
    /// 发送聊天请求并获取完整响应
    ///
    /// # 参数
    ///
    /// * `messages` - 对话历史消息列表
    /// * `tools` - 可用的工具定义列表
    ///
    /// # 返回
    ///
    /// - `Ok(ModelResponse)` - 包含响应内容、Token使用和工具调用
    /// - `Err(AgentError)` - 请求失败
    ///
    /// # 示例
    ///
    /// ```rust,ignore
    /// let response = model.chat(messages, tools).await?;
    /// println!("Model said: {}", response.content);
    /// ```
    async fn chat(&self, messages: Vec<Message>, tools: Vec<ToolDef>)
    -> AgentResult<ModelResponse>;

    /// 发送聊天请求并获取流式响应
    ///
    /// 此方法用于实时流式输出模型响应，提供更好的用户体验。
    ///
    /// # 参数
    ///
    /// * `messages` - 对话历史消息列表
    /// * `tools` - 可用的工具定义列表
    ///
    /// # 返回
    ///
    /// - `Ok(Stream)` - 产生流式数据块的流
    /// - `Err(AgentError)` - 请求失败
    ///
    /// # 流式处理
    ///
    /// 流会产生 `StreamChunk`，每个块包含增量文本。
    /// 消费者应该按顺序处理这些块。
    ///
    /// # 示例
    ///
    /// ```rust,ignore
    /// let mut stream = model.chat_stream(messages, tools).await?;
    /// while let Some(chunk) = stream.next().await {
    ///     print!("{}", chunk.delta);
    /// }
    /// ```
    async fn chat_stream(
        &self,
        messages: Vec<Message>,
        tools: Vec<ToolDef>,
    ) -> AgentResult<Pin<Box<dyn Stream<Item = StreamChunk> + Send>>>;
}

pub fn not_implemented_client(name: &str) -> AgentError {
    AgentError::NotImplemented(format!("provider {name} not implemented"))
}
