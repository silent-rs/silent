//! silent-boot：基于 Silent 的全栈快速开发框架。
//!
//! 模块随里程碑逐步补齐，当前有：
//! - [`app`]：启动器，读配置、连库、装配中间件和路由、优雅关停；
//! - [`config`]：约定配置，`config/app.toml` 加 `APP__` 前缀的环境变量；
//! - [`db`]：连接管理与迁移，迁移是数据库结构的唯一事实来源；
//! - [`web`]：统一响应与错误、带校验的提取器、分页、静态资源、运维端点。
//!
//! 最常用的类型在 [`prelude`] 里。规划见仓库根目录的 `PLAN.md`，技术方案见 `docs/design.md`。
pub mod app;
#[cfg(feature = "cli")]
pub mod cli;
pub mod config;
pub mod db;
pub mod logging;
pub mod prelude;
pub mod web;
