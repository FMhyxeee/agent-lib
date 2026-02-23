use agent_lib::tools::builtin::{ShellTool, GitSafeDirectoryManager};
use agent_lib::tools::{Tool, ToolContext};
use serde_json::json;

/// 演示如何使用 Git 权限自动修复功能
///
/// 这个示例展示了：
/// 1. 如何手动使用 GitSafeDirectoryManager
/// 2. 如何配置 ShellTool 自动修复 Git 权限问题
/// 3. 如何在 workspace 中自动发现 Git 仓库
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 初始化日志
    tracing_subscriber::fmt::init();

    println!("=== Git 权限自动修复演示 ===\n");

    // ===== 示例 1: 手动使用 GitSafeDirectoryManager =====
    println!("1. 手动使用 GitSafeDirectoryManager");
    println!("--------------------------------------");

    let mut git_manager = GitSafeDirectoryManager::new();

    // 添加已知的仓库路径
    git_manager
        .add_repository("D:/WorkSpace/ai-lab/Ai-helper/agent-lib")
        .add_repository("D:/WorkSpace/ai-lab/Ai-helper/ai-desktop-assistant");

    println!("已添加 {} 个仓库路径", git_manager.repositories().len());

    // 检查并修复所有仓库
    match git_manager.fix_all() {
        Ok(count) => {
            println!("成功修复了 {} 个仓库的权限问题", count);
        }
        Err(e) => {
            println!("修复失败: {}", e);
        }
    }

    // ===== 示例 2: 自动发现 workspace 中的仓库 =====
    println!("\n2. 自动发现 workspace 中的仓库");
    println!("--------------------------------------");

    let mut auto_git_manager = GitSafeDirectoryManager::new();

    // 从 workspace 根目录自动发现所有仓库
    match auto_git_manager.discover_from_workspace("D:/WorkSpace/ai-lab/Ai-helper") {
        Ok(_) => {
            println!("自动发现了 {} 个仓库:", auto_git_manager.repositories().len());
            for repo in auto_git_manager.repositories() {
                println!("  - {}", repo);
            }

            // 修复所有发现的仓库
            let _ = auto_git_manager.fix_all();
        }
        Err(e) => {
            println!("自动发现失败: {}", e);
        }
    }

    // ===== 示例 3: 使用 ShellTool（自动修复）=====
    println!("\n3. 使用 ShellTool（自动修复）");
    println!("--------------------------------------");

    // 创建 ShellTool 并配置 workspace 自动发现
    let shell_tool = ShellTool::new()
        .with_workspace_discovery("D:/WorkSpace/ai-lab/Ai-helper");

    // 执行 Git 命令（会自动检测并修复权限问题）
    let ctx = ToolContext {
        cwd: None,
        sandbox_root: None,
    };

    println!("执行 Git 命令（如果权限有问题会自动修复）...");

    match shell_tool
        .execute(
            json!({
                "command": "git -C D:/WorkSpace/ai-lab/Ai-helper/agent-lib status"
            }),
            &ctx,
        )
        .await
    {
        Ok(result) => {
            let output = result.output;
            if output.get("success").and_then(|v| v.as_bool()).unwrap_or(false) {
                println!("✓ Git 命令执行成功");
                if let Some(stdout) = output.get("stdout").and_then(|v| v.as_str()) {
                    if !stdout.is_empty() {
                        println!("输出: {}", stdout.lines().take(5).collect::<Vec<_>>().join("\n"));
                    }
                }
            } else {
                println!("✗ Git 命令执行失败");
                if let Some(stderr) = output.get("stderr").and_then(|v| v.as_str()) {
                    println!("错误: {}", stderr);
                }
            }
        }
        Err(e) => {
            println!("执行出错: {}", e);
        }
    }

    // ===== 示例 4: 禁用自动修复 =====
    println!("\n4. 禁用自动修复的配置");
    println!("--------------------------------------");

    use agent_lib::tools::builtin::ShellToolConfig;

    let config = ShellToolConfig {
        auto_fix_git_permissions: false, // 禁用自动修复
        ..Default::default()
    };

    let manual_shell = ShellTool::with_config(config);
    println!("已创建禁用自动修复的 ShellTool");
    println!("配置: auto_fix_git_permissions = {}", manual_shell.config().auto_fix_git_permissions);

    println!("\n=== 演示完成 ===");

    Ok(())
}
