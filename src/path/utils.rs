//! 路径处理工具函数
//!
//! 提供统一的路径标准化和解析功能。

use std::path::{Component, Path, PathBuf};
use crate::error::AgentResult;

/// 标准化路径,移除.和..组件
///
/// 将路径中的.和..组件解析，返回规范化的绝对路径形式。
///
/// # Examples
///
/// ```
/// # use agent_lib::path::utils::normalize_path;
/// let path = std::path::Path::new("/a/b/../c");
/// assert_eq!(normalize_path(path), std::path::PathBuf::from("/a/c"));
/// ```
pub fn normalize_path(path: &Path) -> PathBuf {
    let mut result = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Prefix(prefix) => result.push(prefix.as_os_str()),
            Component::RootDir => result.push(component.as_os_str()),
            Component::CurDir => {}
            Component::ParentDir => {
                result.pop();
            }
            Component::Normal(part) => result.push(part),
        }
    }
    result
}

/// 解析并标准化路径,支持相对路径转绝对路径
///
/// 将相对路径基于cwd转换为绝对路径，并进行沙盒检查。
///
/// # 参数
///
/// * `cwd` - 当前工作目录
/// * `sandbox_root` - 沙盒根目录(用于安全检查)
/// * `path` - 要解析的路径
///
/// # 返回
///
/// 返回解析后的PathBuf，如果路径逃逸沙盒则返回错误
///
/// # Examples
///
/// ```
/// # use agent_lib::path::utils::resolve_path;
/// let result = resolve_path(Some("/workspace"), Some("/workspace"), "subdir/file.txt");
/// assert!(result.is_ok());
///
/// let result = resolve_path(Some("/workspace"), Some("/workspace"), "../etc");
/// assert!(result.is_err()); // 尝试逃逸沙盒
/// ```
pub fn resolve_path(
    cwd: Option<&str>,
    sandbox_root: Option<&str>,
    path: &str,
) -> AgentResult<PathBuf> {
    let base = cwd.map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."));
    let target = Path::new(path);
    let resolved = if target.is_absolute() {
        target.to_path_buf()
    } else {
        base.join(target)
    };

    if let Some(root) = sandbox_root {
        let root_path = normalize_path(&PathBuf::from(root));
        if resolved
            .components()
            .any(|c| matches!(c, Component::ParentDir))
        {
            return Err(crate::error::AgentError::Tool(
                "parent dir segments not allowed in sandbox".to_string(),
            ));
        }
        let resolved_norm = normalize_path(&resolved);
        if !resolved_norm.starts_with(&root_path) {
            return Err(crate::error::AgentError::Tool(
                "path escapes sandbox".to_string(),
            ));
        }
    }
    Ok(resolved)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_path() {
        assert_eq!(
            normalize_path(Path::new("/a/b/../c")),
            PathBuf::from("/a/c")
        );
        assert_eq!(
            normalize_path(Path::new("./a/./b")),
            PathBuf::from("a/b")
        );
        assert_eq!(
            normalize_path(Path::new("a/./b/../c")),
            PathBuf::from("a/c")
        );
    }

    #[test]
    fn test_resolve_path_with_sandbox() {
        // 正常情况
        let result = resolve_path(
            Some("/workspace"),
            Some("/workspace"),
            "subdir/file.txt",
        );
        assert!(result.is_ok());

        // 尝试逃逸沙盒
        let result = resolve_path(
            Some("/workspace"),
            Some("/workspace"),
            "../etc",
        );
        assert!(result.is_err()); // 应该失败
    }

    #[test]
    fn test_resolve_path_absolute() {
        let result = resolve_path(None, None, "/etc/passwd");
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), PathBuf::from("/etc/passwd"));
    }

    #[test]
    fn test_resolve_path_relative() {
        let result = resolve_path(Some("/home/user"), None, "docs/readme.md");
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), PathBuf::from("/home/user/docs/readme.md"));
    }
}
