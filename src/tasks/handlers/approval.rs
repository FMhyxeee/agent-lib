//! 审批和审查相关handlers
//!
//! 处理工具执行审批、补丁审批、批准响应和代码审查等操作。

use tracing::debug;

use crate::protocol::{Event, ReviewDecision, ReviewRequest};
use crate::session::Session;

/// 处理执行审批
pub async fn handle_exec_approval(sess: &Session, id: String, decision: ReviewDecision) {
    debug!(id, ?decision, "Handling exec approval");

    match decision {
        ReviewDecision::Approve => {
            // 批准执行，返回成功结果
            sess.emit_event(Event::ToolCallResult {
                tool: id.clone(),
                result: crate::tools::ToolResult::text("Exec approved".to_string()),
            })
            .await;
        }
        ReviewDecision::Deny => {
            // 拒绝执行，返回错误
            sess.emit_event(Event::Error {
                error: crate::error::AgentError::Tool(format!("Exec denied: {}", id)),
            })
            .await;
        }
        ReviewDecision::ApproveWithEdits { edits } => {
            // 批准但带编辑，返回编辑后的结果
            sess.emit_event(Event::ToolCallResult {
                tool: id,
                result: crate::tools::ToolResult::text(format!(
                    "Exec approved with edits: {}",
                    edits
                )),
            })
            .await;
        }
    }
}

/// 处理补丁审批
pub async fn handle_patch_approval(sess: &Session, id: String, decision: ReviewDecision) {
    debug!(id, ?decision, "Handling patch approval");

    match decision {
        ReviewDecision::Approve => {
            // 批准补丁，返回成功结果
            sess.emit_event(Event::ToolCallResult {
                tool: format!("patch:{}", id.clone()),
                result: crate::tools::ToolResult::text("Patch approved".to_string()),
            })
            .await;
        }
        ReviewDecision::Deny => {
            // 拒绝补丁，返回错误
            sess.emit_event(Event::Error {
                error: crate::error::AgentError::Tool(format!("Patch denied: {}", id)),
            })
            .await;
        }
        ReviewDecision::ApproveWithEdits { edits } => {
            // 批准但带编辑，返回编辑后的结果
            sess.emit_event(Event::ToolCallResult {
                tool: format!("patch:{}", id),
                result: crate::tools::ToolResult::text(format!(
                    "Patch approved with edits: {}",
                    edits
                )),
            })
            .await;
        }
    }
}

/// 处理批准响应
pub async fn handle_approval_response(sess: &Session, request_id: String, approved: bool) {
    debug!(
        request_id = %request_id,
        approved = approved,
        "Handling approval response"
    );

    if approved {
        sess.emit_event(Event::ToolCallResult {
            tool: request_id.clone(),
            result: crate::tools::ToolResult::text(format!("Request {} approved", request_id)),
        })
        .await;
    } else {
        sess.emit_event(Event::Error {
            error: crate::error::AgentError::Tool(format!(
                "Request {} denied by user",
                request_id
            )),
        })
        .await;
    }
}

/// 处理代码审查请求
pub async fn handle_review(sess: &Session, review_request: ReviewRequest) {
    debug!(
        content_len = review_request.content.len(),
        "Handling review request"
    );

    // 发送审查开始事件
    sess.emit_event(Event::Warning {
        message: format!(
            "Code review started: {} chars",
            review_request.content.len()
        ),
    })
    .await;

    // 执行代码审查
    let review_result =
        perform_code_review(&review_request.content, review_request.context.as_deref());

    // 发送审查结果
    sess.emit_event(Event::ToolCallResult {
        tool: "review".to_string(),
        result: crate::tools::ToolResult::text(review_result),
    })
    .await;
}

/// 执行代码审查
fn perform_code_review(content: &str, context: Option<&str>) -> String {
    let mut issues = Vec::new();
    let mut suggestions = Vec::new();

    // 1. 检查常见问题
    if content.contains("TODO") || content.contains("FIXME") {
        issues.push("Found TODO/FIXME comments that need attention".to_string());
    }

    if content.contains("unwrap()") && !content.contains("unwrap_or") {
        issues.push(
            "Found unwrap() calls that may panic - consider using unwrap_or() or ? operator"
                .to_string(),
        );
    }

    if content.contains(".expect(") {
        issues.push("Found .expect() calls that may panic in production".to_string());
    }

    if content.contains("println!") {
        suggestions
            .push("Found println! macros - consider using a proper logging library".to_string());
    }

    // 2. 检查代码长度
    let line_count = content.lines().count();
    if line_count > 100 {
        suggestions.push(format!(
            "Function is {} lines long - consider breaking it into smaller functions",
            line_count
        ));
    }

    // 3. 检查文档注释
    if !content.contains("///") && !content.contains("/**") {
        suggestions.push("Consider adding documentation comments".to_string());
    }

    // 4. 检查错误处理
    if content.contains("fn ") && !content.contains("Result") && !content.contains("Option") {
        suggestions.push("Consider returning Result for error handling".to_string());
    }

    // 构建审查报告
    let mut report = "## Code Review Report\n\n".to_string();
    report.push_str(&format!("**Content Length:** {} chars\n", content.len()));
    report.push_str(&format!("**Lines:** {}\n\n", line_count));

    if let Some(ctx) = context {
        report.push_str(&format!("**Context:** {}\n\n", ctx));
    }

    if !issues.is_empty() {
        report.push_str("### Issues Found\n\n");
        for (i, issue) in issues.iter().enumerate() {
            report.push_str(&format!("{}. {}\n", i + 1, issue));
        }
        report.push('\n');
    }

    if !suggestions.is_empty() {
        report.push_str("### Suggestions\n\n");
        for (i, suggestion) in suggestions.iter().enumerate() {
            report.push_str(&format!("{}. {}\n", i + 1, suggestion));
        }
    }

    if issues.is_empty() && suggestions.is_empty() {
        report.push_str("### ✅ No issues found!\n\nThe code looks good.");
    }

    report
}
