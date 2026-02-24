use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum MessageRole {
    System,
    User,
    Assistant,
    Tool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub role: MessageRole,
    pub content: String,
    /// 工具调用 ID (仅 Tool 角色使用)
    pub tool_call_id: Option<String>,
    /// 工具调用列表 (仅 Assistant 角色使用)
    pub tool_calls: Option<Vec<ToolCallMessage>>,
    /// 推理内容 (GLM 思考模式)
    ///
    /// 用于保存模型在响应过程中的思考内容,支持保留式思考(Preserved Thinking)。
    /// 在多轮对话中,需要将之前的 reasoning_content 返回给模型以保持推理连贯性。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning_content: Option<String>,
}

/// 消息中的工具调用
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallMessage {
    pub id: String,
    pub name: String,
    pub arguments: Value,
}

impl Message {
    pub fn system(content: impl Into<String>) -> Self {
        Self {
            role: MessageRole::System,
            content: content.into(),
            tool_call_id: None,
            tool_calls: None,
            reasoning_content: None,
        }
    }

    pub fn user(content: impl Into<String>) -> Self {
        Self {
            role: MessageRole::User,
            content: content.into(),
            tool_call_id: None,
            tool_calls: None,
            reasoning_content: None,
        }
    }

    pub fn assistant(content: impl Into<String>) -> Self {
        Self {
            role: MessageRole::Assistant,
            content: content.into(),
            tool_call_id: None,
            tool_calls: None,
            reasoning_content: None,
        }
    }

    pub fn assistant_with_calls(
        content: impl Into<String>,
        tool_calls: Vec<ToolCallMessage>,
    ) -> Self {
        Self {
            role: MessageRole::Assistant,
            content: content.into(),
            tool_call_id: None,
            tool_calls: Some(tool_calls),
            reasoning_content: None,
        }
    }

    /// 创建带有推理内容的助手消息
    pub fn assistant_with_reasoning(
        content: impl Into<String>,
        reasoning_content: impl Into<String>,
    ) -> Self {
        Self {
            role: MessageRole::Assistant,
            content: content.into(),
            tool_call_id: None,
            tool_calls: None,
            reasoning_content: Some(reasoning_content.into()),
        }
    }

    /// 创建带有推理内容和工具调用的助手消息
    pub fn assistant_with_calls_and_reasoning(
        content: impl Into<String>,
        tool_calls: Vec<ToolCallMessage>,
        reasoning_content: impl Into<String>,
    ) -> Self {
        Self {
            role: MessageRole::Assistant,
            content: content.into(),
            tool_call_id: None,
            tool_calls: Some(tool_calls),
            reasoning_content: Some(reasoning_content.into()),
        }
    }

    /// 创建工具结果消息
    pub fn tool_result(tool_call_id: impl Into<String>, result: impl Into<String>) -> Self {
        Self {
            role: MessageRole::Tool,
            content: result.into(),
            tool_call_id: Some(tool_call_id.into()),
            tool_calls: None,
            reasoning_content: None,
        }
    }

    /// 检查是否包含工具调用
    pub fn has_tool_calls(&self) -> bool {
        self.tool_calls
            .as_ref()
            .is_some_and(|calls| !calls.is_empty())
    }
}
