/// 固定模型配置
///
/// 项目当前支持以下模型:
/// - 标准GLM API (glm provider): glm-4.7, glm-4.7-flashx
/// - Coding Plan API (glm-coding provider): glm-4.7-coding, glm-4.7-flashx-coding
///
/// Coding Plan 模型需要单独的订阅,使用不同的端点。
use serde::{Deserialize, Serialize};

/// 支持的模型列表
pub const SUPPORTED_MODELS: &[ModelConfig] = &[
    // GLM-5 Series (Latest flagship, SOTA)
    ModelConfig {
        id: "glm-5",
        display_name: "GLM-5",
        provider: "glm",
        context_window: 128_000,
        supports_streaming: true,
        supports_tools: true,
    },
    ModelConfig {
        id: "glm-5-coding",
        display_name: "GLM-5 (Coding Plan)",
        provider: "glm-coding",
        context_window: 128_000,
        supports_streaming: true,
        supports_tools: true,
    },
    // GLM-4.7 Series
    ModelConfig {
        id: "glm-4.7",
        display_name: "GLM-4.7",
        provider: "glm",
        context_window: 200_000,
        supports_streaming: true,
        supports_tools: true,
    },
    ModelConfig {
        id: "glm-4.7-flashx",
        display_name: "GLM-4.7-FlashX",
        provider: "glm",
        context_window: 200_000,
        supports_streaming: true,
        supports_tools: true,
    },
    ModelConfig {
        id: "glm-4.7-coding",
        display_name: "GLM-4.7 (Coding Plan)",
        provider: "glm-coding",
        context_window: 200_000,
        supports_streaming: true,
        supports_tools: true,
    },
    ModelConfig {
        id: "glm-4.7-flashx-coding",
        display_name: "GLM-4.7-FlashX (Coding Plan)",
        provider: "glm-coding",
        context_window: 200_000,
        supports_streaming: true,
        supports_tools: true,
    },
];

/// 模型配置信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelConfig {
    /// 模型 ID (用于 API 调用)
    pub id: &'static str,
    /// 显示名称
    pub display_name: &'static str,
    /// 提供商 (glm, openai, etc.)
    pub provider: &'static str,
    /// 上下文窗口大小 (tokens)
    pub context_window: usize,
    /// 是否支持流式输出
    pub supports_streaming: bool,
    /// 是否支持工具调用
    pub supports_tools: bool,
}

/// 获取模型的配置信息
pub fn get_model_config(model_id: &str) -> Option<&'static ModelConfig> {
    SUPPORTED_MODELS.iter().find(|m| m.id == model_id)
}

/// 获取模型的上下文窗口大小
pub fn get_context_window(model_id: &str) -> usize {
    get_model_config(model_id)
        .map(|m| m.context_window)
        .unwrap_or(200_000) // 默认 200K
}

/// 检查模型是否受支持
pub fn is_model_supported(model_id: &str) -> bool {
    get_model_config(model_id).is_some()
}

/// 列出所有支持的模型
pub fn list_models() -> &'static [ModelConfig] {
    SUPPORTED_MODELS
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_glm_47_config() {
        let config = get_model_config("glm-4.7").unwrap();
        assert_eq!(config.id, "glm-4.7");
        assert_eq!(config.context_window, 200_000);
        assert_eq!(config.provider, "glm");
    }

    #[test]
    fn test_glm_47_flashx_config() {
        let config = get_model_config("glm-4.7-flashx").unwrap();
        assert_eq!(config.id, "glm-4.7-flashx");
        assert_eq!(config.context_window, 200_000);
        assert_eq!(config.display_name, "GLM-4.7-FlashX");
    }

    #[test]
    fn test_unknown_model_returns_default() {
        let cw = get_context_window("unknown-model");
        assert_eq!(cw, 200_000);
    }

    #[test]
    fn test_is_model_supported() {
        assert!(is_model_supported("glm-4.7"));
        assert!(is_model_supported("glm-4.7-flashx"));
        assert!(!is_model_supported("gpt-4"));
    }

    #[test]
    fn test_list_models() {
        let models = list_models();
        assert_eq!(models.len(), 6);
    }

    #[test]
    fn test_glm5_config() {
        let config = get_model_config("glm-5").unwrap();
        assert_eq!(config.id, "glm-5");
        assert_eq!(config.context_window, 128_000);
        assert_eq!(config.provider, "glm");
    }

    #[test]
    fn test_glm5_coding_config() {
        let config = get_model_config("glm-5-coding").unwrap();
        assert_eq!(config.id, "glm-5-coding");
        assert_eq!(config.context_window, 128_000);
        assert_eq!(config.provider, "glm-coding");
    }

    #[test]
    fn test_glm_coding_config() {
        let config = get_model_config("glm-4.7-coding").unwrap();
        assert_eq!(config.id, "glm-4.7-coding");
        assert_eq!(config.context_window, 200_000);
        assert_eq!(config.provider, "glm-coding");
    }

    #[test]
    fn test_glm_flashx_coding_config() {
        let config = get_model_config("glm-4.7-flashx-coding").unwrap();
        assert_eq!(config.id, "glm-4.7-flashx-coding");
        assert_eq!(config.context_window, 200_000);
        assert_eq!(config.provider, "glm-coding");
    }
}
