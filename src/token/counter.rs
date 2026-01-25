/// 粗略的 token 计数 (Codex 原版: ~4 bytes/token)
///
/// 这是一个快速估算方法，适合不需要精确计数的场景。
/// 根据经验值，大约每 4 个字节对应 1 个 token。
pub fn approx_token_count(text: &str) -> usize {
    const APPROX_BYTES_PER_TOKEN: usize = 4;
    text.len().saturating_add(APPROX_BYTES_PER_TOKEN - 1) / APPROX_BYTES_PER_TOKEN
}

/// 全局缓存的 cl100k_base BPE 分词器
///
/// 使用 once_cell 缓存，避免重复初始化开销。
#[cfg(feature = "codex-compat")]
static CL100K_BASE: once_cell::sync::Lazy<tiktoken_rs::CoreBPE> =
    once_cell::sync::Lazy::new(|| {
        tiktoken_rs::cl100k_base().expect("Failed to initialize cl100k_base BPE")
    });

/// 精确的 token 计数 (使用 tiktoken-rs)
///
/// 当启用 `codex-compat` feature 时使用 tiktoken-rs 进行精确计数，
/// 否则回退到近似计数。
pub fn tiktoken_count(text: &str) -> usize {
    #[cfg(feature = "codex-compat")]
    {
        // 使用全局缓存的 BPE 进行精确计数
        CL100K_BASE.encode_with_special_tokens(text).len()
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

    /// 自动选择最佳计数方法
    ///
    /// 根据是否启用 `codex-compat` feature 自动选择最合适的计数方法：
    /// - 如果启用 codex-compat，使用 tiktoken 精确计数
    /// - 否则使用近似计数
    pub fn auto() -> Self {
        #[cfg(feature = "codex-compat")]
        {
            Self::with_tiktoken()
        }
        #[cfg(not(feature = "codex-compat"))]
        {
            Self::with_approx()
        }
    }

    /// 批量计算多个文本的 token 数量
    ///
    /// # 参数
    /// * `texts` - 要计数的文本切片
    ///
    /// # 返回
    /// 所有文本的 token 数量总和
    pub fn count_batch(&self, texts: &[&str]) -> usize {
        texts.iter().map(|t| self.count(t)).sum()
    }

    /// 从字节数估算 token 数量
    ///
    /// 使用近似方法从字节数估算 token 数量。
    /// 主要在没有实际内容但需要预估时使用。
    ///
    /// # 参数
    /// * `bytes` - 字节数
    ///
    /// # 返回
    /// 估算的 token 数量
    pub fn estimate_from_bytes(&self, bytes: usize) -> usize {
        const APPROX_BYTES_PER_TOKEN: usize = 4;
        bytes.saturating_add(APPROX_BYTES_PER_TOKEN - 1) / APPROX_BYTES_PER_TOKEN
    }

    /// 获取当前计数模式的描述
    ///
    /// # 返回
    /// 描述当前计数模式的字符串
    pub fn mode_name(&self) -> &'static str {
        if self.use_tiktoken {
            #[cfg(feature = "codex-compat")]
            return "tiktoken(cl100k_base)";

            #[cfg(not(feature = "codex-compat"))]
            return "tiktoken(unavailable)";
        }
        "approx(4bytes/token)"
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

    #[test]
    fn test_token_counter_auto() {
        let counter = TokenCounter::auto();
        assert!(counter.count("hello world") > 0);
    }

    #[test]
    fn test_count_batch() {
        let counter = TokenCounter::new(false);
        let texts = vec!["hello", "world", "test"];
        let texts_refs: Vec<&str> = texts.iter().copied().collect();

        let expected = counter.count("hello") + counter.count("world") + counter.count("test");
        let actual = counter.count_batch(&texts_refs);
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_estimate_from_bytes() {
        let counter = TokenCounter::new(false);
        assert_eq!(counter.estimate_from_bytes(0), 0);
        assert_eq!(counter.estimate_from_bytes(3), 1); // 3 bytes / 4 = 0.75 -> 1
        assert_eq!(counter.estimate_from_bytes(8), 2); // 8 bytes / 4 = 2
        assert_eq!(counter.estimate_from_bytes(12), 3); // 12 bytes / 4 = 3
    }

    #[test]
    fn test_mode_name() {
        let counter = TokenCounter::with_approx();
        assert_eq!(counter.mode_name(), "approx(4bytes/token)");

        let counter = TokenCounter::with_tiktoken();
        #[cfg(feature = "codex-compat")]
        assert_eq!(counter.mode_name(), "tiktoken(cl100k_base)");
        #[cfg(not(feature = "codex-compat"))]
        assert_eq!(counter.mode_name(), "tiktoken(unavailable)");
    }
}
