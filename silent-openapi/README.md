# Silent OpenAPI

`silent-openapi` 为 [Silent Web Framework](https://github.com/silent-rs/silent) 提供基于 utoipa 5 的 OpenAPI 3.1 文档生成，以及 Swagger UI 和 ReDoc 集成。

它与 Silent 位于同一工作区，但作为独立 crate 发布和维护兼容关系。

## 特性

- 使用 `utoipa::OpenApi` 和 `ToSchema` 在编译期生成文档；
- 提供 Swagger UI 中间件和路由处理器两种挂载方式；
- 支持 ReDoc、路由文档收集和 OpenAPI JSON 输出；
- 支持 Bearer、API Key 和全局安全要求；
- 可关闭 Swagger UI 的 Try it out，或启用本地嵌入资源。

## 安装

```toml
[dependencies]
silent = "2.16"
silent-openapi = "2.16"
utoipa = "5"
serde = { version = "1", features = ["derive"] }
tokio = { version = "1", features = ["full"] }
```

需要 Swagger UI 本地资源时启用：

```toml
silent-openapi = { version = "2.16", features = ["swagger-ui-embedded"] }
```

## 快速开始

```rust
use serde::{Deserialize, Serialize};
use silent::prelude::*;
use silent_openapi::{SwaggerUiMiddleware, ToSchema};
use utoipa::OpenApi;

#[derive(Serialize, Deserialize, ToSchema)]
struct User {
    id: u64,
    name: String,
}

#[utoipa::path(
    get,
    path = "/users",
    responses((status = 200, description = "用户列表", body = [User]))
)]
async fn list_users(_req: Request) -> Result<Response> {
    Ok(Response::json(&vec![User {
        id: 1,
        name: "Alice".to_string(),
    }]))
}

#[derive(OpenApi)]
#[openapi(
    info(title = "用户 API", version = "1.0.0"),
    paths(list_users),
    components(schemas(User))
)]
struct ApiDoc;

#[tokio::main]
async fn main() {
    let swagger = SwaggerUiMiddleware::new("/docs", ApiDoc::openapi())
        .expect("create Swagger UI");
    let app = Route::new_root()
        .hook(swagger)
        .append(Route::new("users").get(list_users));

    Server::new()
        .bind("127.0.0.1:8080".parse().unwrap())
        .serve(app)
        .await;

}
```

启动后访问：

- Swagger UI：`http://127.0.0.1:8080/docs`
- OpenAPI JSON：`http://127.0.0.1:8080/docs/openapi.json`

## 使用处理器挂载

`SwaggerUiHandler` 已实现 `RouterAdapt`，可以直接追加到根路由：

```rust
use silent::prelude::*;
use silent_openapi::SwaggerUiHandler;

let swagger = SwaggerUiHandler::new("/docs", ApiDoc::openapi())?;
let app = Route::new_root()
    .append(swagger)
    .append(your_api_routes);
```

不需要额外创建 `.any()` 路由。

## 从路由生成文档

`RouteOpenApiExt` 可以根据 Silent 路由生成基础 OpenAPI 文档：

```rust
use silent::prelude::*;
use silent_openapi::{OpenApiDoc, RouteOpenApiExt, SwaggerUiHandler};

let routes = Route::new("api")
    .append(Route::new("users").get(list_users))
    .append(Route::new("users/<id:u64>").get(get_user));

let openapi = routes.to_openapi("User API", "1.0.0");
let openapi = OpenApiDoc::from_openapi(openapi)
    .add_bearer_auth("bearerAuth", Some("JWT Bearer token"))
    .set_global_security("bearerAuth", &[])
    .into_openapi();

let app = Route::new_root()
    .append(SwaggerUiHandler::new("/docs", openapi)?)
    .append(routes);
```

路由自动收集用于生成基础文档；需要精确的请求、响应和 schema 信息时，继续使用 `#[utoipa::path]` 和 `#[derive(ToSchema)]`。

## Swagger UI 配置

```rust
use silent_openapi::{SwaggerUiMiddleware, SwaggerUiOptions};

let options = SwaggerUiOptions {
    try_it_out_enabled: false,
};

let swagger = SwaggerUiMiddleware::with_options(
    "/docs",
    ApiDoc::openapi(),
    options,
)?;
```

自定义 OpenAPI JSON 地址：

```rust
let swagger = SwaggerUiMiddleware::with_custom_api_doc_path(
    "/docs",
    "/openapi.json",
    ApiDoc::openapi(),
)?;
```

## 安全建议

- 生产环境按需关闭 Try it out；
- 将文档入口放在受保护的路由或网关之后；
- 不要把认证、用户或权限逻辑放入 `silent-openapi`；本 crate 只描述 OpenAPI 安全方案；
- 为不同环境设置正确的 `servers`，避免文档指向错误地址；
- 为 OpenAPI JSON 配置合适的缓存和跨域策略。

## 示例

当前示例：

- `simple_example`：最小 Swagger UI 集成；
- `user_api`：完整的用户 CRUD 文档；
- `security_example`：Bearer 安全定义和路由处理器挂载。

从工作区根目录运行：

```bash
cargo run -p silent-openapi --example simple_example
cargo run -p silent-openapi --example user_api
cargo run -p silent-openapi --example security_example
```

## 版本兼容性

| silent-openapi | silent | utoipa | OpenAPI |
|---|---|---|---|
| 2.16.x | 2.16.x | 5.x | 3.1 |

2.x 范围内以工作区当前版本组合为准。升级 utoipa 主版本时需要重新核对生成结果和公开类型兼容性。

## 许可证

本项目采用 Apache-2.0 许可证，详见 [LICENSE](../LICENSE)。

## 相关链接

- [Silent Web Framework](https://github.com/silent-rs/silent)
- [utoipa](https://github.com/juhaku/utoipa)
- [OpenAPI 3.1 规范](https://spec.openapis.org/oas/v3.1.0)
