use serde::{Deserialize, Serialize};
use silent::extractor::Path;
use silent::header;
use silent::prelude::*;
use silent_openapi::{
    endpoint, OpenApiDoc, ReDocHandler, RouteOpenApiExt, SwaggerUiHandler, SwaggerUiOptions,
    ToSchema,
};

#[derive(Serialize, Deserialize, ToSchema)]
struct User {
    id: u64,
    name: String,
}

#[derive(Serialize, Deserialize, ToSchema)]
struct ErrorResponse {
    code: String,
    message: String,
}

// 使用 tags 将端点分组
#[endpoint(summary = "获取问候", description = "返回问候语", tags = "general")]
async fn get_hello(_req: Request) -> Result<String> {
    Ok("Hello, OpenAPI!".into())
}

// 使用 response 声明多状态码响应
#[endpoint(
    summary = "获取用户",
    description = "根据路径参数 id 返回用户信息",
    tags = "users",
    response(status = 404, description = "用户不存在")
)]
async fn get_user(Path(id): Path<u64>) -> Result<User> {
    Ok(User {
        id,
        name: format!("User {}", id),
    })
}

// 使用 deprecated 标记废弃接口
#[endpoint(
    deprecated,
    summary = "旧版获取用户（已废弃）",
    description = "请使用 GET /users/{id} 替代",
    tags = "users"
)]
async fn get_user_legacy(Path(id): Path<u64>) -> Result<User> {
    Ok(User {
        id,
        name: format!("User {}", id),
    })
}

// 多状态码响应声明
#[endpoint(
    summary = "受保护示例",
    description = "演示 401/403 与成功的不同响应",
    tags = "auth",
    response(status = 401, description = "未提供认证信息"),
    response(status = 403, description = "权限不足")
)]
async fn get_protected(req: Request) -> Result<Response> {
    let auth = req
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());
    match auth {
        None => {
            let body = ErrorResponse {
                code: "UNAUTHORIZED".into(),
                message: "missing Authorization".into(),
            };
            Ok(Response::json(&body).with_status(StatusCode::UNAUTHORIZED))
        }
        Some(value) if value.contains("forbidden") => {
            let body = ErrorResponse {
                code: "FORBIDDEN".into(),
                message: "token not allowed".into(),
            };
            Ok(Response::json(&body).with_status(StatusCode::FORBIDDEN))
        }
        Some(_) => Ok(Response::text("ok")),
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    logger::fmt().init();

    // 构建业务路由
    let routes = Route::new("")
        .get(get_hello)
        .append(
            Route::new("users")
                .append(Route::new("<id:u64>").get(get_user))
                .append(Route::new("legacy/<id:u64>").get(get_user_legacy)),
        )
        .append(Route::new("protected").get(get_protected));

    // 基于路由生成 OpenAPI，并补充安全定义
    let openapi = routes.to_openapi("Test API", "1.0.0");
    let openapi = OpenApiDoc::from_openapi(openapi)
        .add_bearer_auth("bearerAuth", Some("JWT Bearer token"))
        .set_global_security("bearerAuth", &[])
        .into_openapi();

    // Swagger UI（/docs）
    let options = SwaggerUiOptions {
        try_it_out_enabled: true,
    };
    let swagger = SwaggerUiHandler::with_options("/docs", openapi.clone(), options)
        .expect("Failed to create Swagger UI");

    // ReDoc（/redoc）— 共用同一 OpenAPI 规范，两种 UI 并存
    let redoc = ReDocHandler::new("/redoc", openapi).expect("Failed to create ReDoc");

    let routes = Route::new("")
        .append(swagger.into_route())
        .append(redoc.into_route())
        .append(routes);

    println!("Server starting!");
    println!("API docs:");
    println!("   Swagger UI: http://localhost:8080/docs");
    println!("   ReDoc:      http://localhost:8080/redoc");
    println!("Endpoints:");
    println!("   GET /                       - general");
    println!("   GET /users/{{id}}             - users");
    println!("   GET /users/legacy/{{id}}      - users (deprecated)");
    println!("   GET /protected              - auth (401/403)");

    let addr = "127.0.0.1:8080".parse().expect("Invalid address");
    Server::new().bind(addr).serve(routes).await;
    Ok(())
}
