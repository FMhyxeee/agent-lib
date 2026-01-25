/// 粗略的 token 计数 (Codex 原版: ~4 bytes/token)
///
/// 这是一个快速估算方法，适合不需要精确计数的场景。
/// 根据经验值，大约每 4 个字节对应 1 个 token。
pub fn approx_token_count(text: &str) -> usize {
    const APPROX_BYTES_PER_TOKEN: usize = 4;
    text.len().saturating_add(APPROX_BYTES_PER_TOKEN - 1) / APPROX_BYTES_PER_TOKEN
}

/// 精确的 token 计数 (使用 tiktoken-rs)
///
/// 当启用 `codex-compat` feature 时使用 tiktoken-rs 进行精确计数，
/// 否则回退到近似计数。
pub fn tiktoken_count(text: &str) -> usize {
    #[cfg(feature = "codex-compat")]
    {
        // 使用 tiktoken-rs 进行精确计数
        use tiktoken_rs::cl100k_base;

        let bpe = cl100k_base().unwrap();
        bpe.encode_with_special_tokens(text).len()
    }

    #[cfg(not(feature = "codex-compat"))]
    {
        // 回退到近似计数
        approx_token_count(text)
    }
}

/// Token 计数器
///
/// 提供灵活的 token 计数方式，可以选择使用精确计数或近似计数。
#[derive(Clone, Debug)]
pub struct TokenCounter {
    use_tiktoken: bool,
}

impl TokenCounter {
    /// 创建新的 Token 计数器
    ///
    /// # 参数
    /// * `use_tiktoken` - 是否使用 tiktoken 进行精确计数
    pub fn new(use_tiktoken: bool) -> Self {
        Self { use_tiktoken }
    }

    /// 使用 tiktoken 创建计数器（如果可用）
    pub fn with_tiktoken() -> Self {
        Self { use_tiktoken: true }
    }

    /// 使用近似计数创建计数器
    pub fn with_approx() -> Self {
        Self {
            use_tiktoken: false,
        }
    }

    /// 计算文本的 token 数量
    pub fn count(&self, text: &str) -> usize {
        if self.use_tiktoken {
            tiktoken_count(text)
        } else {
            approx_token_count(text)
        }
    }
}

impl Default for TokenCounter {
    fn default() -> Self {
        Self::with_approx()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_approx_token_count() {
        assert_eq!(approx_token_count("hello world"), 3);
        assert_eq!(approx_token_count(""), 0);
        assert_eq!(approx_token_count("a"), 1);
    }

    #[test]
    fn test_token_counter_default() {
        let counter = TokenCounter::default();
        assert_eq!(counter.count("hello world"), 3);
    }

    #[test]
    fn test_token_counter_approx() {
        let counter = TokenCounter::with_approx();
        assert_eq!(counter.count("hello world"), 3);
    }

    #[test]
    fn test_token_counter_tiktoken() {
        let counter = TokenCounter::with_tiktoken();
        // tiktoken 会给出不同的结果，但应该非零
        assert!(counter.count("hello world") > 0);
    }
}
