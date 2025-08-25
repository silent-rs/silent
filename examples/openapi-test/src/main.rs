use serde::{Deserialize, Serialize};
use silent::header;
use silent::prelude::*;
use silent_openapi::{OpenApiDoc, RouteOpenApiExt, SwaggerUiHandler, SwaggerUiOptions, ToSchema};

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

// 本示例将使用路由自动生成 OpenAPI，再补充安全定义

async fn get_hello(_req: Request) -> Result<Response> {
    Ok(Response::text("Hello, OpenAPI!"))
}

async fn get_user(req: Request) -> Result<Response> {
    let id: u64 = req.get_path_params("id").unwrap_or(1);
    let user = User {
        id,
        name: format!("User {}", id),
    };
    Ok(Response::json(&user))
}

// 受保护端点：无 Authorization 返回 401，带特殊 token 返回 403，其它通过
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

    // 先构建业务路由
    let routes = Route::new("")
        .get(get_hello)
        .append(Route::new("users").append(Route::new("<id:u64>").get(get_user)))
        .append(Route::new("protected").get(get_protected));

    // 基于路由生成 OpenAPI，并补充 Bearer 安全定义与全局 security
    let openapi = routes.to_openapi("Test API", "1.0.0");
    let openapi = OpenApiDoc::from_openapi(openapi)
        .add_bearer_auth("bearerAuth", Some("JWT Bearer token"))
        .set_global_security("bearerAuth", &[])
        .into_openapi();

    // 可选：关闭 Try it out（生产环境常用）
    let options = SwaggerUiOptions {
        try_it_out_enabled: true,
    };
    let swagger = SwaggerUiHandler::with_options("/docs", openapi, options)
        .expect("Failed to create Swagger UI");

    // 直接将 SwaggerUiHandler 转为可挂载的路由树并追加
    let routes = Route::new("").append(swagger.into_route()).append(routes);

    println!("🚀 Server starting!");
    println!("📖 API docs: http://localhost:8080/docs");
    println!("🔗 Endpoints:");
    println!("   GET /hello");
    println!("   GET /users/{{id}}");
    println!("   GET /protected    - 401/403 示例: Authorization: Bearer <token>");
    println!("      - 无头: 401; token 含 'forbidden': 403; 其他: 200");

    let addr = "127.0.0.1:8080".parse().expect("Invalid address");
    Server::new().bind(addr).serve(routes).await;
    Ok(())
}
