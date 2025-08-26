//! 展示如何在 OpenAPI 文档中添加 Bearer/JWT 安全定义并设置全局 security。

use silent::prelude::*;
use silent_openapi::{OpenApiDoc, RouteOpenApiExt, SwaggerUiHandler};

#[tokio::main]
async fn main() -> silent::Result<()> {
    // 模拟一个需要鉴权的 API
    async fn protected(_req: Request) -> silent::Result<Response> {
        Ok(Response::text("ok"))
    }

    // 基于路由自动生成基础文档，并补充安全定义
    let route = Route::new("api").append(Route::new("protected").get(protected));

    let openapi = route.to_openapi("Secure API", "1.0.0");

    // 包装为 OpenApiDoc 以便添加安全配置
    let openapi = OpenApiDoc::from_openapi(openapi)
        .add_bearer_auth("bearerAuth", Some("JWT Bearer token"))
        .set_global_security("bearerAuth", &[])
        .into_openapi();

    let swagger = SwaggerUiHandler::new("/docs", openapi).expect("create swagger ui");

    let app = Route::new("")
        .append(
            Route::new("docs")
                .insert_handler(silent::Method::GET, std::sync::Arc::new(swagger.clone()))
                .insert_handler(silent::Method::HEAD, std::sync::Arc::new(swagger.clone())),
        )
        .append(
            Route::new("docs/<path:**>")
                .insert_handler(silent::Method::GET, std::sync::Arc::new(swagger.clone()))
                .insert_handler(silent::Method::HEAD, std::sync::Arc::new(swagger)),
        )
        .append(route);

    let addr = "127.0.0.1:8080".parse().expect("Invalid address");
    Server::new().bind(addr).serve(app).await;
    Ok(())
}
