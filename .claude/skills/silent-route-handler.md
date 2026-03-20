# Silent 路由与 Handler 编写

当用户需要创建路由、编写请求处理器、使用提取器时，使用此 Skill。

## 路由基础

```rust
use silent::prelude::*;

// 创建根路由（应用入口，必须有）
let route = Route::new_root();

// 创建子路由
let route = Route::new("api/v1");

// 挂载 HTTP 方法处理器
let route = Route::new("users")
    .get(list_users)
    .post(create_user);

// 嵌套子路由
let route = Route::new_root()
    .append(Route::new("api/v1")
        .append(Route::new("users")
            .get(list_users)
            .post(create_user)
            .append(Route::new("<id:i64>")
                .get(get_user)
                .put(update_user)
                .delete(delete_user)
            )
        )
    );
```

## 路径参数语法

```rust
Route::new("<id>")           // 字符串参数
Route::new("<id:i64>")       // 整数参数（i64）
Route::new("<id:int>")       // 整数参数（同上）
Route::new("<path:**>")      // 通配符（匹配剩余所有路径段）
```

## Handler 编写模式

### 1. 最简单：闭包

```rust
Route::new("hello").get(|_req: Request| async { Ok("hello world") })
```

### 2. 函数 Handler

```rust
async fn hello(_req: Request) -> Result<&'static str> {
    Ok("hello world")
}

// 支持多种返回类型
async fn text_handler(_req: Request) -> Result<String> {
    Ok("text".to_string())
}

async fn json_handler(_req: Request) -> Result<Response> {
    Ok(Response::json(&serde_json::json!({"key": "value"})))
}
```

### 3. 使用提取器

```rust
use serde::Deserialize;

// Path 提取器 — 从路径参数中提取
async fn get_user(Path(id): Path<i64>) -> Result<String> {
    Ok(format!("user id: {id}"))
}
// 路由: Route::new("<id:i64>").get(get_user)

// Path 结构体提取
#[derive(Deserialize)]
struct UserPath {
    id: i64,
    name: String,
}
async fn get_user_by_name(Path(params): Path<UserPath>) -> Result<String> {
    Ok(format!("id={}, name={}", params.id, params.name))
}
// 路由: Route::new("<id:i64>/<name>").get(get_user_by_name)

// Query 提取器 — 从 URL 查询参数中提取
#[derive(Deserialize)]
struct Pagination {
    page: u32,
    size: u32,
}
async fn list(Query(p): Query<Pagination>) -> Result<String> {
    Ok(format!("page={}, size={}", p.page, p.size))
}
// 请求: GET /list?page=1&size=10

// Json 提取器 — 从请求体 JSON 中提取
#[derive(Deserialize)]
struct CreateUser {
    name: String,
    email: String,
}
async fn create_user(Json(input): Json<CreateUser>) -> Result<Response> {
    Ok(Response::json(&serde_json::json!({
        "name": input.name,
        "email": input.email,
    })))
}

// Form 提取器 — 从表单数据中提取
async fn submit_form(Form(data): Form<CreateUser>) -> Result<String> {
    Ok(format!("name={}", data.name))
}
```

### 4. 多提取器组合

```rust
// 元组组合多个提取器
async fn user_detail(
    (Path(id), Query(p)): (Path<i64>, Query<Pagination>),
) -> Result<String> {
    Ok(format!("id={id}, page={}", p.page))
}

// Request + 提取器组合
async fn handler_with_req(
    req: Request,
    Json(body): Json<CreateUser>,
) -> Result<Response> {
    let ua = req.headers().get("user-agent");
    Ok(Response::json(&serde_json::json!({
        "name": body.name,
    })))
}
```

### 5. 获取路径参数（非提取器方式）

```rust
async fn get_user(req: Request) -> Result<String> {
    let id: i64 = req.get_path_params("id")?;
    Ok(format!("user id: {id}"))
}
```

## Response 构造

```rust
// 文本响应
Response::text("hello")

// HTML 响应
Response::html("<h1>Hello</h1>")

// JSON 响应
Response::json(&serde_json::json!({"key": "value"}))

// 空响应
Response::empty()

// 重定向
Response::redirect("/new-path")?

// 自定义状态码
let mut res = Response::json(&data);
res.set_status(StatusCode::CREATED);

// 设置响应头
res.headers_mut().insert("X-Custom", "value".parse().unwrap());

// 字符串和 &str 自动转为 Response
async fn handler(_req: Request) -> Result<&'static str> {
    Ok("auto convert to Response")
}
```

## 路由中间件挂载

```rust
use silent::middlewares::{Logger, Cors, Timeout, RateLimiter, RequestId};
use std::time::Duration;

let route = Route::new("")
    .hook(Logger::new())                      // 请求日志
    .hook(Cors::new().origin("*"))            // CORS
    .hook(Timeout::new(Duration::from_secs(30)))  // 超时
    .hook(RateLimiter::per_second(100.0))     // 限流
    .hook(RequestId::new())                   // 请求追踪 ID
    .get(handler);
```

## Configs 配置注入

```rust
// 在路由上设置配置
let mut configs = Configs::default();
configs.insert(DatabasePool::new());

let mut route = Route::new("").get(handler);
route.set_configs(Some(configs));

// 在处理器中获取配置
async fn handler(req: Request) -> Result<Response> {
    let pool = req.get_config::<DatabasePool>()?;
    // 使用 pool...
    Ok(Response::empty())
}
```

## 静态文件服务

```rust
// 基础用法
let route = Route::new("static").with_static("./public");

// 带选项
let route = Route::new("static").with_static_options("./public", StaticOptions {
    // 配置项...
    ..Default::default()
});
```
