use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use serde_json::json;
use std::process::Stdio;
use std::time::Duration;
use tokio::process::Command;
use tokio::time::timeout;

use crate::error::{AgentError, AgentResult};
use crate::tools::{Tool, ToolContext, ToolDef, ToolResult};
use super::git_utils::GitSafeDirectoryManager;

/// Shell 工具安全策略
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ShellSecurityPolicy {
    /// 宽松模式：仅阻止黑名单中的危险命令
    Permissive,
    /// 严格模式：只允许白名单中的安全命令
    Strict,
    /// 禁用模式：禁止所有命令执行
    Disabled,
}

impl Default for ShellSecurityPolicy {
    fn default() -> Self {
        Self::Strict // 默认使用严格模式
    }
}

/// Shell 工具配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShellToolConfig {
    /// 安全策略
    pub security_policy: ShellSecurityPolicy,
    /// 命令执行超时时间（秒）
    pub timeout_secs: u64,
    /// 自定义黑名单（追加到默认黑名单）
    pub custom_blocklist: Vec<String>,
    /// 自定义白名单（追加到默认白名单）
    pub custom_allowlist: Vec<String>,
    /// 自动修复 Git safe.directory 问题
    pub auto_fix_git_permissions: bool,
}

impl Default for ShellToolConfig {
    fn default() -> Self {
        Self {
            security_policy: ShellSecurityPolicy::default(),
            timeout_secs: 30,
            custom_blocklist: Vec::new(),
            custom_allowlist: Vec::new(),
            auto_fix_git_permissions: true, // 默认启用自动修复
        }
    }
}

/// 安全的 Shell 工具
#[derive(Debug)]
pub struct ShellTool {
    config: ShellToolConfig,
    git_manager: GitSafeDirectoryManager,
}

impl Default for ShellTool {
    fn default() -> Self {
        Self::new()
    }
}

impl ShellTool {
    /// 创建默认配置的 ShellTool
    pub fn new() -> Self {
        Self {
            config: ShellToolConfig::default(),
            git_manager: GitSafeDirectoryManager::new(),
        }
    }

    /// 使用自定义配置创建 ShellTool
    pub fn with_config(config: ShellToolConfig) -> Self {
        Self {
            config,
            git_manager: GitSafeDirectoryManager::new(),
        }
    }

    /// 设置 Git 仓库路径列表
    pub fn with_git_repositories(mut self, repo_paths: Vec<String>) -> Self {
        for path in repo_paths {
            self.git_manager.add_repository(&path);
        }
        self
    }

    /// 自动发现 workspace 中的 Git 仓库
    pub fn with_workspace_discovery(mut self, workspace_root: &str) -> Self {
        let _ = self.git_manager.discover_from_workspace(workspace_root);
        self
    }

    /// 获取当前配置
    pub fn config(&self) -> &ShellToolConfig {
        &self.config
    }

    /// 尝试在执行 Git 命令前自动修复权限问题
    async fn try_fix_git_permissions(&self, command: &str) -> Result<(), AgentError> {
        // 只在启用时处理
        if !self.config.auto_fix_git_permissions {
            return Ok(());
        }

        // 只处理 Git 命令
        if !command.trim().starts_with("git ") {
            return Ok(());
        }

        // 尝试从命令中提取 -C 参数指定的路径，或者使用 cwd
        let repo_path = self.extract_git_repo_path(command);

        if let Some(path) = repo_path {
            // 检查并修复这个特定仓库
            if let Ok(has_issue) = self.git_manager.check_repository(&path) {
                if has_issue {
                    tracing::info!(
                        "Detected Git permission issue for {}, attempting to fix...",
                        path
                    );
                    if let Err(e) = self.git_manager.add_to_safe_directory(&path) {
                        tracing::warn!("Failed to auto-fix Git permissions: {}", e);
                        // 不返回错误，让命令继续执行
                    } else {
                        tracing::info!("Successfully fixed Git permissions for {}", path);
                    }
                }
            }
        } else {
            // 如果无法确定具体仓库，尝试修复所有已知仓库
            let _ = self.git_manager.fix_all();
        }

        Ok(())
    }

