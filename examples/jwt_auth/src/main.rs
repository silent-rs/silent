use jsonwebtoken::Algorithm;
use serde::{Deserialize, Serialize};
use silent::extractor::Json;
use silent::middleware::middlewares::{Claims, Jwt, JwtBuilder, JwtUtils, OptionalJwt};
use silent::prelude::*;
use silent::{Result, SilentError, StatusCode};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// 用户登录请求
#[derive(Deserialize)]
struct LoginRequest {
    username: String,
    password: String,
}

/// 用户登录响应
#[derive(Serialize)]
struct LoginResponse {
    token: String,
    expires_in: u64,
}

/// 用户信息响应
#[derive(Serialize)]
struct UserInfo {
    id: String,
    username: String,
    roles: Vec<String>,
}

/// 简单的内存用户存储（生产环境应使用数据库）
#[derive(Clone)]
struct UserStore {
    users: Arc<Mutex<HashMap<String, User>>>,
}

#[derive(Clone)]
struct User {
    id: String,
    username: String,
    password_hash: String,
    roles: Vec<String>,
    permissions: Vec<String>,
}

impl UserStore {
    fn new() -> Self {
        let mut users = HashMap::new();

        // 添加一些示例用户
        users.insert(
            "admin".to_string(),
            User {
                id: "user_001".to_string(),
                username: "admin".to_string(),
                password_hash: "admin123".to_string(), // 生产环境应该使用哈希
                roles: vec!["admin".to_string(), "user".to_string()],
                permissions: vec![
                    "read".to_string(),
                    "write".to_string(),
                    "delete".to_string(),
                ],
            },
        );

        users.insert(
            "user".to_string(),
            User {
                id: "user_002".to_string(),
                username: "user".to_string(),
                password_hash: "user123".to_string(),
                roles: vec!["user".to_string()],
                permissions: vec!["read".to_string(), "write".to_string()],
            },
        );

        Self {
            users: Arc::new(Mutex::new(users)),
        }
    }

    fn authenticate(&self, username: &str, password: &str) -> Option<User> {
        let users = self.users.lock().unwrap();
        if let Some(user) = users.get(username) {
            if user.password_hash == password {
                // 生产环境应验证哈希
                Some(user.clone())
            } else {
                None
            }
        } else {
            None
        }
    }
}

/// 登录处理器
async fn login_handler(Json(req): Json<LoginRequest>) -> Result<LoginResponse> {
    // 创建用户存储实例（简单示例）
    let user_store = UserStore::new();

    // 验证用户凭证
    let user = user_store
        .authenticate(&req.username, &req.password)
        .ok_or_else(|| SilentError::business_error(StatusCode::UNAUTHORIZED, "用户名或密码错误"))?;

    // 创建JWT声明
    let custom_claims = serde_json::json!({
        "username": user.username,
        "roles": user.roles,
        "permissions": user.permissions
    });

    let claims = Claims::new(&user.id, 3600) // 1小时有效期
        .with_custom(custom_claims)
        .with_audience("silent-app")
        .with_issuer("silent-auth");

    // 生成JWT token
    let token = JwtUtils::encode(&claims, "your-secret-key", Algorithm::HS256)?;

    Ok(LoginResponse {
        token,
        expires_in: 3600,
    })
}

/// 获取当前用户信息（需要认证）
async fn user_info_handler(jwt: Jwt) -> Result<UserInfo> {
    let user_info = UserInfo {
        id: jwt.user_id().to_string(),
        username: jwt.get_claim::<String>("username").unwrap_or_default(),
        roles: jwt.roles(),
    };

    Ok(user_info)
}

/// 管理员专用处理器（需要admin角色）
async fn admin_handler(jwt: Jwt) -> Result<String> {
    if !jwt.has_role("admin") {
        return Err(SilentError::business_error(
            StatusCode::FORBIDDEN,
            "需要管理员权限",
        ));
    }

    Ok(format!(
        "Hello Admin {}! 这是管理员专用页面。",
        jwt.user_id()
    ))
}

/// 可选认证处理器（认证可选）
async fn optional_auth_handler(jwt: OptionalJwt) -> Result<String> {
    match jwt.0 {
        Some(jwt_claims) => Ok(format!(
            "Hello, {}! 你已经登录了。角色: {:?}",
            jwt_claims.user_id(),
            jwt_claims.roles()
        )),
        None => Ok("Hello, anonymous user! 你可以登录获得更多功能。".to_string()),
    }
}

/// 删除操作（需要delete权限）
async fn delete_handler(jwt: Jwt) -> Result<String> {
    if !jwt.has_permission("delete") {
        return Err(SilentError::business_error(
            StatusCode::FORBIDDEN,
            "需要删除权限",
        ));
    }

    Ok("删除操作已执行！".to_string())
}

/// 健康检查处理器（无需认证）
async fn health_handler(_req: Request) -> Result<Response> {
    Ok(Response::text("OK"))
}

/// 首页处理器（无需认证）
async fn home_handler(_req: Request) -> Result<Response> {
    Ok(Response::text(
        r#"
JWT认证示例API

可用端点：
- POST /auth/login    - 用户登录
- GET  /auth/user     - 获取用户信息（需要认证）
- GET  /admin         - 管理员页面（需要admin角色）
- GET  /optional      - 可选认证页面
- DELETE /delete      - 删除操作（需要delete权限）
- GET  /health        - 健康检查（无需认证）

示例用户：
- admin/admin123 (管理员)
- user/user123   (普通用户)

使用方法：
1. 先调用 POST /auth/login 获取token
2. 在请求头中添加: Authorization: Bearer <token>
3. 访问需要认证的端点
"#,
    ))
}

#[tokio::main]
async fn main() -> Result<()> {
    // 初始化日志
    tracing_subscriber::fmt::init();

    // 创建JWT认证中间件
    let jwt_auth = JwtBuilder::new("your-secret-key")
        .algorithm(Algorithm::HS256)
        .audience("silent-app")
        .issuer("silent-auth")
        .skip_path("/") // 首页
        .skip_path("/health") // 健康检查
        .skip_path("/auth/login") // 登录页面
        .skip_path("/optional") // 可选认证页面
        .build();

    // 创建路由
    let mut app = Route::new("");
    app.push(Route::new("/").get(home_handler));
    app.push(Route::new("/health").get(health_handler));

    let mut auth_route = Route::new("/auth");
    auth_route.push(Route::new("/login").post(login_handler));

    let mut user_route = Route::new("/user");
    user_route = user_route.get(user_info_handler).hook(jwt_auth.clone());
    auth_route.push(user_route);

    app.push(auth_route);
    app.push(Route::new("/optional").get(optional_auth_handler));

    let admin_route = Route::new("/admin")
        .get(admin_handler)
        .hook(jwt_auth.clone());
    app.push(admin_route);

    let delete_route = Route::new("/delete").delete(delete_handler).hook(jwt_auth);
    app.push(delete_route);

    // 启动服务器
    let addr = "127.0.0.1:3000".parse().unwrap();
    println!("🚀 JWT认证示例服务器启动在 http://127.0.0.1:3000");
    println!("📖 访问 http://127.0.0.1:3000 查看使用说明");

    Server::new().bind(addr).serve(app).await;

    Ok(())
}
