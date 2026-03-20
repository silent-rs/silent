# Silent 项目脚手架

当用户要求创建一个新的 Silent Web 应用项目时，使用此 Skill。

## 项目结构

```
project-name/
├── Cargo.toml
├── src/
│   ├── main.rs        # 入口：Server 启动 + 路由组装
│   └── routes/        # 路由模块（按业务拆分）
│       └── mod.rs
```

## Cargo.toml 模板

```toml
[package]
name = "project-name"
version = "0.1.0"
edition = "2024"

[dependencies]
silent = "2"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
tracing = "0.1"
```

按需添加 features：
- 需要 WebSocket：`silent = { version = "2", features = ["upgrade"] }`
- 需要静态文件：`silent = { version = "2", features = ["static"] }`
- 需要所有功能：`silent = { version = "2", features = ["full"] }`
- 需要 gRPC：`silent = { version = "2", features = ["grpc"] }`
- 需要 SSE：`silent = { version = "2", features = ["sse"] }`
- 需要模板：`silent = { version = "2", features = ["template"] }`
- 需要会话：`silent = { version = "2", features = ["session"] }`

## main.rs 模板

```rust
use silent::prelude::*;

mod routes;

fn main() {
    logger::fmt().with_max_level(Level::INFO).init();

    let route = routes::create_routes();

    Server::new()
        .bind("0.0.0.0:8080".parse().unwrap())
        .run(route);
}
```

## routes/mod.rs 模板

```rust
use silent::prelude::*;

pub fn create_routes() -> Route {
    Route::new_root()
        .append(Route::new("api/v1")
            .append(Route::new("hello").get(hello))
        )
}

async fn hello(_req: Request) -> Result<&'static str> {
    Ok("Hello, Silent!")
}
```

## 关键约定

- ID 生成使用 `scru128` 库，不使用 uuid
- 时间字段使用 `chrono::Local::now().naive_local()`
- 日志使用 `tracing`，通过 `RUST_LOG` 环境变量过滤
- 禁止使用 unsafe 代码
- `Route::new_root()` 创建根路由，`Route::new("path")` 创建子路由
- 处理器返回类型为 `Result<T>`，T 可以是 `&str`、`String`、`Response` 或任何实现 `Into<Response>` 的类型
