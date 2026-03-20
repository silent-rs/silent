# Silent OpenAPI 集成

当用户需要为 Silent 项目添加 OpenAPI 文档和 Swagger UI 时，使用此 Skill。

## 依赖配置

```toml
[dependencies]
silent = "2.15"
silent-openapi = "2.15"
silent-openapi-macros = "2.15"
utoipa = { version = "5", features = ["preserve_order"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
```

## 基本用法

### 1. 定义数据模型（实现 ToSchema）

```rust
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Serialize, Deserialize, ToSchema)]
pub struct User {
    pub id: i64,
    pub name: String,
    pub email: String,
}

#[derive(Deserialize, ToSchema)]
pub struct CreateUserInput {
    pub name: String,
    pub email: String,
}
```

### 2. 使用 #[endpoint] 宏标注处理器

```rust
use silent_openapi_macros::endpoint;
use silent::prelude::*;
use silent::extractor::{Path, Json, Query};

/// 获取用户列表
#[endpoint(summary = "获取用户列表", description = "返回所有用户")]
async fn list_users(_req: Request) -> Result<Vec<User>> {
    Ok(vec![])
}

/// 根据 ID 获取用户
#[endpoint(summary = "获取用户详情")]
async fn get_user(Path(id): Path<i64>) -> Result<User> {
    Ok(User { id, name: "test".into(), email: "test@example.com".into() })
}

/// 创建用户
#[endpoint(summary = "创建用户")]
async fn create_user(Json(input): Json<CreateUserInput>) -> Result<User> {
    Ok(User { id: 1, name: input.name, email: input.email })
}
```

### 3. 生成 OpenAPI 文档并挂载 Swagger UI

```rust
use silent_openapi::route::RouteOpenApiExt;
use silent_openapi::handler::SwaggerUiHandler;
use silent_openapi::schema::OpenApiDoc;

fn main() {
    logger::fmt().with_max_level(Level::INFO).init();

    // 构建业务路由
    let api_routes = Route::new("api")
        .append(Route::new("users")
            .get(list_users)
            .post(create_user)
            .append(Route::new("<id:i64>").get(get_user))
        );

    // 从路由自动生成 OpenAPI 文档
    let openapi = api_routes.to_openapi("My API", "1.0.0");

    // 可选：添加安全定义
    let openapi = OpenApiDoc::from_openapi(openapi)
        .add_bearer_auth("bearerAuth", Some("JWT Bearer token"))
        .set_global_security("bearerAuth", &[])
        .into_openapi();

    // 创建 Swagger UI 处理器
    let swagger = SwaggerUiHandler::new("/docs", openapi).unwrap();

    // 组合路由
    let route = Route::new("")
        .append(swagger.into_route())
        .append(api_routes);

    Server::new().run(route);
}
```

访问 `http://localhost:8080/docs` 即可查看 Swagger UI。

## #[endpoint] 宏说明

### 属性参数

```rust
#[endpoint]                                    // 自动从文档注释提取 summary
#[endpoint(summary = "描述")]                   // 手动指定 summary
#[endpoint(summary = "描述", description = "详细说明")]  // 同时指定
```

### 支持的处理器签名

```rust
// 纯 Request
#[endpoint]
async fn handler(req: Request) -> Result<Response> { }

// 单提取器
#[endpoint]
async fn handler(Path(id): Path<u64>) -> Result<User> { }

#[endpoint]
async fn handler(Json(body): Json<CreateUser>) -> Result<User> { }

#[endpoint]
async fn handler(Query(q): Query<Params>) -> Result<Vec<User>> { }

#[endpoint]
async fn handler(Form(data): Form<FormData>) -> Result<Response> { }

// Request + 提取器
#[endpoint]
async fn handler(req: Request, Json(body): Json<CreateUser>) -> Result<Response> { }
```

### 自动文档化行为

| 提取器 | 生成的 OpenAPI 内容 |
|--------|-------------------|
| `Path<T>` | path parameters（从路径字符串提取） |
| `Query<T>` | query parameters + schema 引用 |
| `Json<T>` | requestBody application/json + schema 引用 |
| `Form<T>` | requestBody application/x-www-form-urlencoded + schema 引用 |

### 返回类型自动识别

| 返回类型 | 生成的响应文档 |
|----------|--------------|
| `Result<Response>` | 无自动 schema |
| `Result<String>` | text/plain |
| `Result<CustomType>` | application/json + schema 引用 |

## Swagger UI 配置选项

```rust
use silent_openapi::handler::{SwaggerUiHandler, SwaggerUiOptions};

let options = SwaggerUiOptions {
    try_it_out_enabled: true,  // 启用 "Try it out" 交互式调试
};

let swagger = SwaggerUiHandler::with_options("/docs", openapi, options).unwrap();
```

## 中间件方式挂载

```rust
use silent_openapi::middleware::SwaggerUiMiddleware;

let swagger = SwaggerUiMiddleware::new("/docs", openapi).unwrap();
let route = Route::new("")
    .hook(swagger)
    .append(api_routes);
```
