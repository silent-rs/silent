//! 简单的OpenAPI示例
//!
//! 展示最基本的OpenAPI集成用法。
//!
//! 运行方式：
//! ```bash
//! cargo run --example simple_example
//! ```

use serde::{Deserialize, Serialize};
use silent::prelude::*;
use silent_openapi::{SwaggerUiMiddleware, ToSchema};
use utoipa::OpenApi;

/// 用户数据模型
#[derive(Serialize, Deserialize, ToSchema)]
struct User {
    id: u64,
    name: String,
}

/// 简单的API响应
#[derive(Serialize, ToSchema)]
struct ApiResponse {
    message: String,
    status: String,
}

/// API文档定义
#[derive(OpenApi)]
#[openapi(
    info(
        title = "Simple API Demo",
        version = "1.0.0",
        description = "一个使用Silent框架和OpenAPI的简单示例"
    ),
    components(schemas(User, ApiResponse))
)]
struct ApiDoc;

/// Hello World端点
async fn hello(_req: Request) -> Result<Response> {
    let response = ApiResponse {
        message: "Hello, Silent OpenAPI!".to_string(),
        status: "success".to_string(),
    };
    Ok(Response::json(&response))
}

/// 获取用户信息
async fn get_user(req: Request) -> Result<Response> {
    let id: u64 = req.get_path_params("id").unwrap_or(1);

    let user = User {
        id,
        name: format!("User {}", id),
    };

    Ok(Response::json(&user))
}

/// 健康检查
async fn health_check(_req: Request) -> Result<Response> {
    Ok(Response::text("OK"))
}

#[tokio::main]
async fn main() -> Result<()> {
    // 初始化日志
    logger::fmt().init();

    println!("🚀 启动简单的OpenAPI示例...");

    // 创建Swagger UI中间件
    let swagger_middleware = SwaggerUiMiddleware::new("/swagger-ui", ApiDoc::openapi())
        .expect("创建Swagger UI中间件失败");

    // 构建路由
    let routes = Route::new("")
        .hook(swagger_middleware) // 添加Swagger UI中间件
        .get(hello) // 根路径
        .append(Route::new("health").get(health_check)) // 健康检查
        .append(
            Route::new("users").append(Route::new("<id:u64>").get(get_user)), // 用户详情
        );

    println!("📖 API文档地址:");
    println!("   Swagger UI: http://localhost:8080/swagger-ui");
    println!("   OpenAPI JSON: http://localhost:8080/swagger-ui/openapi.json");
    println!();
    println!("🔗 API端点:");
    println!("   GET    /              - Hello World");
    println!("   GET    /health        - 健康检查");
    println!("   GET    /users/{{id}}    - 获取用户信息");
    println!();
    println!("✨ 服务启动成功！按 Ctrl+C 停止服务");

    // 启动服务器
    let addr = "127.0.0.1:8080".parse().expect("Invalid address");
    Server::new().bind(addr)?.serve(routes).await;

    Ok(())
}
