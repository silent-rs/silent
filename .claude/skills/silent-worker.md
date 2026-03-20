# Silent Cloudflare Worker 开发

当用户需要使用 Silent 框架开发 Cloudflare Worker 应用时，使用此 Skill。

## 项目结构

```
my-worker/
├── Cargo.toml
├── wrangler.toml
└── src/
    ├── lib.rs      # Worker 入口
    └── route.rs    # 路由定义
```

## Cargo.toml

```toml
[package]
name = "my-worker"
version = "0.1.0"
edition = "2024"

[lib]
crate-type = ["cdylib"]

[dependencies]
silent = { version = "2.15", features = ["worker"] }
worker = "0.7"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
console_error_panic_hook = "0.1"
http-body-util = "0.1"
```

## wrangler.toml

```toml
name = "my-worker"
main = "build/worker/shim.mjs"
compatibility_date = "2025-09-24"

[build]
command = "cargo install -q worker-build && worker-build --release"

# KV 绑定（可选）
# 创建: wrangler kv namespace create MY_KV
[[kv_namespaces]]
binding = "MY_KV"
id = "your-kv-namespace-id"

# D1 绑定（可选）
# 创建: wrangler d1 create my-database
[[d1_databases]]
binding = "MY_DB"
database_name = "my-database"
database_id = "your-d1-database-id"

# R2 绑定（可选）
# 创建: wrangler r2 bucket create my-bucket
[[r2_buckets]]
binding = "MY_BUCKET"
bucket_name = "my-bucket"
```

## Worker 入口 (lib.rs)

```rust
#![cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]

#[cfg(target_arch = "wasm32")]
mod route;

#[cfg(target_arch = "wasm32")]
use worker::{Context, Env, Request, Response, Result};

#[cfg(target_arch = "wasm32")]
use crate::route::get_route;
#[cfg(target_arch = "wasm32")]
use silent::Configs;

#[cfg(target_arch = "wasm32")]
#[worker::event(fetch)]
pub async fn main(req: Request, env: Env, ctx: Context) -> Result<Response> {
    console_error_panic_hook::set_once();

    // 将 Env 和 Context 注入到 Configs
    // 处理器通过 req.get_config::<Env>() 和 req.get_config::<Context>() 获取
    let mut cfg = Configs::default();
    cfg.insert(env);
    cfg.insert(ctx);

    let wr = get_route().with_configs(cfg);
    Ok(wr.call(req).await)
}
```

## 路由定义 (route.rs)

```rust
use silent::{Request, Response, prelude::{Route, WorkRoute}};
use worker::Env;

pub fn get_route() -> WorkRoute {
    let route = Route::new_root()
        .append(Route::new("hello").get(hello))
        .append(Route::new("kv").append(
            Route::new("<key>").get(kv_get).put(kv_put).delete(kv_delete),
        ));

    WorkRoute::new(route)
}

async fn hello(_req: Request) -> silent::Result<&'static str> {
    Ok("hello from Worker")
}
```

## 访问 Worker 绑定

### KV 操作

```rust
async fn kv_get(req: Request) -> silent::Result<Response> {
    let env = req.get_config::<Env>()?;
    let key: String = req.get_path_params("key")?;
    let kv = env.kv("MY_KV").map_err(worker_err)?;

    match kv.get(&key).text().await {
        Ok(Some(value)) => Ok(Response::json(&serde_json::json!({"key": key, "value": value}))),
        Ok(None) => Err(silent::SilentError::business_error(
            silent::StatusCode::NOT_FOUND,
            format!("key '{key}' not found"),
        )),
        Err(e) => Err(worker_err(e)),
    }
}

async fn kv_put(mut req: Request) -> silent::Result<Response> {
    let env = req.get_config::<Env>()?.clone();
    let key: String = req.get_path_params("key")?;
    let value = read_body_text(&mut req).await?;
    let kv = env.kv("MY_KV").map_err(worker_err)?;
    kv.put(&key, &value).map_err(worker_err)?.execute().await.map_err(worker_err)?;
    Ok(Response::json(&serde_json::json!({"status": "ok"})))
}
```

### D1 数据库操作

```rust
async fn d1_query(req: Request) -> silent::Result<Response> {
    let env = req.get_config::<Env>()?;
    let d1 = env.d1("MY_DB").map_err(worker_err)?;
    let stmt = d1.prepare("SELECT id, name FROM users LIMIT 100");
    let result = stmt.all().await.map_err(worker_err)?;
    let rows: Vec<serde_json::Value> = result.results().map_err(worker_err)?;
    Ok(Response::json(&serde_json::json!({"users": rows})))
}
```

### R2 对象存储操作

```rust
async fn r2_get(req: Request) -> silent::Result<Response> {
    let env = req.get_config::<Env>()?;
    let key: String = req.get_path_params("key")?;
    let bucket = env.bucket("MY_BUCKET").map_err(worker_err)?;

    match bucket.get(&key).execute().await {
        Ok(Some(obj)) => {
            let body = obj.body().ok_or_else(|| worker_err("no body"))?
                .bytes().await.map_err(worker_err)?;
            let mut resp = Response::empty();
            resp.set_body(silent::prelude::ResBody::from(body));
            Ok(resp)
        }
        Ok(None) => Err(silent::SilentError::business_error(
            silent::StatusCode::NOT_FOUND, format!("'{key}' not found"),
        )),
        Err(e) => Err(worker_err(e)),
    }
}
```

## 辅助函数

```rust
use http_body_util::BodyExt;

/// Worker 错误转 SilentError
fn worker_err(msg: impl std::fmt::Display) -> silent::SilentError {
    silent::SilentError::business_error(
        silent::StatusCode::INTERNAL_SERVER_ERROR,
        msg.to_string(),
    )
}

/// 读取请求体为字节
async fn read_body_bytes(req: &mut Request) -> silent::Result<Vec<u8>> {
    let body = req.take_body();
    let collected = body.collect().await.map_err(|e| {
        silent::SilentError::business_error(
            silent::StatusCode::BAD_REQUEST,
            format!("read body error: {e}"),
        )
    })?;
    Ok(collected.to_bytes().to_vec())
}

/// 读取请求体为文本
async fn read_body_text(req: &mut Request) -> silent::Result<String> {
    let bytes = read_body_bytes(req).await?;
    String::from_utf8(bytes).map_err(|e| {
        silent::SilentError::business_error(
            silent::StatusCode::BAD_REQUEST,
            format!("invalid UTF-8: {e}"),
        )
    })
}
```

## 本地开发与部署

```bash
# 安装 wasm 目标
rustup target add wasm32-unknown-unknown

# 本地预览
cd my-worker && wrangler dev

# 部署
wrangler login
wrangler deploy
```

## 关键注意事项

- Worker 环境下 `Configs` 是只读的，跨请求不保持状态
- 需要持久化请使用 KV / D1 / R2 / Durable Objects
- JSON 请求体可直接使用 `req.json_parse::<T>().await`
- 获取路径参数使用 `req.get_path_params("key")?`
- 获取 Env 时如需可变操作需要 `.clone()`：`req.get_config::<Env>()?.clone()`
