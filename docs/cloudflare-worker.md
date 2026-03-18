Cloudflare Worker 集成与使用指南（Silent 路由）

参考官方入门指南：https://developers.cloudflare.com/workers/get-started/guide/

环境准备
- Rust + Wasm 目标：`rustup target add wasm32-unknown-unknown`
- Wrangler（推荐全局安装）：
  - npm: `npm i -g wrangler`
  - 或 Homebrew: `brew install cloudflare/wrangler/wrangler`
- 可选：`worker-build`（wrangler 构建时会调用）：`cargo install -q worker-build`

示例位置
- 目录：`examples/cloudflare-worker`
- 入口：`examples/cloudflare-worker/src/lib.rs`
- 路由：`examples/cloudflare-worker/src/route.rs`
- 配置：`examples/cloudflare-worker/wrangler.toml`

示例路由一览
| 路径 | 方法 | 功能 | 使用的绑定 |
|------|------|------|-----------|
| `/hello` | GET | 基础文本响应 | 无 |
| `/kv/<key>` | GET | 读取 KV 值 | KV (`MY_KV`) |
| `/kv/<key>` | PUT | 写入 KV 值（请求体为值） | KV (`MY_KV`) |
| `/kv/<key>` | DELETE | 删除 KV 键 | KV (`MY_KV`) |
| `/d1/users` | GET | 查询用户列表（最多100条） | D1 (`MY_DB`) |
| `/d1/users` | POST | 创建用户（JSON: `{"name":"..","email":".."}`) | D1 (`MY_DB`) |
| `/r2/<key>` | GET | 读取 R2 对象 | R2 (`MY_BUCKET`) |
| `/r2/<key>` | PUT | 上传对象到 R2（请求体为文件内容） | R2 (`MY_BUCKET`) |
| `/r2/<key>` | DELETE | 删除 R2 对象 | R2 (`MY_BUCKET`) |

路由与适配
- 使用 `WorkRoute` 适配 Cloudflare Worker 的 `Request/Response` 与 Silent 的 `Request/Response`。
- 适配中使用 `Response::status()` 与 `Response::take_body()` 完成状态与字节聚合。
- 响应体通过 `http-body-util::BodyExt::collect` 聚合，兼容 Once/Chunks/Stream/Incoming/Boxed。
- 错误响应保留原始状态码（如 404、400），不再统一返回 500。

WorkRoute 增强功能

`with_configs()` 方法
- 用于将 Cloudflare Worker 的绑定（Env 或其子资源）注入到路由中
- 处理器通过 `req.get_config::<T>()` 获取注入的配置
- 推荐直接注入 `Env`，在处理器中按需获取 KV/D1/R2 等绑定

```rust
use silent::prelude::*;
use worker::Env;

let mut cfg = Configs::default();
cfg.insert(env);  // 注入整个 Env
let wr = WorkRoute::new(route).with_configs(cfg);
```

处理器中获取绑定：
```rust
async fn my_handler(req: Request) -> silent::Result<Response> {
    let env = req.get_config::<Env>()?;
    let kv = env.kv(“MY_KV”).map_err(worker_err)?;
    let d1 = env.d1(“MY_DB”).map_err(worker_err)?;
    let bucket = env.bucket(“MY_BUCKET”).map_err(worker_err)?;
    // ...
}
```

Context 注入
- 将 `worker::Context` 与 `Env` 一样注入到 `Configs` 中
- 处理器通过 `req.get_config::<worker::Context>()` 获取
- 适用于需要调度后台任务（`ctx.wait_until(fut)`）的场景

```rust
let mut cfg = Configs::default();
cfg.insert(env);
cfg.insert(ctx);  // Context 也注入 Configs
let wr = WorkRoute::new(route).with_configs(cfg);

// 处理器中使用 Context
async fn my_handler(req: Request) -> silent::Result<Response> {
    let ctx = req.get_config::<worker::Context>()?;
    ctx.wait_until(async { /* 后台任务 */ });
    Ok(Response::empty())
}
```

