//! Web 层：统一响应与错误、带校验的提取器、分页、静态资源、运维端点。
//!
//! - [`error`]：[`Envelope`](error::Envelope)、[`AppError`](error::AppError)、[`AppResult`](error::AppResult)；
//! - [`handler`]：[`TraceId`](handler::TraceId) 和 [`ErrorHandler`](handler::ErrorHandler) 两个中间件；
//! - [`extract`]：[`ValidatedJson`](extract::ValidatedJson)、[`ValidatedQuery`](extract::ValidatedQuery)；
//! - [`pagination`]：[`PageQuery`](pagination::PageQuery)、[`PageResult`](pagination::PageResult)；
//! - [`assets`]：前端静态资源与 SPA 兜底；
//! - [`health`]：`/healthz`、`/readyz`。
//!
//! 最常用的类型在 [`crate::prelude`] 里。

pub mod assets;
pub mod error;
pub mod extract;
pub mod handler;
pub mod health;
pub mod pagination;

pub use assets::{Asset, AssetSource};
pub use error::{AppError, AppResult, Created, Envelope};
pub use extract::{ValidatedJson, ValidatedQuery};
pub use handler::{ErrorHandler, TraceId};
pub use health::{Readiness, ReadinessReport};
pub use pagination::{Page, PageQuery, PageResult};
