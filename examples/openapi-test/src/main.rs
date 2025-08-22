use serde::{Deserialize, Serialize};
use silent::prelude::*;
use silent_openapi::{SwaggerUiMiddleware, ToSchema};
use utoipa::OpenApi;

#[derive(Serialize, Deserialize, ToSchema)]
struct User {
    id: u64,
    name: String,
}

#[derive(OpenApi)]
#[openapi(info(title = "Test API", version = "1.0.0"), components(schemas(User)))]
struct ApiDoc;

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

#[tokio::main]
async fn main() -> Result<()> {
    logger::fmt().init();

    let swagger =
        SwaggerUiMiddleware::new("/docs", ApiDoc::openapi()).expect("Failed to create Swagger UI");

    let routes = Route::new("")
        .hook(swagger)
        .get(get_hello)
        .append(Route::new("users").append(Route::new("<id:u64>").get(get_user)));

    println!("🚀 Server starting!");
    println!("📖 API docs: http://localhost:8080/docs");
    println!("🔗 Endpoints:");
    println!("   GET /hello");
    println!("   GET /users/{{id}}");

    let addr = "127.0.0.1:8080".parse().expect("Invalid address");
    Server::new().bind(addr).serve(routes).await;
    Ok(())
}
