//! 应用的装配：业务路由、OpenAPI 文档、前端静态资源。
//!
//! 测试里用 [`builder`] 拿到装配好但还没监听端口的 `App`，配合 `TestClient` 直接调路由。

use rust_embed::RustEmbed;
use silent_boot::prelude::*;

#[derive(OpenApi)]
#[openapi(
    info(title = "{{ title }}", version = "0.1.0"),
    paths(
        crate::api::article::page,
        crate::api::article::create,
        crate::api::article::detail,
        crate::api::article::update,
        crate::api::article::remove
    ),
    components(schemas(
        crate::dto::article::ArticleDto,
        crate::dto::article::CreateArticleDto,
        crate::dto::article::UpdateArticleDto,
        Envelope<crate::dto::article::ArticleDto>,
        Envelope<PageResult<crate::dto::article::ArticleDto>>
    ))
)]
struct ApiDoc;

/// 前端的构建产物。release 编进二进制；debug 从 `web/dist` 读磁盘，重新构建前端不用重新编译后端。
#[derive(RustEmbed)]
#[folder = "web/dist/"]
struct WebAssets;

/// 按配置装配应用。`serve` 和测试共用这一份装配。
pub fn builder(config: AppConfig) -> AppBuilder {
    App::with_config(config)
        .migrator(migration::Migrator)
        .route(crate::api::routes())
        .openapi(ApiDoc::openapi())
        .assets(WebAssets)
}

/// 启动服务，直到收到 Ctrl-C 或 SIGTERM。
pub async fn serve(config: AppConfig) -> BootResult {
    builder(config).build().await?.run().await
}
