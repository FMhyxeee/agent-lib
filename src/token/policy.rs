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

    /// 根据模型自动设置截断策略
    ///
    /// 根据常见的 AI 模型自动设置合适的上下文窗口大小。
    /// 如果模型名称未知，使用默认值 (128k tokens)。
    ///
    /// # 参数
    /// * `model` - 模型名称，如 "gpt-4", "gpt-4-turbo", "gpt-3.5-turbo" 等
    ///
    /// # 返回
    /// 适合该模型的截断策略
    pub fn for_model(model: &str) -> Self {
        let limit = match model {
            // GPT-4 系列
            "gpt-4" | "gpt-4-0314" | "gpt-4-32k" => 8192,
            "gpt-4-turbo" | "gpt-4-1106-preview" | "gpt-4-0125-preview" => 128000,
            "gpt-4-vision-preview" => 128000,

            // GPT-3.5 系列
            "gpt-3.5-turbo" | "gpt-3.5-turbo-0301" => 4097,
            "gpt-3.5-turbo-16k" => 16385,
            "gpt-3.5-turbo-1106" | "gpt-3.5-turbo-0125" => 16385,

            // Claude 系列
            "claude-2" | "claude-2.0" | "claude-2.1" => 100000,
            "claude-instant-1" | "claude-instant-1.2" => 100000,
            "claude-3-haiku" | "claude-3-haiku-20240307" => 100000,
            "claude-3-sonnet" | "claude-3-sonnet-20240229" => 200000,
            "claude-3-opus" | "claude-3-opus-20240229" => 200000,

            // Gemini 系列
            "gemini-pro" | "gemini-1.0-pro" => 30720,
            "gemini-pro-1.5" | "gemini-1.5-pro" => 2097152, // 2M tokens

            // 默认（保守估计）
            _ => 128000,
        };
        Self::tokens(limit)
    }

    /// 检查是否超过指定的 token 数量
    ///
    /// # 参数
    /// * `count` - 要检查的 token 数量
    ///
    /// # 返回
    /// 如果超过限制返回 true，否则返回 false
    pub fn exceeds(&self, count: usize) -> bool {
        count > self.token_budget()
    }

    /// 按百分比保留 token 预算
    ///
    /// 计算在总预算中按百分比保留的 token 数量。
    /// 主要用于在压缩历史时保留一定比例的空间给系统消息等。
    ///
    /// # 参数
    /// * `total` - 总 token 数（如模型的上下文窗口）
    /// * `reserve_percent` - 保留的百分比（0-100）
    ///
    /// # 返回
    /// 可用 token 数量的截断策略
    pub fn with_reserve(total: usize, reserve_percent: usize) -> Self {
        let limit = (total * (100 - reserve_percent) / 100) as u64;
        Self::tokens(limit)
    }

    /// 计算剩余的 token 空间
    ///
    /// 返回当前预算与使用数量之间的差值。
    /// 如果超出预算返回负值。
    ///
    /// # 参数
    /// * `current_count` - 当前已使用的 token 数量
    ///
    /// # 返回
    /// 剩余的 token 数量（可能为负值）
    pub fn remaining(&self, current_count: usize) -> isize {
        let budget = self.token_budget();
        budget as isize - current_count as isize
    }

    /// 检查是否有足够的 token 空间
    ///
    /// # 参数
    /// * `required` - 所需的 token 数量
    ///
    /// # 返回
    /// 如果有足够空间返回 true，否则返回 false
    pub fn has_enough(&self, required: usize) -> bool {
        self.remaining(required) >= 0
    }

    /// 估算可以容纳多少个平均长度的消息
    ///
    /// # 参数
    /// * `avg_tokens_per_message` - 每条消息的平均 token 数量
    ///
    /// # 返回
    /// 估计可以容纳的消息数量
    pub fn estimate_message_capacity(&self, avg_tokens_per_message: usize) -> usize {
        if avg_tokens_per_message == 0 {
            0
        } else {
            self.token_budget() / avg_tokens_per_message
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

    #[test]
    fn test_for_model() {
        let gpt4 = TruncationPolicy::for_model("gpt-4");
        assert_eq!(gpt4.token_budget(), 8192);

        let gpt4_turbo = TruncationPolicy::for_model("gpt-4-turbo");
        assert_eq!(gpt4_turbo.token_budget(), 128000);

        let gpt35 = TruncationPolicy::for_model("gpt-3.5-turbo");
        assert_eq!(gpt35.token_budget(), 4097);

        let claude = TruncationPolicy::for_model("claude-2");
        assert_eq!(claude.token_budget(), 100000);

        let unknown = TruncationPolicy::for_model("unknown-model");
        assert_eq!(unknown.token_budget(), 128000); // 默认值
    }

    #[test]
    fn test_exceeds() {
        let policy = TruncationPolicy::tokens(1000);

        assert!(!policy.exceeds(500));
        assert!(!policy.exceeds(1000));
        assert!(policy.exceeds(1001));
        assert!(policy.exceeds(5000));
    }

    #[test]
    fn test_with_reserve() {
        let policy = TruncationPolicy::with_reserve(10000, 20); // 保留20%，使用80%
        assert_eq!(policy.token_budget(), 8000);

        let policy = TruncationPolicy::with_reserve(10000, 50); // 保留50%
        assert_eq!(policy.token_budget(), 5000);

        let policy = TruncationPolicy::with_reserve(10000, 0); // 不保留
        assert_eq!(policy.token_budget(), 10000);
    }

    #[test]
    fn test_remaining() {
        let policy = TruncationPolicy::tokens(1000);

        assert_eq!(policy.remaining(500), 500);
        assert_eq!(policy.remaining(1000), 0);
        assert_eq!(policy.remaining(1500), -500);
    }

    #[test]
    fn test_has_enough() {
        let policy = TruncationPolicy::tokens(1000);

        assert!(policy.has_enough(500));
        assert!(policy.has_enough(1000));
        assert!(!policy.has_enough(1001));
        assert!(!policy.has_enough(1500));
    }

    #[test]
    fn test_estimate_message_capacity() {
        let policy = TruncationPolicy::tokens(1000);

        assert_eq!(policy.estimate_message_capacity(100), 10); // 1000 / 100 = 10
        assert_eq!(policy.estimate_message_capacity(200), 5); // 1000 / 200 = 5
        assert_eq!(policy.estimate_message_capacity(0), 0); // 避免除零
        assert_eq!(policy.estimate_message_capacity(1500), 0); // 1000 / 1500 = 0
    }

    #[test]
    fn test_different_models() {
        // 测试多种模型的配置
        let models = vec![
            "gpt-4",
            "gpt-4-32k",
            "gpt-4-turbo",
            "gpt-3.5-turbo",
            "gpt-3.5-turbo-16k",
            "claude-2",
            "claude-instant-1",
            "gemini-pro",
            "gemini-pro-1.5",
        ];

        for model in models {
            let policy = TruncationPolicy::for_model(model);
            assert!(
                policy.token_budget() > 0,
                "Model {} should have a positive token budget",
                model
            );
        }
    }
}
