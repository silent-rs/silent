# Silent OpenAPI

🚀 为 [Silent Web Framework](https://github.com/silent-rs/silent) 提供 OpenAPI 3.0 支持和 Swagger UI 集成。

## ✨ 特性

- 🔧 **深度集成** - 与 Silent 框架无缝集成
- 📖 **自动文档** - 基于 [utoipa](https://github.com/juhaku/utoipa) 的编译时文档生成
- 🖥️ **Swagger UI** - 内置美观的交互式 API 文档界面
- 🚀 **零运行时开销** - 编译时生成，运行时高性能
- 🎯 **易于使用** - 简单的 API 和丰富的示例
- 🌐 **中文支持** - 完整的中文文档和错误消息

## 📦 安装

在你的 `Cargo.toml` 中添加：

```toml
[dependencies]
silent = "2.5"
silent-openapi = "0.1"
utoipa = { version = "4.2", features = ["derive"] }
serde = { version = "1.0", features = ["derive"] }
tokio = { version = "1.0", features = ["full"] }
```

## 🚀 快速开始

### 基础使用

```rust
use serde::{Deserialize, Serialize};
use silent::prelude::*;
use silent_openapi::{SwaggerUiMiddleware, ToSchema};
use utoipa::OpenApi;

#[derive(Serialize, Deserialize, ToSchema)]
struct User {
    id: u64,
    name: String,
    email: String,
}

#[derive(OpenApi)]
#[openapi(
    info(title = "用户API", version = "1.0.0"),
    paths(get_users, create_user),
    components(schemas(User))
)]
struct ApiDoc;

#[utoipa::path(
    get,
    path = "/users",
    responses((status = 200, description = "用户列表", body = [User]))
)]
async fn get_users(_req: Request) -> Result<Response> {
    let users = vec![
        User { id: 1, name: "张三".to_string(), email: "zhangsan@example.com".to_string() }
    ];
    Ok(Response::json(&users))
}

#[utoipa::path(
    post,
    path = "/users",
    request_body = User,
    responses((status = 201, description = "用户创建成功", body = User))
)]
async fn create_user(mut req: Request) -> Result<Response> {
    let user: User = req.form_parse().await?;
    Ok(Response::json(&user).with_status(StatusCode::CREATED))
}

#[tokio::main]
async fn main() -> Result<()> {
    logger::fmt().init();

    // 创建 Swagger UI 中间件
    let swagger = SwaggerUiMiddleware::new("/docs", ApiDoc::openapi())?;

    // 构建路由
    let routes = Route::new("")
        .hook(swagger)  // 添加 Swagger UI
        .append(
            Route::new("users")
                .get(get_users)
                .post(create_user)
        );

    println!("📖 API 文档: http://localhost:8080/docs");

    Server::new().run(routes);
    Ok(())
}
```

### 使用处理器方式

```rust
use silent_openapi::SwaggerUiHandler;

// 创建 Swagger UI 处理器
let swagger_handler = SwaggerUiHandler::new("/api-docs", ApiDoc::openapi())?;

let routes = Route::new("")
    .append(Route::new("api-docs").any(swagger_handler))
    .append(your_api_routes);
```

## 📚 详细用法

### 定义数据模型

使用 `ToSchema` derive 宏为你的数据结构生成 OpenAPI 模式：

```rust
use silent_openapi::ToSchema;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, ToSchema)]
#[schema(example = json!({
    "id": 1,
    "name": "张三",
    "email": "zhangsan@example.com"
}))]
struct User {
    /// 用户 ID
    #[schema(minimum = 1)]
    id: u64,

    /// 用户名
    #[schema(min_length = 1, max_length = 50)]
    name: String,

    /// 邮箱地址
    #[schema(format = "email")]
    email: String,
}
```

### 文档化 API 端点

使用 `utoipa::path` 宏为你的处理函数生成文档：

```rust
#[utoipa::path(
    get,
    path = "/users/{id}",
    tag = "users",
    summary = "获取用户信息",
    description = "根据用户 ID 获取用户详细信息",
    params(
        ("id" = u64, Path, description = "用户 ID", example = 1)
    ),
    responses(
        (status = 200, description = "成功获取用户信息", body = User),
        (status = 404, description = "用户不存在", body = ErrorResponse)
    )
)]
async fn get_user(req: Request) -> Result<Response> {
    let id: u64 = req.get_path_params("id")?;
    // 处理逻辑...
}
```

### 定义 OpenAPI 文档

```rust
#[derive(OpenApi)]
#[openapi(
    info(
        title = "用户管理 API",
        version = "1.0.0",
        description = "一个简单的用户管理系统 API",
        contact(
            name = "API Support",
            email = "support@example.com"
        )
    ),
    servers(
        (url = "http://localhost:8080", description = "开发服务器"),
        (url = "https://api.example.com", description = "生产服务器")
    ),
    paths(
        get_users,
        get_user,
        create_user,
        update_user,
        delete_user
    ),
    components(
        schemas(User, CreateUserRequest, ErrorResponse)
    ),
    tags(
        (name = "users", description = "用户管理相关 API")
    )
)]
struct ApiDoc;

### 路由自动生成 OpenAPI + 安全定义 + Try it out 开关

无需手写 `#[derive(OpenApi)]`，可以直接从路由生成基础文档，并补充安全定义：

```rust
use silent_openapi::{RouteOpenApiExt, OpenApiDoc, SwaggerUiMiddleware, SwaggerUiOptions};

// 1) 先构建业务路由
let routes = Route::new("")
    .append(Route::new("users").get(list_users))
    .append(Route::new("users").append(Route::new("<id:u64>").get(get_user)));

// 2) 基于路由生成 OpenAPI 并添加 Bearer(JWT) 安全定义 + 全局 security
let openapi = routes.to_openapi("User API", "1.0.0");
let openapi = OpenApiDoc::from_openapi(openapi)
    .add_bearer_auth("bearerAuth", Some("JWT Bearer token"))
    .set_global_security("bearerAuth", &[])
    .into_openapi();

// 3) 自定义 UI 选项（如关闭 Try it out）并挂载到 /docs
let options = SwaggerUiOptions { try_it_out_enabled: false };
let swagger = SwaggerUiMiddleware::with_options("/docs", openapi, options)?;
let app = Route::new("").hook(swagger).append(routes);
```
```

## 🎨 配置选项

### Swagger UI 自定义

```rust
// 使用自定义路径
let swagger = SwaggerUiMiddleware::with_custom_api_doc_path(
    "/docs",           // Swagger UI 路径
    "/openapi.json",   // OpenAPI JSON 路径
    ApiDoc::openapi()
)?;
```

### 多种集成方式

1. **中间件方式** - 推荐用于全局文档
2. **处理器方式** - 推荐用于特定路由下的文档

## 📖 示例

查看 `examples/` 目录中的完整示例：

- `basic_openapi.rs` - 基础集成示例
- `user_api.rs` - 完整的用户管理 API

运行示例：

```bash
# 基础示例
cargo run --example basic_openapi

# 用户 API 示例
cargo run --example user_api
```

## 🔒 生产环境建议

- 关闭交互尝试：将 `try_it_out_enabled` 设为 `false`，避免未授权的在线调用。
- 保护文档入口：将 `/docs` 放在受保护的子路由或网关后，或在上游加鉴权（如 Basic/JWT）。
- 安全定义：在 OpenAPI 中声明 `bearerAuth` 并设置全局 `security`，与实际网关/服务策略一致。
- CORS 与缓存：为 `/openapi.json` 设置合理的 `Cache-Control`，并按需配置 CORS；避免缓存过期导致前端文档不一致。
- 环境隔离：为 dev/stage/prod 设置不同的 `servers`，并确保敏感接口在非生产环境才开放 `Try it out`。

## 🛠️ 支持的特性

### OpenAPI 3.0 特性

- ✅ 路径和操作定义
- ✅ 请求/响应模式
- ✅ 参数验证
- ✅ 标签和分组
- ✅ 示例数据
- ✅ 服务器配置
- ✅ 安全定义（计划中）

### Swagger UI 特性

- ✅ 交互式 API 测试
- ✅ 模式浏览
- ✅ 请求/响应示例
- ✅ 中文界面支持
- ✅ 响应式设计
- ✅ CDN 资源加载

## 🔧 高级用法

### 错误处理

```rust
use silent_openapi::{OpenApiError, Result};

fn handle_openapi_error(error: OpenApiError) -> Response {
    match error {
        OpenApiError::Json(e) => {
            Response::json(&format!("JSON 错误: {}", e))
                .with_status(StatusCode::BAD_REQUEST)
        }
        OpenApiError::ResourceNotFound { resource } => {
            Response::json(&format!("资源未找到: {}", resource))
                .with_status(StatusCode::NOT_FOUND)
        }
        _ => {
            Response::json("内部服务器错误")
                .with_status(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}
```

### 路由文档收集

```rust
use silent_openapi::{RouteDocumentation, OpenApiDoc};

// 从现有路由生成文档
let doc = my_route.generate_openapi_doc(
    "My API",
    "1.0.0",
    Some("API description")
);
```

## 🚦 版本兼容性

| silent-openapi | silent | utoipa |
|---------------|---------|---------|
| 0.1.x         | 2.5.x   | 4.2.x   |

## 🤝 贡献

欢迎贡献代码、报告问题或提出建议！

1. Fork 项目
2. 创建特性分支 (`git checkout -b feature/amazing-feature`)
3. 提交更改 (`git commit -m 'Add amazing feature'`)
4. 推送分支 (`git push origin feature/amazing-feature`)
5. 创建 Pull Request

## 📄 许可证

本项目采用 MIT 或 Apache-2.0 双许可证。详见 [LICENSE](../LICENSE) 文件。

## 🔗 相关链接

- [Silent Web Framework](https://github.com/silent-rs/silent)
- [utoipa - OpenAPI for Rust](https://github.com/juhaku/utoipa)
- [OpenAPI 3.0 规范](https://swagger.io/specification/)
- [Swagger UI](https://swagger.io/tools/swagger-ui/)

---

<div align="center">
Made with ❤️ for the Rust community
</div>
