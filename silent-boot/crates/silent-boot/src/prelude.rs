//! 最常用的类型。应用的 `api`、`services`、`dto` 里写一句 `use silent_boot::prelude::*;` 就够。
//!
//! [`Result`] 是 Silent 的 `Result<T, SilentError>`，handler 的返回类型用它；
//! service 层的返回类型用 [`AppResult`]（`Result<T, AppError>`）。

pub use silent::prelude::*;
// 提取器 `State<T>` 在 Silent 的 prelude 里被它的底层容器 `State` 遮住了，这里换成 handler 用的那个。
pub use silent::Server;
pub use silent::extractor::{Json, Path, Query, State};
// 生成的代码直接用这些名字；版本和框架带进来的保持一致，应用不必再各写一份依赖。
pub use chrono::{self, NaiveDateTime};
pub use serde::{Deserialize, Serialize};
pub use silent;
pub use validator::Validate;
pub use {scru128, sea_orm, serde_json, validator};

pub use crate::app::{App, AppBuilder, AppState, BootError, BootResult};
pub use crate::config::AppConfig;
pub use crate::db::DbRegistry;
pub use crate::web::pagination::paginate;
pub use crate::web::{
    AppError, AppResult, Created, Envelope, Page, PageQuery, PageResult, ValidatedJson,
    ValidatedQuery,
};

/// 写 OpenAPI 文档要用的 trait；文档本身是 `#[derive(OpenApi)]` 加 `::openapi()` 得到的 [`OpenApiDocument`]。
#[cfg(feature = "openapi")]
pub use utoipa::{
    self, IntoParams, IntoResponses, OpenApi, ToResponse, ToSchema,
    openapi::OpenApi as OpenApiDocument,
};
