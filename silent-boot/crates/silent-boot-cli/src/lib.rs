//! `silent-boot` 全局工具的库部分，`main.rs` 只负责解析参数。
//!
//! 目前只有 `dev` 运行循环（[`dev`]）：构建并运行应用，监听源码变更，自动重新构建、
//! 重启，并通过 [`events`] 通知页面刷新。

pub mod dev;
pub mod events;
pub mod watch;