只读 Configs（重要）
- 由于 Wasm/Workers 的执行模型，实例的跨请求复用不可保证，且可能冷启动。处理器内对 `Configs` 的修改不会在后续请求中保持。
- 将 `Configs` 视为只读配置的载体，仅在初始化阶段注入不可变参数（常量、开关、外部服务句柄等）。
- 如需跨请求可变状态，请使用 Cloudflare 的持久化能力：KV、Durable Objects、D1、R2、Queues 等。

Env 与 Context 的使用
- `Env`：用于获取绑定（KV/DO/D1/R2/Queues 等）。建议将只读句柄注入到 `Configs`，供路由/处理器读取。
- `Context`：与 `Env` 一样通过 `Configs` 注入，处理器通过 `req.get_config::<Context>()` 获取。用于调度后台任务（`ctx.wait_until(fut)`），任务可在响应返回后继续执行。

示例：注入 Env + Context 并访问 KV 绑定
```rust
use worker::{Context, Env, Request, Response, Result};
use silent::prelude::*;

#[worker::event(fetch)]
pub async fn main(req: Request, env: Env, ctx: Context) -> Result<Response> {
    console_error_panic_hook::set_once();

    // 将 Env 和 Context 注入到 Configs
    let mut cfg = Configs::default();
    cfg.insert(env);
    cfg.insert(ctx);

    let wr = WorkRoute::new(get_route()).with_configs(cfg);

    Ok(wr.call(req).await)
}

fn get_route() -> Route {
    Route::new_root()
        .append(Route::new("hello").get(hello))
        .append(Route::new("kv").append(
            Route::new("<key>").get(kv_get).put(kv_put),
        ))
}

async fn hello(_req: silent::Request) -> silent::Result<&'static str> {
    Ok("hello from Worker")
}

/// KV 读取示例
async fn kv_get(req: silent::Request) -> silent::Result<silent::Response> {
    let env = req.get_config::<Env>()?;
    let key: String = req.get_path_params("key")?;
    let kv = env.kv("MY_KV").map_err(|e| {
        silent::SilentError::business_error(
            silent::StatusCode::INTERNAL_SERVER_ERROR, e.to_string(),
        )
    })?;
    match kv.get(&key).text().await {
        Ok(Some(v)) => Ok(silent::Response::json(&serde_json::json!({"key": key, "value": v}))),
        Ok(None) => Err(silent::SilentError::business_error(
            silent::StatusCode::NOT_FOUND, format!("key '{key}' not found"),
        )),
        Err(e) => Err(silent::SilentError::business_error(
            silent::StatusCode::INTERNAL_SERVER_ERROR, e.to_string(),
        )),
    }
}

/// KV 写入示例
async fn kv_put(mut req: silent::Request) -> silent::Result<silent::Response> {
    let env = req.get_config::<Env>()?.clone();
    let key: String = req.get_path_params("key")?;
    let value = read_body_text(&mut req).await?;
    let kv = env.kv("MY_KV").map_err(|e| {
        silent::SilentError::business_error(
            silent::StatusCode::INTERNAL_SERVER_ERROR, e.to_string(),
        )
    })?;
    kv.put(&key, &value).map_err(|e| {
        silent::SilentError::business_error(
            silent::StatusCode::INTERNAL_SERVER_ERROR, e.to_string(),
        )
    })?.execute().await.map_err(|e| {
        silent::SilentError::business_error(
            silent::StatusCode::INTERNAL_SERVER_ERROR, e.to_string(),
        )
    })?;
    Ok(silent::Response::json(&serde_json::json!({"status": "ok", "key": key})))
}
```

