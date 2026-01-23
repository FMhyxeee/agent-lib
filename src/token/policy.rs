use serde::{Deserialize, Serialize};

/// Token 截断策略
///
/// 用于控制如何截断过长的对话历史以适应模型的上下文窗口。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TruncationPolicy {
    pub mode: TruncationMode,
    pub limit: u64,
}

impl TruncationPolicy {
    /// 创建新的截断策略
    pub fn new(mode: TruncationMode) -> Self {
        let limit = mode.limit();
        Self { mode, limit }
    }

    /// 创建基于 Token 的截断策略
    pub fn tokens(limit: u64) -> Self {
        Self {
            mode: TruncationMode::Tokens(limit),
            limit,
        }
    }

    /// 创建基于字节的截断策略
    pub fn bytes(limit: u64) -> Self {
        Self {
            mode: TruncationMode::Bytes(limit),
            limit,
        }
    }

    /// 计算实际的 token 预算
    ///
    /// 根据截断模式返回实际的 token 数量限制。
    pub fn token_budget(&self) -> usize {
        match self.mode {
            TruncationMode::Tokens(tokens) => tokens as usize,
            TruncationMode::Bytes(bytes) => (bytes / 4) as usize,
        }
    }
}

/// 截断模式
///
/// 定义截断限制的单位类型。
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TruncationMode {
    /// 基于 Token 数量截断
    Tokens(u64),
    /// 基于字节数截断
    Bytes(u64),
}

impl TruncationMode {
    pub fn limit(&self) -> u64 {
        match self {
            TruncationMode::Tokens(t) => *t,
            TruncationMode::Bytes(b) => *b,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_truncation_policy_tokens() {
        let policy = TruncationPolicy::tokens(1000);
        assert_eq!(policy.token_budget(), 1000);
        assert!(matches!(policy.mode, TruncationMode::Tokens(1000)));
    }

    #[test]
    fn test_truncation_policy_bytes() {
        let policy = TruncationPolicy::bytes(4000);
        assert_eq!(policy.token_budget(), 1000);
        assert!(matches!(policy.mode, TruncationMode::Bytes(4000)));
    }

    #[test]
    fn test_truncation_policy_new() {
        let policy = TruncationPolicy::new(TruncationMode::Tokens(500));
        assert_eq!(policy.token_budget(), 500);
        assert_eq!(policy.limit, 500);
    }
}
