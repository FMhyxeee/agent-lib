use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::{debug, info};

use crate::protocol::Op;
use crate::session::{Session, TurnContext};
use crate::tasks::CompactTask;

// 导入handler模块
use crate::tasks::handlers::{
    approval, interaction, mcp, session as session_handlers, skill, system,
};

/// Submission 结构
///
/// 表示一个提交到 submission_loop 的操作。
#[derive(Debug)]
pub struct Submission {
    pub id: String,
    pub op: Op,
}

impl Submission {
    /// 创建新的 Submission
    pub fn new(id: impl Into<String>, op: Op) -> Self {
        Self { id: id.into(), op }
    }
}

/// Codex 兼容的核心事件循环
///
/// 这是处理所有 Op 的统一入口点，管理任务的创建、执行和生命周期。
pub async fn submission_loop(sess: Arc<Session>, mut rx_sub: mpsc::Receiver<Submission>) {
    let mut previous_context: Option<Arc<TurnContext>> = None;

    info!("Starting submission loop");

    while let Some(sub) = rx_sub.recv().await {
        debug!(op = ?sub.op, "Processing submission");

        match sub.op {
            Op::Interrupt => {
                session_handlers::handle_interrupt(&sess).await;
            }

            Op::OverrideTurnContext {
                cwd,
                approval_policy,
                sandbox_policy,
                model,
                effort,
                summary,
                collaboration_mode,
            } => {
                previous_context = Some(
                    session_handlers::handle_override_turn_context(
                        &sess,
                        sub.id,
                        cwd,
                        approval_policy,
                        sandbox_policy,
                        model,
                        effort,
                        summary,
                        collaboration_mode,
                    )
                    .await,
                );
            }

            Op::UserTurn { .. } | Op::UserInputLegacy { .. } => {
                interaction::handle_user_input_or_turn(&sess, sub.id, sub.op, &mut previous_context).await;
            }

            Op::ExecApproval { id, decision } => {
                approval::handle_exec_approval(&sess, id, decision).await;
            }

            Op::PatchApproval { id, decision } => {
                approval::handle_patch_approval(&sess, id, decision).await;
            }

            Op::Compact => {
                if let Some(ctx) = &previous_context {
                    sess.spawn_task(Arc::clone(ctx), CompactTask).await;
                } else {
                    let ctx = sess.new_default_turn().await;
                    sess.spawn_task(ctx, CompactTask).await;
                }
            }

            Op::Shutdown => {
                info!("Shutdown requested, exiting submission loop");
                break;
            }

            Op::ListMcpTools => {
                mcp::handle_list_mcp_tools(&sess).await;
            }

            Op::ListMcpResources => {
                mcp::handle_list_mcp_resources(&sess).await;
            }

            Op::ReadMcpResource { uri } => {
                mcp::handle_read_mcp_resource(&sess, uri).await;
            }

            Op::ListMcpPrompts => {
                mcp::handle_list_mcp_prompts(&sess).await;
            }

            Op::GetMcpPrompt { name, arguments } => {
                mcp::handle_get_mcp_prompt(&sess, name, arguments).await;
            }

            Op::RefreshMcpServers { config } => {
                mcp::handle_refresh_mcp_servers(&sess, config).await;
            }

            Op::Undo => {
                session_handlers::handle_undo(&sess).await;
            }

            Op::ThreadRollback { num_turns } => {
                session_handlers::handle_thread_rollback(&sess, num_turns).await;
            }

            Op::AddToHistory { text } => {
                session_handlers::handle_add_to_history(&sess, text).await;
            }

            Op::RunUserShellCommand { command } => {
                interaction::handle_run_user_shell_command(&sess, command).await;
            }

            Op::RunSubAgent { mode, input } => {
                interaction::handle_run_sub_agent(&sess, mode, input).await;
            }

            Op::ApprovalResponse {
                request_id,
                approved,
            } => {
                approval::handle_approval_response(&sess, request_id, approved).await;
            }

            Op::Handoff {
                target_agent,
                context,
            } => {
                system::handle_handoff(&sess, target_agent, context).await;
            }

            Op::UserInputAnswer { id, response } => {
                interaction::handle_user_input_answer(&sess, id, response).await;
            }

            Op::Review { review_request } => {
                approval::handle_review(&sess, review_request).await;
            }

            Op::GetHistoryEntryRequest { offset, log_id } => {
                system::handle_get_history_entry_request(&sess, offset, log_id).await;
            }

            Op::ListSkills { cwds, force_reload } => {
                skill::handle_list_skills(&sess, cwds, force_reload).await;
            }
            Op::GetSkill { name } => {
                skill::handle_get_skill(&sess, name).await;
            }
            Op::ApplySkill { name } => {
                skill::handle_apply_skill(&sess, name).await;
            }
            Op::ReadSkillFile {
                skill_name,
                file_path,
            } => {
                skill::handle_read_skill_file(&sess, skill_name, file_path).await;
            }

            Op::ListCustomPrompts => {
                system::handle_list_custom_prompts(&sess).await;
            }

            Op::ListModels => {
                system::handle_list_models(&sess).await;
            }

            Op::StartTurn { prompt, .. } => {
                system::handle_start_turn(&sess, prompt).await;
            }

            Op::UserInput { content } => {
                interaction::handle_user_input(&sess, content).await;
            }

            _ => {
                debug!("Unhandled op: {:?}", sub.op);
            }
        }
    }

    info!("Submission loop exited");
}

// === Handler 函数 ===
// 所有handler函数已迁移到 handlers/ 模块中：
// - session.rs: 会话中断、上下文覆盖、撤销、回滚等
// - skill.rs: 技能列表、获取、应用、读取技能文件
// - approval.rs: 执行审批、补丁审批、批准响应、代码审查
// - interaction.rs: 用户输入、子代理、shell命令执行
// - system.rs: Agent移交、历史查询、自定义提示、模型列表
// - mcp.rs: MCP工具、资源、提示管理
