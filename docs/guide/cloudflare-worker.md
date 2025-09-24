# Cloudflare Worker 使用说明（Silent 路由）

本说明展示如何在 Cloudflare Workers 上运行 Silent 的路由示例，并使用 Wrangler 开发、预览与部署。参考官方入门指南：https://developers.cloudflare.com/workers/get-started/guide/

## 环境准备
- Rust + wasm 目标
  - `rustup target add wasm32-unknown-unknown`
- Wrangler（推荐全局安装）
  - npm: `npm i -g wrangler`
  - 或 Homebrew: `brew install cloudflare/wrangler/wrangler`
- 可选：安装 `worker-build`（wrangler 会调用）
  - `cargo install -q worker-build`

## 示例位置
- 目录：`examples/cloudflare-worker`
- 入口：`examples/cloudflare-worker/src/lib.rs`
- 路由：`examples/cloudflare-worker/src/route.rs`
- 配置：`examples/cloudflare-worker/wrangler.toml`

## 路由与适配
- 使用 `WorkRoute` 适配 Cloudflare Worker 的 `Request/Response` 与 Silent 的 `Request/Response`。
- 只读 Configs（重要）：在 Workers 环境中，请仅在初始化阶段通过 `WorkRoute::with_configs` 注入只读配置；不要依赖在请求处理中对 Configs 的修改（不会跨请求持久化）。

示例（节选）：
```rust
use silent::prelude::*;
use std::sync::{Arc, Mutex};

pub fn get_route() -> WorkRoute {
    // 初始化阶段注入只读配置（跨请求不可变）
    let mut configs = Configs::default();
    configs.insert(Arc::new(Mutex::new(0i64))); // 如需可变状态，请改用 KV / DO / D1 等

    let route = Route::new_root().append(Route::new("hello").get(|req: Request| async move {
        // 仅演示用：读出配置（注意：不要在此处依赖跨请求可变）
        let counter = req.get_config::<Arc<Mutex<i64>>>()?;
        let current = *counter.lock().unwrap();
        Ok(format!("hello from Worker, step(readonly): {}", current))
    }));

    WorkRoute::new(route).with_configs(configs)
}
```

更多关于只读限制详见：`docs/requirements/cloudflare-workers.md` 中“重要限制：Workers 中的 Configs 仅支持只读”。

## 只读 Configs 与状态管理
- Configs 仅承载只读配置（例如：开关、常量、外部服务客户端句柄等）。
- 任何“数据值”的修改都不会在后续请求中保持；不要把 `Arc<Mutex<T>>` 等作为跨请求状态管理手段。
- 如需跨请求可变状态，请使用 Cloudflare 的持久化能力：KV、Durable Objects、D1、R2、Queues 等。

## 使用 Env 获取绑定（KV/DO/D1 等）
在 `#[worker::event(fetch)]` 的入口可获得 `Env`，可通过它获取绑定，并将只读句柄注入到 `Configs` 中，供路由/处理器使用。

示例：在入口处注入 KV 句柄（只读句柄本身可用于读写持久化存储）
```rust
use worker::{Context, Env, Request, Response, Result};
use silent::prelude::*;

#[worker::event(fetch)]
pub async fn main(req: Request, env: Env, _ctx: Context) -> Result<Response> {
    console_error_panic_hook::set_once();

    // 1) 获取绑定（例如：KV）
    let kv = env.kv("MY_KV_NAMESPACE")?; // 在 wrangler.toml 中配置

    // 2) 注入只读配置（把句柄存放到 Configs 中）
    let mut cfg = Configs::default();
    cfg.insert(kv);

    // 3) 构建 WorkRoute 并注入只读 Configs
    let wr = WorkRoute::new(get_route()).with_configs(cfg);

    // 4) 调用路由
    Ok(wr.call(req).await)
}

fn get_route() -> Route {
    Route::new_root().append(Route::new("hello").get(|req: Request| async move {
        // 在处理器中读取只读句柄并使用（例如：调用 KV）
        use worker::kv::KvStore;
        let kv = req.get_config::<KvStore>()?;
        // kv.put("key", "value").execute().await?; // 示例：写入
        Ok("ok".to_string())
    }))
}
```

## 本地预览（wrangler dev）
在示例目录执行：
```bash
cd examples/cloudflare-worker
wrangler dev
```
默认监听 `http://127.0.0.1:8787`，测试：
```bash
curl http://127.0.0.1:8787/hello
```

## 构建与部署
- 登录账号：
```bash
wrangler login
```
- 构建 + 部署：
```bash
cd examples/cloudflare-worker
wrangler deploy
```
部署完成后，Wrangler 将输出可访问的生产地址。

## wrangler.toml 说明（示例已内置）
`examples/cloudflare-worker/wrangler.toml` 包含最小工作配置，例如：
```toml
name = "silent-cloudflare-worker"
main = "build/worker/shim.mjs"
compatibility_date = "2024-09-01"

[build]
command = "cargo install -q worker-build && worker-build --release"
```
Wrangler 将使用 `worker-build` 将 Rust 工程编译为 Wasm，并生成可运行的 Worker 入口。

## 环境变量与机密
- 普通变量：在 `wrangler.toml` 的 `[vars]` 中定义
- 机密：
```bash
wrangler secret put MY_SECRET
```
处理器中可通过 `Env` 获取，或在 `with_configs` 时注入只读配置。

## 常见问题
- Wasm/Workers 下请求生命周期短且实例不可预测：不要依赖进程内“全局可变状态”。
- 需要持久化：使用 Cloudflare KV / Durable Objects / D1 / R2 等。
- 构建失败缺少 target：确保 `wasm32-unknown-unknown` 已安装。
- 首次构建时间较长：`wrangler` 会自动拉取依赖并安装 `wasm-bindgen` 等工具。

## 小结
- 使用 `WorkRoute` 将 Silent 路由运行在 Cloudflare Workers。
- 配置仅注入只读内容；跨请求可变状态请使用 Cloudflare 的持久化服务。
- 开发：`wrangler dev`；部署：`wrangler deploy`。
