//! 路径处理模块
//!
//! 提供统一的路径标准化和解析功能。

pub mod utils;

pub use utils::{normalize_path, resolve_path};
