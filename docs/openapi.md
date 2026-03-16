# Silent OpenAPI 使用指南

本文档介绍如何在 Silent Web 框架中集成 OpenAPI 文档生成与 Swagger UI，覆盖从基础接入到高级用法的完整流程。

## 目录

1. [依赖配置](#1-依赖配置)
2. [定义数据模型](#2-定义数据模型)
3. [使用 `#[endpoint]` 宏定义处理器](#3-使用-endpoint-宏定义处理器)
4. [生成 OpenAPI 文档](#4-生成-openapi-文档)
5. [挂载 Swagger UI](#5-挂载-swagger-ui)
6. [提取器自动集成](#6-提取器自动集成)
7. [枚举类型文档化](#7-枚举类型文档化)
8. [安全定义与全局配置](#8-安全定义与全局配置)
9. [文档说明的来源与优先级](#9-文档说明的来源与优先级)
10. [集成方式对比](#10-集成方式对比)
11. [完整示例](#11-完整示例)
12. [常见问题](#12-常见问题)

---

## 1. 依赖配置

在业务 crate 的 `Cargo.toml` 中添加：

```toml
[dependencies]
silent = { version = "2.13" }
silent-openapi = { version = "2.13" }
serde = { version = "1", features = ["derive"] }
utoipa = "5"   # silent-openapi 已复导出 ToSchema 等常用 trait，但直接引入 utoipa 可获取更多 derive 宏
```

`silent-openapi` 已复导出以下常用类型，无需额外引入：
- `ToSchema`、`IntoParams`、`ToResponse`、`IntoResponses`
- `OpenApi`（utoipa 的核心类型）

---

## 2. 定义数据模型

所有作为请求体或响应体的结构体/枚举需要派生 `ToSchema`：

```rust
use serde::{Serialize, Deserialize};
use silent_openapi::ToSchema;

#[derive(Serialize, Deserialize, ToSchema)]
struct User {
    id: u64,
    name: String,
    email: Option<String>,
}

#[derive(Serialize, Deserialize, ToSchema)]
struct CreateUserRequest {
    name: String,
    email: String,
}
```

---

## 3. 使用 `#[endpoint]` 宏定义处理器

`#[endpoint]` 是 Silent OpenAPI 的核心宏，它同时完成：
- 将函数包装为可注册到路由的端点常量
- 自动注册 summary、description、响应类型、请求体类型到全局文档注册表

### 基本用法

```rust
use silent::prelude::*;
use silent_openapi::endpoint;

#[endpoint(summary = "健康检查", description = "返回服务运行状态")]
async fn health(_req: Request) -> Result<String> {
    Ok("ok".into())
}
```

### 属性参数

| 参数 | 类型 | 说明 |
|------|------|------|
| `summary` | 字符串 | 接口摘要，显示在 Swagger UI 列表中 |
| `description` | 字符串 | 接口详细描述 |

两个参数都是可选的，未指定时会从函数的 `///` 文档注释中自动提取（见[第 9 节](#9-文档说明的来源与优先级)）。

### 支持的处理器形态

`#[endpoint]` 支持三种函数签名：

```rust
// 形态 1：直接接收 Request
#[endpoint(summary = "获取首页")]
async fn index(_req: Request) -> Result<Response> {
    Ok(Response::text("Hello"))
}

// 形态 2：单个提取器参数（Path / Query / Json / Form）
#[endpoint(summary = "获取用户")]
async fn get_user(Path(id): Path<u64>) -> Result<User> {
    Ok(User { id, name: format!("User {}", id), email: None })
}

// 形态 3：Request + 提取器
#[endpoint(summary = "创建用户")]
async fn create_user(_req: Request, Json(body): Json<CreateUserRequest>) -> Result<User> {
    Ok(User { id: 1, name: body.name, email: Some(body.email) })
}
```

### 返回类型与 OpenAPI 响应映射

| 返回类型 | OpenAPI 响应 |
|---------|-------------|
| `Result<String>` / `Result<&str>` | `200` + `text/plain` |
| `Result<T>`（`T` 实现 `ToSchema`） | `200` + `application/json` + `$ref` 引用 |
| `Result<Response>` | `200` 默认响应（无 content-type 约束） |

---

## 4. 生成 OpenAPI 文档

### 方式一：从路由自动生成（推荐）

使用 `RouteOpenApiExt::to_openapi` 直接从路由树生成完整的 OpenAPI 文档：

```rust
use silent_openapi::RouteOpenApiExt;

let routes = Route::new("")
    .append(Route::new("health").get(health))
    .append(Route::new("users").append(Route::new("<id:u64>").get(get_user)));

// 自动收集路由中所有 handler 的文档元信息
let openapi = routes.to_openapi("My API", "1.0.0");
```

此方法会：
1. 递归遍历路由树，收集所有 handler 的文档元信息
2. 将 Silent 路径参数格式（`<id:u64>`）自动转换为 OpenAPI 格式（`{id}`）
3. 自动注册 `#[endpoint]` 声明的响应类型和请求体类型的 schema
4. 为路径参数自动生成 `parameters` 声明

### 方式二：使用 OpenApiDoc 构建器

适用于需要更多控制的场景：

```rust
use silent_openapi::OpenApiDoc;

let doc = OpenApiDoc::new("My API", "1.0.0")
    .description("基于 Silent 框架的 RESTful API")
    .add_server("https://api.example.com", Some("生产环境"))
    .add_server("http://localhost:8080", Some("开发环境"));
```

### 方式三：合并两种方式

先从路由自动生成，再追加自定义配置：

```rust
let openapi = routes.to_openapi("My API", "1.0.0");
let openapi = OpenApiDoc::from_openapi(openapi)
    .description("完整的 API 文档")
    .add_server("https://api.example.com", Some("生产环境"))
    .add_bearer_auth("bearerAuth", Some("JWT Bearer token"))
    .set_global_security("bearerAuth", &[])
    .into_openapi();
```

---

## 5. 挂载 Swagger UI

Silent OpenAPI 提供两种方式集成 Swagger UI 页面。

### 方式一：Handler 方式（推荐）

将 Swagger UI 作为独立路由挂载：

```rust
use silent_openapi::{SwaggerUiHandler, SwaggerUiOptions};

let options = SwaggerUiOptions {
    try_it_out_enabled: true,  // 启用 "Try it out" 交互功能
};
let swagger = SwaggerUiHandler::with_options("/docs", openapi, options)
    .expect("创建 Swagger UI");

let app = Route::new("")
    .append(swagger.into_route())  // 挂载 /docs 和 /docs/openapi.json
    .append(routes);

Server::new().bind("127.0.0.1:8080".parse().unwrap()).serve(app).await;
```

访问 `http://localhost:8080/docs` 即可查看文档页面。

### 方式二：Middleware 方式

作为中间件拦截匹配路径的请求：

```rust
use silent_openapi::SwaggerUiMiddleware;

let middleware = SwaggerUiMiddleware::new("/docs", openapi)?;

let app = Route::new("")
    .hook(middleware)
    .append(routes);
```

还可自定义 API 文档路径：

```rust
let middleware = SwaggerUiMiddleware::with_custom_api_doc_path(
    "/docs",
    "/api/spec.json",
    openapi,
)?;
```

### SwaggerUiOptions

| 字段 | 类型 | 默认值 | 说明 |
|------|------|--------|------|
| `try_it_out_enabled` | `bool` | `true` | 是否在 UI 中启用 "Try it out" 交互按钮 |

---

## 6. 提取器自动集成

`#[endpoint]` 宏能自动识别以下提取器类型，并在 OpenAPI 文档中生成对应的请求描述：

| 提取器 | 生成的 OpenAPI 描述 |
|--------|-------------------|
| `Json<T>` | `requestBody` + `application/json` + `$ref: T` |
| `Form<T>` | `requestBody` + `application/x-www-form-urlencoded` + `$ref: T` |
| `Query<T>` | `parameters` (query) + `$ref: T` |
| `Path<T>` | 由路由路径参数 `<name:type>` 自动生成 |

### 示例

```rust
#[derive(Serialize, Deserialize, ToSchema)]
struct SearchParams {
    keyword: String,
    page: Option<u32>,
    page_size: Option<u32>,
}

/// 搜索用户
///
/// 根据关键词搜索用户列表，支持分页
#[endpoint]
async fn search_users(Query(params): Query<SearchParams>) -> Result<Vec<User>> {
    // 实现搜索逻辑...
    Ok(vec![])
}
```

生成的 OpenAPI 文档中会自动包含：
- query parameter 引用 `SearchParams` schema
- `SearchParams` 的 schema 定义注册到 `components.schemas`
- summary = "搜索用户"，description = "根据关键词搜索用户列表，支持分页"（从文档注释提取）

### JSON 请求体示例

```rust
#[derive(Serialize, Deserialize, ToSchema)]
struct CreateUserRequest {
    name: String,
    email: String,
}

#[endpoint(summary = "创建用户")]
async fn create_user(Json(body): Json<CreateUserRequest>) -> Result<User> {
    Ok(User { id: 1, name: body.name, email: Some(body.email) })
}
```

生成的 OpenAPI 文档中会自动包含：
- `requestBody` 引用 `CreateUserRequest` schema，content-type 为 `application/json`
- `required: true`

---

## 7. 枚举类型文档化

枚举类型同样通过 `#[derive(ToSchema)]` 派生，`#[endpoint]` 会自动注册其完整 schema（包括所有变体和嵌套类型）。

### 简单枚举（字符串枚举）

```rust
#[derive(Serialize, Deserialize, ToSchema)]
enum UserStatus {
    Active,
    Inactive,
    Pending,
}
```

生成的 OpenAPI schema：
```json
{
  "UserStatus": {
    "type": "string",
    "enum": ["Active", "Inactive", "Pending"]
  }
}
```

### 带数据的枚举（tagged union）

```rust
#[derive(Serialize, Deserialize, ToSchema)]
struct OrderItem {
    product_id: u64,
    quantity: u32,
}

#[derive(Serialize, Deserialize, ToSchema)]
enum ApiResponse {
    Success { data: Vec<OrderItem> },
    Error { code: i32, message: String },
}
```

嵌套的 `OrderItem` schema 会通过 `ToSchema::schemas()` 递归注册到 `components.schemas`。

### 作为返回类型

```rust
#[endpoint(summary = "查询订单状态")]
async fn get_order_status(Path(id): Path<u64>) -> Result<ApiResponse> {
    Ok(ApiResponse::Success {
        data: vec![OrderItem { product_id: 1, quantity: 2 }],
    })
}
```

自动生成 `200` 响应，content-type 为 `application/json`，引用 `ApiResponse` schema。

### 作为请求体类型

```rust
#[derive(Serialize, Deserialize, ToSchema)]
enum CreateAction {
    Quick { name: String },
    Detailed { name: String, description: String, tags: Vec<String> },
}

#[endpoint(summary = "创建资源")]
async fn create_resource(Json(action): Json<CreateAction>) -> Result<Response> {
    Ok(Response::text("created"))
}
```

---

## 8. 安全定义与全局配置

### Bearer / JWT 认证

```rust
let openapi = OpenApiDoc::from_openapi(openapi)
    .add_bearer_auth("bearerAuth", Some("JWT Bearer token"))
    .set_global_security("bearerAuth", &[])
    .into_openapi();
```

生成的 OpenAPI 文档会包含：
```json
{
  "components": {
    "securitySchemes": {
      "bearerAuth": {
        "type": "http",
        "scheme": "bearer",
        "bearerFormat": "JWT"
      }
    }
  },
  "security": [
    { "bearerAuth": [] }
  ]
}
```

### 添加服务器信息

```rust
let doc = OpenApiDoc::from_openapi(openapi)
    .add_server("https://api.example.com", Some("生产环境"))
    .add_server("http://localhost:8080", Some("开发环境"));
```

### 导出 JSON

```rust
let doc = OpenApiDoc::from_openapi(openapi);

// 压缩 JSON
let json = doc.to_json()?;

// 格式化 JSON
let pretty_json = doc.to_pretty_json()?;

// serde_json::Value
let value = doc.to_json_value()?;
```

---

## 9. 文档说明的来源与优先级

`#[endpoint]` 宏按以下优先级确定 summary 和 description：

1. **属性参数**（最高优先级）：`#[endpoint(summary = "...", description = "...")]`
2. **文档注释**：从 `///` 注释提取
   - 首个非空行 → `summary`
   - 剩余非空行（用换行连接）→ `description`
3. **自动回退**（最低优先级）：
   - summary → `<METHOD> <path>`（如 `GET /users/{id}`）
   - description → `Handler for <METHOD> <path>`

### 文档注释示例

```rust
/// 获取用户信息
///
/// 根据用户 ID 查询完整的用户资料，包括基本信息和权限设置。
/// 需要 Bearer token 认证。
#[endpoint]
async fn get_user(Path(id): Path<u64>) -> Result<User> {
    // ...
}
```

等价于：
```rust
#[endpoint(summary = "获取用户信息", description = "根据用户 ID 查询完整的用户资料，包括基本信息和权限设置。\n需要 Bearer token 认证。")]
```

### operationId 与 tag 自动生成

- **operationId**：由 `<method>_<path>` 自动生成（路径中非字母数字字符替换为 `_`）
- **tag**：取路径首个非空段（如 `/users/{id}` → tag `users`）

---

## 10. 集成方式对比

| 特性 | Handler 方式 | Middleware 方式 |
|------|-------------|----------------|
| 集成方式 | `swagger.into_route()` 追加到路由树 | `.hook(middleware)` 挂载到路由 |
| 路径匹配 | 精确匹配 `/docs` 和 `/docs/*` | 拦截匹配路径，不匹配则透传 |
| 自定义 API 路径 | 通过构造参数 | `with_custom_api_doc_path()` |
| 适用场景 | 独立文档入口 | 需要与业务路由共存在同一路由节点 |

### 推荐选择

- **新项目**：使用 Handler 方式，路由结构更清晰
- **已有项目改造**：使用 Middleware 方式，无需调整路由结构

---

## 11. 完整示例

```rust
use serde::{Deserialize, Serialize};
use silent::extractor::Path;
use silent::prelude::*;
use silent_openapi::{
    endpoint, OpenApiDoc, RouteOpenApiExt,
    SwaggerUiHandler, SwaggerUiOptions, ToSchema,
};

// ===== 数据模型 =====

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

// ===== 处理器 =====

#[endpoint(summary = "获取问候", description = "返回欢迎消息")]
async fn get_hello(_req: Request) -> Result<String> {
    Ok("Hello, OpenAPI!".into())
}

#[endpoint(summary = "获取用户", description = "根据路径参数 id 返回用户信息")]
async fn get_user(Path(id): Path<u64>) -> Result<User> {
    Ok(User {
        id,
        name: format!("User {}", id),
    })
}

#[endpoint(summary = "受保护资源")]
async fn get_protected(req: Request) -> Result<Response> {
    let auth = req
        .headers()
        .get(http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok());
    match auth {
        None => Ok(Response::json(&ErrorResponse {
            code: "UNAUTHORIZED".into(),
            message: "missing Authorization".into(),
        }).with_status(StatusCode::UNAUTHORIZED)),
        Some(_) => Ok(Response::text("ok")),
    }
}

// ===== 启动 =====

#[tokio::main]
async fn main() -> Result<()> {
    logger::fmt().init();

    // 1. 构建业务路由
    let routes = Route::new("")
        .get(get_hello)
        .append(Route::new("users").append(Route::new("<id:u64>").get(get_user)))
        .append(Route::new("protected").get(get_protected));

    // 2. 从路由自动生成 OpenAPI，追加安全配置
    let openapi = routes.to_openapi("My API", "1.0.0");
    let openapi = OpenApiDoc::from_openapi(openapi)
        .add_bearer_auth("bearerAuth", Some("JWT"))
        .set_global_security("bearerAuth", &[])
        .into_openapi();

    // 3. 挂载 Swagger UI
    let swagger = SwaggerUiHandler::with_options(
        "/docs",
        openapi,
        SwaggerUiOptions { try_it_out_enabled: true },
    ).expect("创建 Swagger UI");

    let app = Route::new("")
        .append(swagger.into_route())
        .append(routes);

    // 4. 启动服务
    Server::new()
        .bind("127.0.0.1:8080".parse().unwrap())
        .serve(app)
        .await;
    Ok(())
}
```

启动后访问：
- Swagger UI：`http://localhost:8080/docs`
- OpenAPI JSON：`http://localhost:8080/docs/openapi.json`
- 业务接口：`http://localhost:8080/users/42`

---

## 12. 常见问题

### Swagger UI 显示 404

Swagger UI 的静态资源（CSS/JS）通过 CDN 加载，仅 `/docs`、`/docs/index.html` 和 `/docs/openapi.json` 由服务端提供。确保网络能访问 CDN。

### 接口没有出现在文档中

1. 确认处理器使用了 `#[endpoint]` 宏
2. 确认路由已添加到传给 `to_openapi()` 的路由树中
3. `to_openapi()` 只收集已挂载 handler 的路由节点

### 请求体没有自动生成

确保：
1. 使用了 `Json<T>`、`Form<T>` 或 `Query<T>` 提取器
2. 内部类型 `T` 派生了 `ToSchema`
3. 处理器使用了 `#[endpoint]` 宏（非普通函数）

### Schema 引用显示为空对象

确保调用了 `.apply_registered_schemas()`（`to_openapi()` 内部已自动调用）。如果使用 `OpenApiDoc::new()` 手动构建，需要显式调用：

```rust
let doc = OpenApiDoc::new("API", "1.0.0")
    .add_paths(paths)
    .apply_registered_schemas();  // 将注册的 schema 写入 components
```

### 生产环境建议

- 通过 `SwaggerUiOptions { try_it_out_enabled: false }` 关闭交互
- 或仅在非生产环境挂载 Swagger UI，生产环境只保留 `openapi.json` 导出
- 如需鉴权，在网关层或中间件层控制对 `/docs` 路径的访问