    /// 从 Git 命令中提取仓库路径
    fn extract_git_repo_path(&self, command: &str) -> Option<String> {
        let parts: Vec<&str> = command.split_whitespace().collect();

        // 查找 -C 参数
        for (i, part) in parts.iter().enumerate() {
            if *part == "-C" && i + 1 < parts.len() {
                return Some(parts[i + 1].to_string());
            }
        }

        // 如果没有 -C 参数，返回 None（将使用当前工作目录）
        None
    }

    /// 检查命令是否允许执行
    fn is_command_allowed(&self, command: &str) -> Result<(), String> {
        match self.config.security_policy {
            ShellSecurityPolicy::Disabled => {
                return Err("Shell command execution is disabled by security policy".to_string());
            }
            ShellSecurityPolicy::Permissive => {
                // 宽松模式：检查黑名单
                if self.is_in_blocklist(command) {
                    return Err(format!(
                        "Command blocked by security policy (in blocklist): {}",
                        command
                    ));
                }
            }
            ShellSecurityPolicy::Strict => {
                // 严格模式：必须匹配白名单且不在黑名单中
                if self.is_in_blocklist(command) {
                    return Err(format!(
                        "Command blocked by security policy (in blocklist): {}",
                        command
                    ));
                }
                if !self.is_in_allowlist(command) {
                    return Err(format!(
                        "Command not allowed by security policy (not in allowlist): {}",
                        command
                    ));
                }
            }
        }
        Ok(())
    }

    /// 检查命令是否在黑名单中
    fn is_in_blocklist(&self, command: &str) -> bool {
        let command_lower = command.to_lowercase();

        // 默认黑名单：危险命令
        let default_blocklist = [
            // === 系统破坏 ===
            "rm -rf /",
            "rm -rf /*",
            "rm -rf ~",
            "rm -rf *",
            "format",
            "mkfs",
            "dd if=/dev/zero",
            "dd if=/dev/random",
            ":(){:|:&};:", // Fork bomb
            // === 系统控制 ===
            "shutdown",
            "reboot",
            "halt",
            "poweroff",
            "init 0",
            "init 6",
            // === Windows 危险命令 ===
            "format c:",
            "del /q",
            "rmdir /s /q c:\\",
            "shutdown /s",
            "shutdown /r",
            // === 密码/密钥相关 ===
            "passwd",
            "chpasswd",
            // === 数据库危险操作 ===
            "drop database",
            "truncate table",
            // === 网络危险操作 ===
            "iptables -f",
            "iptables --flush",
            // === 权限提升 ===
            "chmod 777",
            "chmod -r 777",
        ];

        // 检查默认黑名单
        for blocked in default_blocklist {
            if command_lower.contains(blocked) {
                return true;
            }
        }

        // 检查自定义黑名单
        for blocked in &self.config.custom_blocklist {
            if command_lower.contains(&blocked.to_lowercase()) {
                return true;
            }
        }

        false
    }

    /// 检查命令是否在白名单中
    fn is_in_allowlist(&self, command: &str) -> bool {
        let command_lower = command.to_lowercase();

        // 默认白名单：安全的命令模式
        let default_allowlist = [
            // === 信息查看 ===
            "ls",
            "dir",
            "cat ",
            "type ",
            "head",
            "tail",
            "grep",
            "findstr",
            "echo",
            "pwd",
            "cd ",
            "which",
            "where",
            "whoami",
            "hostname",
            "date",
            "time",
            // === Git 操作 ===
            "git status",
            "git log",
            "git diff",
            "git show",
            "git branch",
            "git remote",
            "git tag",
            // === 构建工具===
            "cargo build",
            "cargo test",
            "cargo check",
            "cargo fmt",
            "cargo clippy",
            "cargo doc",
            "cargo clean",
            "npm run",
            "npm test",
            "npm build",
            "npm lint",
            "make",
            "cmake",
            // === 文件操作（安全范围）===
            "mkdir",
            "touch",
            "cp ",
            "mv ",
            "copy ",
            "move ",
            // === 开发工具 ===
            "rustc",
            "python",
            "node",
            "cargo",
            "rustfmt",
            // === 包管理器（只读操作）===
            "cargo search",
            "npm search",
            "pip list",
            "pip show",
        ];

        // 检查默认白名单
        for pattern in default_allowlist {
            if command_lower.starts_with(pattern) {
                return true;
            }
        }

        // 检查自定义白名单
        for pattern in &self.config.custom_allowlist {
            if command_lower.starts_with(&pattern.to_lowercase()) {
                return true;
            }
        }

        false
    }
}

