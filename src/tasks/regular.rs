use async_trait::async_trait;
use serde_json::Value;
use std::sync::Arc;
use tracing::{debug, info};

use crate::model::{Message, ToolCallMessage};
use crate::protocol::{Event, TurnAbortReason};
use crate::session::{TaskSession, TurnContext};
use crate::tasks::{SessionTask, TaskKind};
use crate::tools::{needs_truncation, truncate_output};
use tokio_util::sync::CancellationToken;

/// 常规 Turn 任务
///
/// 处理标准的用户输入和模型响应循环，支持工具调用。
#[derive(Clone, Copy, Default)]
pub struct RegularTask;

/// 最大工具调用循环次数，防止无限循环
const MAX_TOOL_CALL_LOOPS: usize = 10;

#[async_trait]
impl SessionTask for RegularTask {
    fn kind(&self) -> TaskKind {
        TaskKind::Regular
    }

    async fn run(
        self: Arc<Self>,
        session: Arc<dyn TaskSession>,
        ctx: Arc<TurnContext>,
        cancellation_token: CancellationToken,
    ) -> Option<String> {
        let turn_id = ctx.sub_id.clone();
        info!("[{}] Starting RegularTask", turn_id);

        // 发送 TurnStarted 事件
        session
            .emit_event(Event::TurnStarted {
                turn_id: turn_id.clone(),
            })
            .await;

        // 检查是否被取消
        if cancellation_token.is_cancelled() {
            debug!("[{}] Task cancelled before start", turn_id);
            session
                .emit_event(Event::TurnAborted {
                    reason: TurnAbortReason::Error("cancelled before start".to_string()),
                })
                .await;
            return None;
        }

        // 1. 获取对话历史
        let mut history = session.history().await;
        let total_tokens = history.total_tokens();
        debug!("[{}] Current history: {} tokens", turn_id, total_tokens);

        // 2. 检查是否需要压缩
        let context_window = ctx.context_window;
        if total_tokens > context_window {
            let keep_recent = ((context_window as f32) * 0.7) as usize;
            let summary = format!(
                "[Compacted {} tokens of conversation history before turn {}]",
                total_tokens.saturating_sub(keep_recent),
                turn_id
            );
            debug!(
                "[{}] Compacting history ({} > {} tokens), keeping {} recent messages",
                turn_id, total_tokens, context_window, keep_recent
            );
            session.compact_history(keep_recent, summary).await;
            history = session.history().await;
        }

        // 3. 准备模型调用
        let messages = history.for_prompt();

        if messages.is_empty() {
            debug!("[{}] No messages to send to model", turn_id);
            session
                .emit_event(Event::Warning {
                    message: "No messages in history to process".to_string(),
                })
                .await;
            return None;
        }

        // 4. 获取可用工具
        let tools = session.list_tools().await;
        debug!("[{}] Available tools: {}", turn_id, tools.len());

        // 5. 发送 ModelStreaming 事件（开始）
        session
            .emit_event(Event::ModelStreaming {
                chunk: format!("[{}] Thinking...\n", turn_id),
            })
            .await;

        // 6. 工具调用循环
        let mut loop_count = 0;
        let mut current_messages = messages;
        let mut final_content = String::new();
        let mut final_usage = None;

        loop {
            if cancellation_token.is_cancelled() {
                debug!("[{}] Task cancelled during tool loop", turn_id);
                session
                    .emit_event(Event::TurnAborted {
                        reason: TurnAbortReason::Error("cancelled during tool loop".to_string()),
                    })
                    .await;
                return None;
            }

            // 检查循环次数
            if loop_count >= MAX_TOOL_CALL_LOOPS {
                debug!("[{}] Max tool call loops reached", turn_id);
                session
                    .emit_event(Event::Warning {
                        message: format!("Max tool call loops ({}) reached", MAX_TOOL_CALL_LOOPS),
                    })
                    .await;
                break;
            }

            // 调用模型
            let response = match session
                .chat_model(current_messages.clone(), tools.clone())
                .await
            {
                Ok(resp) => resp,
                Err(e) => {
                    let error_msg = format!("{:?}", e);
                    debug!("[{}] Model call failed: {}", turn_id, error_msg);
                    session.emit_event(Event::Error { error: e.clone() }).await;
                    session
                        .emit_event(Event::ModelStreaming {
                            chunk: format!("[ERROR: {}]\n", error_msg),
                        })
                        .await;
                    return None;
                }
            };

            // 保存最终响应信息
            final_content = response.content.clone();
            final_usage = Some(response.usage.clone());

            // 检查是否有工具调用
            if response.tool_calls.is_empty() {
                // 修复 P0-1: 将助手响应添加到会话历史
                session
                    .push_message(Message::assistant(response.content.clone()))
                    .await;

                // 没有工具调用，发送响应内容
                let chunk_size = 20;
                let mut current_chunk = String::new();
                for ch in response.content.chars() {
                    current_chunk.push(ch);
                    if current_chunk.chars().count() >= chunk_size {
                        if cancellation_token.is_cancelled() {
                            session
                                .emit_event(Event::TurnAborted {
                                    reason: TurnAbortReason::Error(
                                        "cancelled during response".to_string(),
                                    ),
                                })
                                .await;
                            return None;
                        }
                        session
                            .emit_event(Event::ModelStreaming {
                                chunk: current_chunk.clone(),
                            })
                            .await;
                        current_chunk.clear();
                    }
                }
                if !current_chunk.is_empty() {
                    session
                        .emit_event(Event::ModelStreaming {
                            chunk: current_chunk,
                        })
                        .await;
                }
                break;
            }

            // 有工具调用 - 构建助手消息（包含工具调用）
            let tool_calls: Vec<ToolCallMessage> = response
                .tool_calls
                .iter()
                .map(|tc| ToolCallMessage {
                    id: tc.id.clone(),
                    name: tc.name.clone(),
                    arguments: tc.arguments.clone(),
                })
                .collect();

            let assistant_msg =
                Message::assistant_with_calls(response.content.clone(), tool_calls.clone());

            // 修复 P0-1: 将助手消息添加到会话历史
            session.push_message(assistant_msg.clone()).await;

            debug!(
                "[{}] Model requested {} tool calls",
                turn_id,
                response.tool_calls.len()
            );

            // 发送工具调用事件
            for tc in &response.tool_calls {
                session
                    .emit_event(Event::ToolCallRequested {
                        tool: tc.name.clone(),
                        args: tc.arguments.clone(),
                    })
                    .await;
            }

            // 执行所有工具调用
            let mut tool_messages = Vec::new();
            for tc in &response.tool_calls {
                debug!(
                    "[{}] Executing tool: {} with args: {}",
                    turn_id, tc.name, tc.arguments
                );

                match session.execute_tool(&tc.name, tc.arguments.clone()).await {
                    Ok(result) => {
                        // 将结果转换为字符串
                        let mut result_str = match &result.output {
                            Value::String(s) => s.clone(),
                            v => serde_json::to_string_pretty(v)
                                .unwrap_or_else(|_| format!("{:?}", v)),
                        };

                        // 应用工具输出截断，防止过大输出击穿 context
                        let max_size = ctx.tool_output_max_size;
                        if needs_truncation(&result_str, max_size) {
                            let original_len = result_str.chars().count();
                            result_str = truncate_output(&result_str, max_size);
                            debug!(
                                "[{}] Tool {} output truncated: {} -> {} chars",
                                turn_id,
                                tc.name,
                                original_len,
                                result_str.chars().count()
                            );
                            session
                                .emit_event(Event::Warning {
                                    message: format!(
                                        "Tool '{}' output truncated from {} to {} characters",
                                        tc.name,
                                        original_len,
                                        result_str.chars().count()
                                    ),
                                })
                                .await;
                        }

                        debug!("[{}] Tool {} result: {}", turn_id, tc.name, result_str);

                        // 发送工具结果事件
                        session
                            .emit_event(Event::ToolCallResult {
                                tool: tc.name.clone(),
                                result: result.clone(),
                            })
                            .await;

                        // 创建工具结果消息
                        tool_messages.push(Message::tool_result(&tc.id, result_str));
                    }
                    Err(e) => {
                        let error_str = format!("Error: {:?}", e);
                        debug!("[{}] Tool {} failed: {}", turn_id, tc.name, error_str);

                        // 发送工具错误事件
                        session.emit_event(Event::Error { error: e.clone() }).await;

                        // 创建错误结果消息
                        tool_messages.push(Message::tool_result(&tc.id, error_str));
                    }
                }
            }

            // 修复 P0-1: 将工具结果消息添加到会话历史
            for tool_msg in &tool_messages {
                session.push_message(tool_msg.clone()).await;
            }

            // 构建新的消息列表（用于下一轮模型调用）
            current_messages.push(assistant_msg);
            current_messages.extend(tool_messages);

            loop_count += 1;
            debug!("[{}] Tool loop iteration {}", turn_id, loop_count);
        }

        // 7. 发送完成事件
        if let Some(usage) = final_usage {
            session
                .emit_event(Event::ModelComplete {
                    content: final_content.clone(),
                    usage,
                })
                .await;
        }

        info!(
            "[{}] RegularTask completed ({} tool loops)",
            turn_id, loop_count
        );
        Some(format!(
            "Turn {}: {} tokens processed, response length: {}, tool loops: {}",
            turn_id,
            total_tokens,
            final_content.len(),
            loop_count
        ))
    }
}
