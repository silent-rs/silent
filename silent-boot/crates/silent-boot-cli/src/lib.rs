//! `silent-boot` 全局工具的库部分，`main.rs` 只负责解析参数。
//!
//! - [`new`]：生成应用的骨架，模板在 [`templates`]；
//! - [`dev`]：构建并运行应用，监听源码变更，自动重新构建、重启，并通过 [`events`] 通知页面刷新；
//! - [`forward`]：在应用目录里把 `migrate`、`generate`、`healthcheck` 转发给应用自己的子命令。

pub mod dev;
pub mod events;
pub mod forward;
pub mod new;
pub mod templates;
pub mod watch;
