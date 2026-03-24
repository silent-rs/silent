//! # Silent OpenAPI
//!
//! 为Silent Web框架提供OpenAPI 3.0支持，包括自动文档生成和Swagger UI集成。
//!
//! ## 主要特性
//!
//! - 🚀 基于utoipa的高性能OpenAPI文档生成
//! - 📖 内置Swagger UI界面
//! - 🔧 与Silent框架深度集成
//! - 📝 支持路由文档自动收集
//! - 🎯 零运行时开销的编译时文档生成
//!
//! ## 快速开始
//!
//! ```ignore
//! use silent::prelude::*;
//! use silent_openapi::{OpenApiDoc, SwaggerUiHandler};
//! use utoipa::OpenApi;
//!
//! #[derive(OpenApi)]
//! #[openapi(
//!     paths(get_hello),
//!     components(schemas())
//! )]
//! struct ApiDoc;
//!
//! async fn get_hello(_req: Request) -> Result<Response> {
//!     Ok(Response::text("Hello, OpenAPI!"))
//! }
//!
//! fn main() {
//!     let route = Route::new("")
//!         .get(get_hello)
//!         .append(SwaggerUiHandler::new("/swagger-ui", ApiDoc::openapi()));
//!
//!     Server::new().run(route);
//! }
//! ```

pub mod doc;
#[cfg(feature = "swagger-ui-embedded")]
pub mod embedded;
pub use silent_openapi_macros::endpoint;
pub mod error;
pub mod handler;
pub mod middleware;
pub mod redoc;
pub mod route;
pub mod schema;
pub mod ui_html;

// 重新导出核心类型
pub use error::{OpenApiError, Result};
pub use handler::SwaggerUiHandler;
pub use middleware::SwaggerUiMiddleware;
pub use redoc::{ReDocHandler, ReDocMiddleware};
pub use route::{DocumentedRoute, RouteDocumentation, RouteOpenApiExt};
pub use schema::OpenApiDoc;

// 重新导出utoipa的核心类型，方便用户使用
pub use utoipa::{
    IntoParams, IntoResponses, ToResponse, ToSchema,
    openapi::{self, OpenApi},
};

/// Silent OpenAPI的版本信息
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Swagger UI 配置选项
#[derive(Clone)]
pub struct SwaggerUiOptions {
    pub try_it_out_enabled: bool,
}

impl Default for SwaggerUiOptions {
    fn default() -> Self {
        Self {
            try_it_out_enabled: true,
        }
    }
}

/// 创建一个基础的OpenAPI文档结构
///
/// # 参数
///
/// - `title`: API标题
/// - `version`: API版本
/// - `description`: API描述
///
/// # 示例
///
/// ```rust
/// use silent_openapi::create_openapi_doc;
///
/// let openapi = create_openapi_doc(
///     "用户管理API",
///     "1.0.0",
///     "基于Silent框架的用户管理系统"
/// );
/// ```
pub fn create_openapi_doc(
    title: &str,
    version: &str,
    description: &str,
) -> utoipa::openapi::OpenApi {
    use utoipa::openapi::*;

    OpenApiBuilder::new()
        .info(
            InfoBuilder::new()
                .title(title)
                .version(version)
                .description(Some(description))
                .build(),
        )
        .build()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_version() {
        #[allow(clippy::const_is_empty)]
        {
            assert!(!VERSION.is_empty());
        }
    }

    #[test]
    fn test_create_openapi_doc() {
        let doc = create_openapi_doc("Test API", "1.0.0", "A test API");
        assert_eq!(doc.info.title, "Test API");
        assert_eq!(doc.info.version, "1.0.0");
        assert_eq!(doc.info.description, Some("A test API".to_string()));
    }
}