本地预览（wrangler dev）
```bash
cd examples/cloudflare-worker
wrangler dev
```
默认监听 `http://127.0.0.1:8787`，测试示例：
```bash
# 基础路由
curl http://127.0.0.1:8787/hello

# KV 操作
curl -X PUT http://127.0.0.1:8787/kv/mykey -d "hello world"
curl http://127.0.0.1:8787/kv/mykey
curl -X DELETE http://127.0.0.1:8787/kv/mykey

# D1 操作（需先建表，见 wrangler.toml 注释）
curl http://127.0.0.1:8787/d1/users
curl -X POST http://127.0.0.1:8787/d1/users \
  -H "Content-Type: application/json" \
  -d '{"name":"Alice","email":"alice@example.com"}'

# R2 操作
curl -X PUT http://127.0.0.1:8787/r2/test.txt -d "file content"
curl http://127.0.0.1:8787/r2/test.txt
curl -X DELETE http://127.0.0.1:8787/r2/test.txt
```

构建与部署
- 登录账号：`wrangler login`
- 构建 + 部署：
```bash
cd examples/cloudflare-worker
wrangler deploy
```
部署完成后，wrangler 会输出可访问的生产地址。

wrangler.toml 配置

最小配置：
```toml
name = "example-cloudflare-worker"
main = "build/index.js"
compatibility_date = "2025-09-24"

[build]
command = "cargo install -q worker-build && worker-build --release"
```

绑定配置（KV/D1/R2）：
```toml
# KV 命名空间绑定
# 创建命令: wrangler kv namespace create MY_KV
[[kv_namespaces]]
binding = "MY_KV"
id = "your-kv-namespace-id"
preview_id = "your-kv-preview-id"

# D1 数据库绑定
# 创建命令: wrangler d1 create my-database
# 建表: wrangler d1 execute MY_DB --local --command "CREATE TABLE IF NOT EXISTS users (id INTEGER PRIMARY KEY AUTOINCREMENT, name TEXT NOT NULL, email TEXT NOT NULL)"
[[d1_databases]]
binding = "MY_DB"
database_name = "my-database"
database_id = "your-d1-database-id"

# R2 存储桶绑定
# 创建命令: wrangler r2 bucket create my-bucket
[[r2_buckets]]
binding = "MY_BUCKET"
bucket_name = "my-bucket"
```

wrangler 将使用 `worker-build` 将 Rust 工程编译为 Wasm，并生成可运行的 Worker 入口。

环境变量与机密
- 普通变量：在 `wrangler.toml` 的 `[vars]` 中定义
- 机密：`wrangler secret put MY_SECRET`
- 处理器中可通过 `Env` 获取，或在 `with_configs` 时注入只读配置（推荐仅注入只读句柄）。

常见问题
- Wasm/Workers 下请求生命周期短且实例不可预测：不要依赖进程内“全局可变状态”。
- 需要持久化：使用 Cloudflare KV / Durable Objects / D1 / R2 等。
- 构建失败缺少目标：确保 `wasm32-unknown-unknown` 已安装。
- 首次构建较慢：wrangler 会拉取依赖并安装工具（如 wasm-bindgen）。

请求体读取

Worker 环境下读取请求体需使用 `req.take_body()` + `BodyExt::collect()`：
```rust
use http_body_util::BodyExt;

async fn read_body_bytes(req: &mut silent::Request) -> silent::Result<Vec<u8>> {
    let body = req.take_body();
    let collected = body.collect().await.map_err(|e| {
        silent::SilentError::business_error(
            silent::StatusCode::BAD_REQUEST,
            format!("read body error: {e}"),
        )
    })?;
    Ok(collected.to_bytes().to_vec())
}
```

JSON 请求体可直接使用 `req.json_parse::<T>().await`。

范围与验收（本仓示例）
- `examples/cloudflare-worker`，产物 `cdylib`，目标 `wasm32-unknown-unknown`。
- 能执行 `cargo build -p example-cloudflare-worker --target wasm32-unknown-unknown` 成功。
- 在 `wrangler dev` 下本地预览，所有路由正常工作：
  - `/hello` 返回文本
  - `/kv/<key>` 支持 GET/PUT/DELETE
  - `/d1/users` 支持 GET/POST
  - `/r2/<key>` 支持 GET/PUT/DELETE
