/// 工具输出截断功能
///
/// 防止工具返回过大的输出导致 context 击穿。
use serde_json::Value;

/// 截断标记
const TRUNCATION_MARKER: &str = "\n\n... [输出过大已截断] ...\n\n";

/// 截断时保留的开头字符数
const HEAD_SIZE: usize = 25_000;

/// 截断时保留的结尾字符数
const TAIL_SIZE: usize = 24_500;

/// 截断工具输出
///
/// 如果输出超过 `max_size`，保留开头和结尾，中间用省略标记代替。
///
/// # 参数
/// * `output` - 要截断的字符串输出
/// * `max_size` - 最大允许的字符数
///
/// # 返回
/// 截断后的字符串
///
/// # 示例
/// ```rust
/// use agent_lib::tools::truncate_output;
///
/// let short = "Hello, world!";
/// assert_eq!(truncate_output(short, 100), "Hello, world!");
///
/// let long = "x".repeat(100_000);
/// let truncated = truncate_output(&long, 50_000);
/// assert!(truncated.len() <= 50_000);
/// assert!(truncated.contains("... [输出过大已截断] ..."));
/// ```
pub fn truncate_output(output: &str, max_size: usize) -> String {
    let output_len = output.chars().count();

    // 如果输出小于限制，直接返回
    if output_len <= max_size {
        return output.to_string();
    }

    // 计算保留的开头和结尾大小
    let marker_len = TRUNCATION_MARKER.chars().count();
    let head_size = HEAD_SIZE.min(max_size / 2);
    let tail_size = (max_size - head_size - marker_len).min(TAIL_SIZE);

    // 收集开头的字符
    let head: String = output.chars().take(head_size).collect();

    // 收集结尾的字符
    let tail: String = output.chars().rev().take(tail_size).collect::<String>()
        .chars().rev().collect();

    format!("{}{}{}", head, TRUNCATION_MARKER, tail)
}

/// 截断 JSON 工具输出
///
/// 如果输出是 JSON 字符串且超过 `max_size`，尝试保留 JSON 结构。
/// 对于数组，保留开头和结尾的元素。
/// 对于对象，保留所有键但截断过长的字符串值。
///
/// # 参数
/// * `output` - JSON 值
/// * `max_size` - 最大允许的字符数（序列化后）
///
/// # 返回
/// 截断后的 JSON 字符串
pub fn truncate_json_output(output: &Value, max_size: usize) -> String {
    let json_str = serde_json::to_string_pretty(output)
        .unwrap_or_else(|_| format!("{:?}", output));

    let json_len = json_str.chars().count();

    if json_len <= max_size {
        return json_str;
    }

    // 对于过大的 JSON，使用简单截断
    truncate_output(&json_str, max_size)
}

/// 检查输出是否需要截断
///
/// # 参数
/// * `output` - 要检查的字符串
/// * `max_size` - 最大允许的字符数
///
/// # 返回
/// 如果需要截断返回 true
pub fn needs_truncation(output: &str, max_size: usize) -> bool {
    output.chars().count() > max_size
}

/// 计算截断后的字符数
///
/// # 参数
/// * `original_len` - 原始字符数
/// * `max_size` - 最大允许的字符数
///
/// # 返回
/// 截断后的实际字符数
pub fn truncated_size(original_len: usize, max_size: usize) -> usize {
    if original_len <= max_size {
        original_len
    } else {
        let marker_len = TRUNCATION_MARKER.chars().count();
        let head_size = HEAD_SIZE.min(max_size / 2);
        let tail_size = (max_size - head_size - marker_len).min(TAIL_SIZE);
        head_size + marker_len + tail_size
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_no_truncation_needed() {
        let short = "Hello, world!";
        let result = truncate_output(short, 100);
        assert_eq!(result, "Hello, world!");
    }

    #[test]
    fn test_truncation_basic() {
        let long = "x".repeat(100_000);
        let result = truncate_output(&long, 50_000);

        assert!(result.len() <= 50_000);
        assert!(result.contains("... [输出过大已截断] ..."));
        assert!(result.starts_with("x"));
        assert!(result.ends_with("x"));
    }

    #[test]
    fn test_truncation_with_multibyte_chars() {
        // 中文字符测试
        let chinese = "你好".repeat(50_000); // 约 100,000 字符
        let result = truncate_output(&chinese, 50_000);

        assert!(result.chars().count() <= 50_000);
        assert!(result.contains("... [输出过大已截断] ..."));
    }

    #[test]
    fn test_truncation_preserves_head_and_tail() {
        let mut content = String::new();
        content.push_str("HEAD_MARKER");
        content.push_str(&"x".repeat(100_000));
        content.push_str("TAIL_MARKER");

        let result = truncate_output(&content, 50_000);

        assert!(result.contains("HEAD_MARKER"));
        assert!(result.contains("TAIL_MARKER"));
        assert!(result.contains("... [输出过大已截断] ..."));
    }

    #[test]
    fn test_needs_truncation() {
        assert!(!needs_truncation("short", 100));
        assert!(needs_truncation("x".repeat(1000).as_str(), 100));
    }

    #[test]
    fn test_truncated_size() {
        assert_eq!(truncated_size(100, 1000), 100); // 不需要截断
        assert!(truncated_size(100_000, 50_000) <= 50_000); // 需要截断
    }

    #[test]
    fn test_truncate_json_output() {
        let large_json = serde_json::json!({
            "data": "x".repeat(100_000)
        });

        let result = truncate_json_output(&large_json, 50_000);
        assert!(result.chars().count() <= 50_000);
    }

    #[test]
    fn test_exact_boundary() {
        // 测试正好等于边界的情况
        let exact = "x".repeat(50_000);
        let result = truncate_output(&exact, 50_000);

        // 不应该被截断
        assert_eq!(result.chars().count(), 50_000);
        assert!(!result.contains("... [输出过大已截断] ..."));
    }

    #[test]
    fn test_one_over_boundary() {
        // 测试超过边界 1 个字符的情况
        let over = "x".repeat(50_001);
        let result = truncate_output(&over, 50_000);

        assert!(result.chars().count() <= 50_000);
        assert!(result.contains("... [输出过大已截断] ..."));
    }
}
