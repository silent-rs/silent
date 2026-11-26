//! silent-boot：基于 Silent 的全栈快速开发框架。
//!
//! 当前处于 M0（验证与定稿）阶段，模块随里程碑逐步补齐：
//! - [`db`]：连接管理与迁移，迁移是数据库结构的唯一事实来源。
//!
//! 规划见仓库根目录的 `PLAN.md`，技术方案见 `docs/design.md`。

pub mod db;

#[cfg(feature = "cli")]
pub mod cli;
