## 萃取器（Extractors）使用指南

本文介绍 Silent 框架的请求萃取器体系，包括内置萃取器、注册方式、返回与错误处理，以及如何编写自定义萃取器。

### 核心概念
- **FromRequest**: 所有萃取器实现的统一接口，用于从 `Request` 中提取类型化参数。
- **IntoRouteHandler<Args>**: 统一适配处理函数到路由的接口。支持以下三种处理器形态：
  - 仅 `Request`：`fn(Request) -> Future<Result<T>>`
  - 仅萃取器参数：`fn(Args) -> Future<Result<T>>`
  - `Request + 萃取器参数`：`fn(Request, Args) -> Future<Result<T>>`

### 注册方式（零泛型）
使用 `Route::get/post/...` 直接注册，无需显式泛型或适配器：

```rust
use silent::prelude::*;

#[derive(serde::Deserialize)]
struct Page { page: u32, size: u32 }

async fn list(Query(p): Query<Page>) -> Result<String> {
    Ok(format!("page={}, size={}", p.page, p.size))
}

async fn detail((Path(id), Query(p)): (Path<i64>, Query<Page>)) -> Result<String> {
    Ok(format!("id={id}, page={}, size={}", p.page, p.size))
}

async fn detail_with_req(req: Request, Path(id): Path<i64>) -> Result<String> {
    Ok(format!("{} -> id={id}", req.uri()))
}

fn main() {
    let route = Route::new("api")
        .append(Route::new("list").get(list))
        .append(Route::new("detail/<id:i64>").get(detail))
        .append(Route::new("detail2/<id:int>").get(detail_with_req));
    Server::new().run(route);
}
```

仍可显式使用适配器（可选）：

```rust
use silent::prelude::*;

async fn create(Json(input): Json<MyCreate>) -> Result<String> { /* ... */ }

let route = Route::new("api")
    .append(Route::new("create").post(handler_from_extractor(create)));
```

### 内置萃取器一览
- **Path<T>**：从路径参数解析到 `T`
  - 支持单值（仅一个路径参数）与结构体（多个路径参数按字段名匹配）
  - 路由写法示例：`"user/<id:int>"`、`"user/<id:i64>/<name>"`、`"<path:**>"`

- **Query<T>**：从 URL 查询参数解析到 `T`

- **Json<T>**：从 `application/json` 解析到 `T`
  - 内部带缓存，重复解析同一请求不会重复读取 body

- **Form<T>**：从表单解析到 `T`
  - 支持 `application/x-www-form-urlencoded` 与（启用 multipart 功能时）`multipart/form-data`
  - 需 `T: Deserialize + Serialize`

- **TypedHeader<H>**：类型化请求头（等价 axum 的 TypedHeader）

- **Method / Uri / Version / RemoteAddr**：请求方法、URI、HTTP 版本、远端地址

- **Extension<T>**：从 `Request.extensions()` 提取扩展（需 `T: Clone` 且已注入）

- **Configs<T>**：从 `Request.configs()` 提取全局配置（需 `T: Clone` 且已注入）

- **Option<E>**：当 `E: FromRequest` 失败时返回 `None`

- **Result<E, Response>**：当 `E: FromRequest` 失败时返回 `Err(Response)`

- **元组 (A, B, C, D)**：组合萃取，内置支持 1~4 个元素的元组

### 示例代码片段

```rust
use silent::prelude::*;
use silent::headers::UserAgent;
use serde::Deserialize;

#[derive(Deserialize)]
struct Page { page: u32, size: u32 }

async fn ex_path(Path(id): Path<i64>) -> Result<String> { Ok(format!("id={id}")) }
async fn ex_query(Query(p): Query<Page>) -> Result<String> { Ok(format!("{}-{}", p.page, p.size)) }
async fn ex_header(TypedHeader(ua): TypedHeader<UserAgent>) -> Result<String> { Ok(ua.as_str().into()) }

async fn ex_tuple((Path(id), Query(p)): (Path<i64>, Query<Page>)) -> Result<String> {
    Ok(format!("{id}:{}-{}", p.page, p.size))
}

let route = Route::new("api")
    .append(Route::new("p/<id:int>").get(ex_path))
    .append(Route::new("q").get(ex_query))
    .append(Route::new("h").get(ex_header))
    .append(Route::new("t/<id:int>").get(ex_tuple));
```

#### 多个 Query 结构体同时萃取

```rust
use silent::prelude::*;
use serde::Deserialize;

#[derive(Deserialize)]
struct Page { page: u32, size: u32 }

#[derive(Deserialize)]
struct Search { keyword: Option<String> }

async fn ex_multi_query((Query(s), Query(p)): (Query<Search>, Query<Page>)) -> Result<String> {
    Ok(format!(
        "keyword={:?}, page={}, size={}",
        s.keyword, p.page, p.size
    ))
}

let route = Route::new("api")
    .append(Route::new("multi_query").get(ex_multi_query));
```

### 错误与返回
- `FromRequest::Rejection` 需实现 `Into<Response>`，框架会自动将错误转换为响应返回。
- 对于组合萃取（元组）、`Option`、`Result`：
  - 元组：任一元素提取失败会立即返回 `Response`
  - `Option<E>`：失败时为 `None`
  - `Result<E, Response>`：失败时为 `Err(Response)`

