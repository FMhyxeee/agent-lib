use std::path::Path;
use std::process::Command;
use std::io;

/// Git 安全目录管理器
///
/// 用于检测和修复 Git 仓库的权限问题（dubious ownership）
#[derive(Debug)]
pub struct GitSafeDirectoryManager {
    /// 已知的仓库路径
    repository_paths: Vec<String>,
}

impl GitSafeDirectoryManager {
    /// 创建新的管理器
    pub fn new() -> Self {
        Self {
            repository_paths: Vec::new(),
        }
    }

    /// 添加一个仓库路径
    pub fn add_repository(&mut self, path: &str) -> &mut Self {
        self.repository_paths.push(path.to_string());
        self
    }

    /// 自动检测并添加 workspace 中的所有仓库
    ///
    /// 从 workspace 根目录查找所有 .git 目录来识别仓库
    pub fn discover_from_workspace(&mut self, workspace_root: &str) -> io::Result<&mut Self> {
        let workspace_path = Path::new(workspace_root);

        // 检查 workspace 根目录本身是否是仓库
        if workspace_path.join(".git").exists() {
            self.add_repository(workspace_root);
        }

        // 检查常见的子目录
        let entries = std::fs::read_dir(workspace_path)?;
        for entry in entries {
            let entry = entry?;
            let path = entry.path();

            // 只检查子目录
            if path.is_dir() {
                let git_dir = path.join(".git");

                // 检查是否是 Git 仓库（.git 目录存在或者是 git submodule/file）
                if git_dir.exists() {
                    if let Some(path_str) = path.to_str() {
                        self.add_repository(path_str);
                    }
                }
            }
        }

        Ok(self)
    }

    /// 检查特定 Git 仓库是否存在权限问题
    ///
    /// 通过运行 `git status` 来测试，如果返回 dubious ownership 错误则有问题
    pub fn check_repository(&self, repo_path: &str) -> io::Result<bool> {
        let output = Command::new("git")
            .args(["-C", repo_path, "status"])
            .output();

        match output {
            Ok(output) => {
                // 检查 stderr 是否包含 dubious ownership 错误
                let stderr = String::from_utf8_lossy(&output.stderr);
                Ok(stderr.contains("dubious ownership") || stderr.contains("unsafe repository"))
            }
            Err(_) => {
                // git 命令失败，可能不是 git 仓库或其他问题
                Ok(false)
            }
        }
    }

    /// 将仓库路径添加到 Git 的 safe.directory 配置
    ///
    /// 使用 --global 范围，这样会应用到当前用户的所有 git 操作
    pub fn add_to_safe_directory(&self, repo_path: &str) -> io::Result<()> {
        // 标准化路径（转换为绝对路径，统一分隔符）
        let normalized_path = Path::new(repo_path).canonicalize()?;

        let path_str = normalized_path
            .to_str()
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "Path contains invalid UTF-8 characters"))?;

        // 检查是否已经在 safe.directory 中
        let is_already_safe = self.is_in_safe_directory(path_str)?;

        if is_already_safe {
            tracing::debug!("Repository already in safe.directory: {}", path_str);
            return Ok(());
        }

        // 添加到 safe.directory
        let output = Command::new("git")
            .args(["config", "--global", "--add", "safe.directory", path_str])
            .output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(io::Error::new(
                io::ErrorKind::Other,
                format!("Failed to add {} to safe.directory: {}", path_str, stderr),
            ));
        }

        tracing::info!("Added repository to safe.directory: {}", path_str);
        Ok(())
    }

    /// 检查路径是否已在 safe.directory 配置中
    fn is_in_safe_directory(&self, path: &str) -> io::Result<bool> {
        let output = Command::new("git")
            .args(["config", "--global", "--get-all", "safe.directory"])
            .output()?;

        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            // 标准化路径进行比较（处理路径分隔符差异）
            let normalized_path = path.replace('\\', "/");
            let existing: Vec<String> = stdout
                .lines()
                .map(|p| p.replace('\\', "/"))
                .collect();

            Ok(existing.iter().any(|p| {
                p.trim() == normalized_path
                // 也支持路径不带尾部斜杠的比较
                || p.trim_end_matches('/') == normalized_path.trim_end_matches('/')
            }))
        } else {
            // 配置不存在或为空
            Ok(false)
        }
    }

    /// 修复所有已知仓库的权限问题
    ///
    /// 返回成功修复的仓库数量
    pub fn fix_all(&self) -> io::Result<usize> {
        let mut fixed_count = 0;

        for repo_path in &self.repository_paths {
            // 检查是否有问题
            if self.check_repository(repo_path)? {
                // 尝试修复
                match self.add_to_safe_directory(repo_path) {
                    Ok(_) => {
                        fixed_count += 1;
                        tracing::info!("Fixed git permissions for: {}", repo_path);
                    }
                    Err(e) => {
                        tracing::warn!(
                            "Failed to fix git permissions for {}: {}",
                            repo_path,
                            e
                        );
                    }
                }
            } else {
                tracing::debug!("No permission issues for: {}", repo_path);
            }
        }

        Ok(fixed_count)
    }

    /// 获取所有已知的仓库路径
    pub fn repositories(&self) -> &[String] {
        &self.repository_paths
    }
}

impl Default for GitSafeDirectoryManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_manager() {
        let manager = GitSafeDirectoryManager::new();
        assert_eq!(manager.repositories().len(), 0);
    }

    #[test]
    fn test_add_repository() {
        let mut manager = GitSafeDirectoryManager::new();
        manager.add_repository("/test/repo");
        assert_eq!(manager.repositories().len(), 1);
        assert_eq!(manager.repositories()[0], "/test/repo");
    }
}
