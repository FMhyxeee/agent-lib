//! Handler模块 - 处理各种Op类型的handler函数
//!
//! 将原本在loop.rs中的大型handler函数拆分到独立模块中，
//! 提高代码可维护性和可测试性。

pub mod mcp;
pub mod skill;
pub mod session;
pub mod approval;
pub mod interaction;
pub mod system;