### 自定义萃取器

```rust
use silent::prelude::*;
use async_trait::async_trait;

pub struct UserAgentString(pub String);

#[async_trait]
impl FromRequest for UserAgentString {
    type Rejection = SilentError;
    async fn from_request(req: &mut Request) -> Result<Self, Self::Rejection> {
        let ua = req
            .headers()
            .get("user-agent")
            .and_then(|h| h.to_str().ok())
            .ok_or(SilentError::ParamsNotFound)?;
        Ok(UserAgentString(ua.to_string()))
    }
}

async fn handler(UserAgentString(ua): UserAgentString) -> Result<String> {
    Ok(ua)
}

let route = Route::new("api").append(Route::new("ua").get(handler));
```

### 兼容性说明
- 处理函数保留对 `Request` 的支持，可与萃取器参数同时使用：`fn(Request, Args) -> _`。
- 也可继续使用传统 `fn(Request) -> _` 的处理函数注册。

### 注意事项
- `Json<T>` 和 `Form<T>` 读取请求体；框架对 JSON 和表单做了缓存/复用，避免重复解析。
- `Form<T>` 需要 `T: Deserialize + Serialize`。
- `TypedHeader<H>` 依赖 `headers` crate 的头部类型。

### 调试建议
- 局部调试可使用 `RequestExt::extract::<T>()` 手动提取并验证类型是否能正确解析。
- 遇到 `ParamsNotFound` / `ContentTypeError` 等错误信息时，检查路由路径参数、请求头 `Content-Type` 与实际请求体格式是否一致。

MVP: FromRequest 萃取器系统

参考: [RFC Issue #113](https://github.com/silent-rs/silent/issues/113)

目标
- 引入统一的 `FromRequest` 萃取器抽象，降低样板、提升类型安全
- 内置 `Path<T> / Query<T> / Json<T> / Form<T>` 四类常用萃取器
- 保持与现有 `Request` API 共存，兼容中间件/路由

核心 API
```rust
#[async_trait::async_trait]
pub trait FromRequest: Sized {
    type Rejection: Into<silent::Response> + Send + 'static;
    async fn from_request(req: &mut silent::Request) -> Result<Self, Self::Rejection>;
}

pub struct Path<T>(pub T);
pub struct Query<T>(pub T);
pub struct Json<T>(pub T);
pub struct Form<T>(pub T);

#[async_trait::async_trait]
pub trait RequestExt {
    async fn extract<T>(&mut self) -> Result<T, T::Rejection>
    where
        T: FromRequest + Send + 'static;
}
```

适配器与路由扩展
- `Route::{get, post, put, delete, patch, options}`：统一注册接口，接受 `IntoRouteHandler<Args>`；可直接注册以下三种形态的处理函数：
  - `fn(Request) -> Future<Result<T>>`
  - `fn(Args) -> Future<Result<T>>`
  - `fn(Request, Args) -> Future<Result<T>>`
- `handler_from_extractor(f)`：可选的显式适配器，将“接受萃取器参数的处理函数”适配为“接受 Request 的处理函数”

使用示例
```rust
use serde::Deserialize;
use silent::prelude::*;

#[derive(Deserialize)]
struct Page { page: u32, size: u32 }

async fn user_detail((Path(id), Query(p)): (Path<i64>, Query<Page>)) -> Result<String> {
    Ok(format!("id={id}, page={}, size={}", p.page, p.size))
}

fn main() {
    logger::fmt().with_max_level(Level::INFO).init();
    let route = Route::new("api/users/<id:int>").get(user_detail);
    Server::new().run(route);
}
```

行为细节
- Path：
  - 仅一个参数时，按单值解析（支持数值、字符串、枚举等）
  - 多参数时，按字段名聚合解析为结构体
- Query：基于 `Request::params_parse` 解析
- Json：基于 `Request::json_parse`，复用 `json_data` 缓存
- Form：基于 `Request::form_parse`，multipart 复用 `form_data` 缓存，urlencoded 复用 `json_data` 缓存

增强萃取器（axum 风格）
- Option<T>：包装萃取器，失败返回 `None`
- Result<T, Response>：包装萃取器，失败返回 `Err(Response)`
- Extension<T>：从 `Request::extensions()` 克隆提取 `T`
- TypedHeader<H>：从请求头以类型化头提取 `H: headers::Header`
- Method / Uri / Version / RemoteAddr：轻量信息提取
- Configs<T>：从全局 `Configs` 提取并克隆 `T`（等价 axum 的 State<T>；在 `prelude` 以别名 `Cfg` 导出）

路由注册（统一接口）
- 直接使用 `get/post/...` 注册；必要时可以显式使用 `handler_from_extractor(...)` 进行适配。

兼容性
- 与现有 `Request` API 共存，无需强制迁移
- 与路由、DFS 匹配、中间件保持兼容

验收
- `cargo clippy -p silent --all-targets --all-features --tests --benches -- -D warnings` 全绿
- 示例可运行
