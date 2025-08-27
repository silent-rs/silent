//! 用户管理API示例
//!
//! 这是一个更专注的示例，展示如何构建一个完整的用户管理API。
//!
//! 运行方式：
//! ```bash
//! cargo run --example user_api
//! ```

use serde::{Deserialize, Serialize};
use silent::prelude::*;
use silent_openapi::{SwaggerUiMiddleware, ToSchema};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use utoipa::OpenApi;

/// 用户数据
#[derive(Serialize, Deserialize, ToSchema, Clone, Debug)]
struct User {
    id: u64,
    name: String,
    email: String,
    created_at: String,
}

/// 创建/更新用户请求
#[derive(Deserialize, Serialize, ToSchema, Debug)]
struct UserRequest {
    name: String,
    email: String,
}

/// 简单的内存存储
type UserStore = Arc<RwLock<HashMap<u64, User>>>;

/// API文档
#[derive(OpenApi)]
#[openapi(
    info(
        title = "用户管理API",
        version = "1.0.0",
        description = "简单的用户CRUD操作API"
    ),
    paths(list_users, get_user, create_user, delete_user),
    components(schemas(User, UserRequest))
)]
struct ApiDoc;

/// 获取用户列表
#[utoipa::path(
    get,
    path = "/users",
    responses(
        (status = 200, description = "用户列表", body = [User])
    )
)]
async fn list_users(req: Request) -> Result<Response> {
    let store = req.get_config::<UserStore>()?;
    let users: Vec<User> = store.read().await.values().cloned().collect();
    Ok(Response::json(&users))
}

/// 获取单个用户
#[utoipa::path(
    get,
    path = "/users/{id}",
    params(("id" = u64, Path, description = "用户ID")),
    responses(
        (status = 200, description = "用户信息", body = User),
        (status = 404, description = "用户不存在")
    )
)]
async fn get_user(req: Request) -> Result<Response> {
    let id: u64 = req.get_path_params("id")?;
    let store = req.get_config::<UserStore>().unwrap();

    if let Some(user) = store.read().await.get(&id) {
        Ok(Response::json(user))
    } else {
        Ok(Response::text("User not found").with_status(StatusCode::NOT_FOUND))
    }
}

/// 创建用户
#[utoipa::path(
    post,
    path = "/users",
    request_body = UserRequest,
    responses(
        (status = 201, description = "用户创建成功", body = User)
    )
)]
async fn create_user(mut req: Request) -> Result<Response> {
    let user_req: UserRequest = req.json_parse().await?;
    let store = req.get_config::<UserStore>().unwrap();

    let mut store_write = store.write().await;
    let id = store_write.len() as u64 + 1;

    let user = User {
        id,
        name: user_req.name,
        email: user_req.email,
        created_at: "2025-01-30T12:00:00Z".to_string(),
    };

    store_write.insert(id, user.clone());

    Ok(Response::json(&user).with_status(StatusCode::CREATED))
}

/// 删除用户
#[utoipa::path(
    delete,
    path = "/users/{id}",
    params(("id" = u64, Path, description = "用户ID")),
    responses(
        (status = 204, description = "删除成功"),
        (status = 404, description = "用户不存在")
    )
)]
async fn delete_user(req: Request) -> Result<Response> {
    let id: u64 = req.get_path_params("id")?;
    let store = req.get_config::<UserStore>().unwrap();

    if store.write().await.remove(&id).is_some() {
        Ok(Response::empty().with_status(StatusCode::NO_CONTENT))
    } else {
        Ok(Response::text("User not found").with_status(StatusCode::NOT_FOUND))
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    logger::fmt().init();

    // 创建用户存储并添加一些示例数据
    let mut initial_data = HashMap::new();
    initial_data.insert(
        1,
        User {
            id: 1,
            name: "Alice".to_string(),
            email: "alice@example.com".to_string(),
            created_at: "2025-01-30T12:00:00Z".to_string(),
        },
    );
    initial_data.insert(
        2,
        User {
            id: 2,
            name: "Bob".to_string(),
            email: "bob@example.com".to_string(),
            created_at: "2025-01-30T12:01:00Z".to_string(),
        },
    );
    let store: UserStore = Arc::new(RwLock::new(initial_data));

    // 创建Swagger UI中间件
    let swagger_middleware =
        SwaggerUiMiddleware::new("/docs", ApiDoc::openapi()).expect("创建Swagger UI中间件失败");

    // 构建路由
    let routes = Route::new("")
        .hook(swagger_middleware) // 使用 root_hook 添加全局中间件
        .append(
            Route::new("users")
                .get(list_users)
                .post(create_user)
                .append(Route::new("<id:u64>").get(get_user).delete(delete_user)),
        );

    println!("🚀 用户管理API启动！");
    println!("📖 文档地址: http://localhost:8080/docs");
    println!("🔗 API端点:");
    println!("   GET    /users      - 获取用户列表");
    println!("   POST   /users      - 创建用户");
    println!("   GET    /users/{{id}} - 获取用户");
    println!("   DELETE /users/{{id}} - 删除用户");

    // 配置服务器并启动
    let addr = "127.0.0.1:8080".parse().expect("Invalid address");
    let mut configs = Configs::default();
    configs.insert(store);

    Server::new()
        .bind(addr)
        .with_configs(configs)
        .serve(routes)
        .await;

    Ok(())
}
