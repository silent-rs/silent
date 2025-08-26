## OpenAPI 使用指南（结合萃取器风格）

本文档介绍如何在 Silent 工程中集成 OpenAPI（接口文档）并与萃取器风格的路由配合使用。目标是用最少的标注获得可读、可交互（Swagger UI）的接口文档，并在生产环境具备可控的启停能力。

### 1. 依赖
- 工作空间已包含 `silent-openapi`。
- 业务 crate 常用依赖：
  - `serde = { version = "1", features = ["derive"] }`
  - `utoipa = { version = "5", features = ["derive"] }`（`silent-openapi` 已复导出常用 trait）

### 2. 定义数据模型（ToSchema）
```rust
use serde::{Serialize, Deserialize};
use silent_openapi::ToSchema;

#[derive(Serialize, Deserialize, ToSchema)]
struct User { id: u64, name: String }
```

### 3. 定义处理器与路由（配合萃取器）
推荐使用 `#[endpoint]` 宏为处理器注册文档说明与响应类型：
```rust
use silent::prelude::*;
use silent_openapi::{endpoint, ToSchema};

#[endpoint(summary = "获取用户", description = "根据路径参数 id 返回用户信息")]
async fn get_user(Path(id): Path<u64>) -> Result<User> {
    Ok(User { id, name: format!("User {}", id) })
}

#[endpoint(summary = "健康检查", description = "返回 ok 字符串")]
async fn health(_req: Request) -> Result<String> { Ok("ok".into()) }

let routes = Route::new("")
    .append(Route::new("users/<id:u64>").get(get_user))
    .append(Route::new("health").get(health));
```

返回类型与 OpenAPI 响应映射：
- `Result<String>` / `Result<&str>` → `text/plain` 响应
- `Result<T>`（非 `Response` 且带 `ToSchema`）→ `application/json`，并自动生成/合并 `components.schemas` 中的 `T`
- `Result<Response>` → 保留原样（默认 200 响应，无 content）

### 4. 生成 OpenAPI 文档
`silent-openapi` 提供 `RouteOpenApiExt::to_openapi`：
```rust
use silent_openapi::{OpenApiDoc, RouteOpenApiExt};

let openapi = routes.to_openapi("My API", "1.0.0");
// 叠加安全策略（可选）
let openapi = OpenApiDoc::from_openapi(openapi)
    .add_bearer_auth("bearerAuth", Some("JWT Bearer token"))
    .set_global_security("bearerAuth", &[])
    .into_openapi();
```

路径与参数规则：
- Silent 路由 `<id:u64>` / `<name>` 会被自动转换为 OpenAPI `{id}` / `{name}`
- 基于萃取器的请求体（Json/Form）参数暂不自动生成 requestBody，请结合 utoipa 的 `#[utoipa::path(...)]` 按需补充

### 5. 挂载 Swagger UI（文档页面）
使用处理器方式更易于集成：
```rust
use silent_openapi::{SwaggerUiHandler, SwaggerUiOptions};

let swagger = SwaggerUiHandler::with_options("/docs", openapi, SwaggerUiOptions { try_it_out_enabled: true })
    .expect("create swagger ui");
let app = Route::new("")
    .append(swagger.into_route())  // /docs 与 /docs/openapi.json
    .append(routes);

Server::new().bind("127.0.0.1:8080".parse().unwrap()).serve(app).await;
```

生产建议：
- 通过 `SwaggerUiOptions { try_it_out_enabled: false }` 关闭交互；或仅在非生产环境挂载 UI
- 如需鉴权与 CORS 控制，可在网关层或中间件层实现

### 6. 文档说明的来源
- `#[endpoint(summary = "...", description = "...")]` 优先使用属性参数
- 未显式指定时，从处理函数的 `///` 文档注释首行/剩余行提取 `summary/description`
- 若仍为空，则按规则回退：
  - `summary`: `<METHOD> <path>`
  - `description`: `Handler for <METHOD> <path>`

### 7. 与萃取器的配合
- 路由注册保持零泛型：`.get(get_user)`；
- 支持三种处理器形态：
  - `fn(Request) -> _`
  - `fn(Args) -> _`（如 `Path<T> / Query<T> / Json<T>`）
  - `fn(Request, Args) -> _`
- 常见参数（路径 `<id:u64>`）已自动体现在 OpenAPI 的 `parameters` 中；其余复杂输入（如 Query 结构体、JSON 请求体）推荐用 utoipa 标注补充

### 8. 常见问题
- UI 404：本版本非主页资源使用 CDN，除 `/docs` 与 `/docs/index.html` 外的静态资源默认 404
- 版本兼容：该 UI 版本已支持 OpenAPI 3.1；若外部工具要求 3.0 可按需降级 utoipa

### 9. 最佳实践清单
- 模型统一 `#[derive(Serialize, Deserialize, ToSchema)]`
- 处理器统一加 `#[endpoint]`，便于自动生成响应映射与说明
- `Route::to_openapi(...)` + `OpenApiDoc::from_openapi(...)` 叠加安全、服务器配置
- 生产环境关闭 Try it out 或仅保留 openapi.json 导出