#[async_trait]
impl Tool for ShellTool {
    fn definition(&self) -> ToolDef {
        ToolDef {
            name: "shell".to_string(),
            description: concat!(
                "Execute a shell command with security restrictions. ",
                "Commands are validated against security policy (blocklist/allowlist). ",
                "Execution has a configurable timeout (default 30s)."
            )
            .to_string(),
            schema: json!({
                "type": "object",
                "properties": {
                    "command": {
                        "type": "string",
                        "description": "The shell command to execute"
                    }
                },
                "required": ["command"]
            }),
        }
    }

    async fn execute(&self, args: Value, _ctx: &ToolContext) -> AgentResult<ToolResult> {
        let command = args
            .get("command")
            .and_then(|value| value.as_str())
            .ok_or_else(|| AgentError::Tool("missing command parameter".to_string()))?;

        // 安全检查
        if let Err(reason) = self.is_command_allowed(command) {
            tracing::warn!("Shell command blocked: {}", reason);
            return Err(AgentError::Tool(format!("Command not allowed: {}", reason)));
        }

        // 尝试自动修复 Git 权限问题
        self.try_fix_git_permissions(command).await?;

        tracing::info!("Executing shell command: {}", command);

        // 带超时执行命令
        let exec_timeout = Duration::from_secs(self.config.timeout_secs);
        let result = timeout(exec_timeout, async {
            if cfg!(windows) {
                Command::new("cmd")
                    .args(["/C", command])
                    .stdout(Stdio::piped())
                    .stderr(Stdio::piped())
                    .output()
                    .await
            } else {
                Command::new("sh")
                    .arg("-c")
                    .arg(command)
                    .stdout(Stdio::piped())
                    .stderr(Stdio::piped())
                    .output()
                    .await
            }
        })
        .await;

        match result {
            Ok(Ok(output)) => {
                let stdout = String::from_utf8_lossy(&output.stdout).to_string();
                let stderr = String::from_utf8_lossy(&output.stderr).to_string();
                let exit_code = output.status.code();

                tracing::debug!(
                    "Shell command completed: exit_code={:?}, stdout_len={}, stderr_len={}",
                    exit_code,
                    stdout.len(),
                    stderr.len()
                );

                Ok(ToolResult {
                    output: json!({
                        "stdout": stdout,
                        "stderr": stderr,
                        "exit_code": exit_code,
                        "success": output.status.success(),
                    }),
                })
            }
            Ok(Err(e)) => {
                tracing::error!("Shell command execution failed: {}", e);
                Err(AgentError::Tool(format!("Shell execution failed: {}", e)))
            }
            Err(_) => {
                tracing::warn!(
                    "Shell command timed out after {}s: {}",
                    self.config.timeout_secs,
                    command
                );
                Err(AgentError::Tool(format!(
                    "Command timed out after {} seconds",
                    self.config.timeout_secs
                )))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config_is_strict() {
        let tool = ShellTool::new();
        assert!(matches!(
            tool.config.security_policy,
            ShellSecurityPolicy::Strict
        ));
    }

    #[test]
    fn test_blocklist_detection() {
        let tool = ShellTool::new();

        // 应该被阻止的危险命令
        assert!(tool.is_in_blocklist("rm -rf /"));
        assert!(tool.is_in_blocklist("rm -rf /home"));
        assert!(tool.is_in_blocklist("shutdown"));
        assert!(tool.is_in_blocklist("format c:"));
        assert!(tool.is_in_blocklist("DROP DATABASE users"));
        assert!(tool.is_in_blocklist("chmod 777 /etc/passwd"));

        // 安全命令不应被阻止
        assert!(!tool.is_in_blocklist("ls -la"));
        assert!(!tool.is_in_blocklist("git status"));
        assert!(!tool.is_in_blocklist("cargo build"));
    }

    #[test]
    fn test_allowlist_detection() {
        let tool = ShellTool::new();

        // 应该被允许的安全命令
        assert!(tool.is_in_allowlist("ls -la"));
        assert!(tool.is_in_allowlist("git status"));
        assert!(tool.is_in_allowlist("cargo build"));
        assert!(tool.is_in_allowlist("npm test"));
        assert!(tool.is_in_allowlist("cat file.txt"));
        assert!(tool.is_in_allowlist("mkdir new_dir"));

        // 未在白名单中的命令
        assert!(!tool.is_in_allowlist("some-random-command"));
        assert!(!tool.is_in_allowlist("curl http://example.com"));
    }

    #[test]
    fn test_strict_mode_allows_safe_commands() {
        let tool = ShellTool::new();

        // 严格模式下，白名单命令应该被允许
        assert!(tool.is_command_allowed("ls -la").is_ok());
        assert!(tool.is_command_allowed("git status").is_ok());
        assert!(tool.is_command_allowed("cargo build").is_ok());

        // 严格模式下，非白名单命令应该被拒绝
        assert!(tool.is_command_allowed("curl http://example.com").is_err());
        assert!(tool.is_command_allowed("wget file").is_err());
    }

    #[test]
    fn test_strict_mode_blocks_dangerous_commands() {
        let tool = ShellTool::new();

        // 即使在白名单中，危险命令也应该被阻止
        assert!(tool.is_command_allowed("rm -rf /").is_err());
        assert!(tool.is_command_allowed("shutdown").is_err());
    }

    #[test]
    fn test_permissive_mode() {
        let config = ShellToolConfig {
            security_policy: ShellSecurityPolicy::Permissive,
            ..Default::default()
        };
        let tool = ShellTool::with_config(config);

        // 宽松模式下，非黑名单命令应该被允许
        assert!(tool.is_command_allowed("curl http://example.com").is_ok());
        assert!(tool.is_command_allowed("wget file").is_ok());

        // 黑名单命令仍然被阻止
        assert!(tool.is_command_allowed("rm -rf /").is_err());
        assert!(tool.is_command_allowed("shutdown").is_err());
    }

    #[test]
    fn test_disabled_mode() {
        let config = ShellToolConfig {
            security_policy: ShellSecurityPolicy::Disabled,
            ..Default::default()
        };
        let tool = ShellTool::with_config(config);

        // 禁用模式下，所有命令都被阻止
        assert!(tool.is_command_allowed("ls").is_err());
        assert!(tool.is_command_allowed("git status").is_err());
    }

    #[test]
    fn test_custom_blocklist() {
        let config = ShellToolConfig {
            custom_blocklist: vec!["my-dangerous-tool".to_string()],
            ..Default::default()
        };
        let tool = ShellTool::with_config(config);

        assert!(tool.is_in_blocklist("my-dangerous-tool --option"));
    }

    #[test]
    fn test_custom_allowlist() {
        let config = ShellToolConfig {
            custom_allowlist: vec!["my-safe-tool".to_string()],
            ..Default::default()
        };
        let tool = ShellTool::with_config(config);

        assert!(tool.is_in_allowlist("my-safe-tool --option"));
    }

    #[tokio::test]
    async fn test_execute_missing_command() {
        let tool = ShellTool::new();
        let ctx = ToolContext {
            cwd: None,
            sandbox_root: None,
        };

        let result = tool.execute(json!({}), &ctx).await;

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("missing command"));
    }

    #[tokio::test]
    async fn test_execute_blocked_command() {
        let tool = ShellTool::new();
        let ctx = ToolContext {
            cwd: None,
            sandbox_root: None,
        };

        let result = tool.execute(json!({"command": "rm -rf /"}), &ctx).await;

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not allowed"));
    }
}
