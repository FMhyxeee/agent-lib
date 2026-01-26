/// 全局缓存的 cl100k_base BPE 分词器
///
/// 使用 once_cell 缓存，避免重复初始化开销。
static CL100K_BASE: once_cell::sync::Lazy<tiktoken_rs::CoreBPE> =
    once_cell::sync::Lazy::new(|| {
        tiktoken_rs::cl100k_base().expect("Failed to initialize cl100k_base BPE")
    });

/// 精确的 token 计数 (使用 tiktoken)
///
/// 使用 cl100k_base BPE 进行精确计数，这是 GPT-4/GPT-3.5 使用的分词器。
pub fn count_tokens(text: &str) -> usize {
    CL100K_BASE.encode_with_special_tokens(text).len()
}

/// Token 计数器
///
/// 提供灵活的 token 计数方式。
#[derive(Clone, Debug, Default)]
pub struct TokenCounter;

impl TokenCounter {
    /// 创建新的 Token 计数器
    pub fn new() -> Self {
        Self
    }

    /// 计算单个文本的 token 数量
    pub fn count(&self, text: &str) -> usize {
        count_tokens(text)
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
    /// 估算的 token 数量 (约 4 bytes/token)
    pub fn estimate_from_bytes(&self, bytes: usize) -> usize {
        const APPROX_BYTES_PER_TOKEN: usize = 4;
        bytes.saturating_add(APPROX_BYTES_PER_TOKEN - 1) / APPROX_BYTES_PER_TOKEN
    }

    /// 获取当前计数模式的描述
    ///
    /// # 返回
    /// 描述当前计数模式的字符串
    pub fn mode_name(&self) -> &'static str {
        "tiktoken(cl100k_base)"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_count_tokens() {
        // 英文
        assert_eq!(count_tokens("hello world"), 2);
        assert_eq!(count_tokens(""), 0);

        // 中文
        assert!(count_tokens("你好") > 0);
        assert!(count_tokens("你好世界") > 0);

        // 代码
        let code = "fn add(a: i32, b: i32) -> i32 { a + b }";
        assert!(count_tokens(code) > 0);
    }

    #[test]
    fn test_token_counter_default() {
        let counter = TokenCounter::default();
        assert!(counter.count("hello world") > 0);
    }

    #[test]
    fn test_count_batch() {
        let counter = TokenCounter::new();
        let texts = ["hello", "world", "test"];
        let texts_refs: Vec<&str> = texts.to_vec();

        let expected = counter.count("hello") + counter.count("world") + counter.count("test");
        let actual = counter.count_batch(&texts_refs);
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_estimate_from_bytes() {
        let counter = TokenCounter::new();
        assert_eq!(counter.estimate_from_bytes(0), 0);
        assert_eq!(counter.estimate_from_bytes(4), 1);
        assert_eq!(counter.estimate_from_bytes(8), 2);
        assert_eq!(counter.estimate_from_bytes(12), 3);
    }

    #[test]
    fn test_mode_name() {
        let counter = TokenCounter::new();
        assert_eq!(counter.mode_name(), "tiktoken(cl100k_base)");
    }
}
