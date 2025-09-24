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

路由与适配
- 使用 `WorkRoute` 适配 Cloudflare Worker 的 `Request/Response` 与 Silent 的 `Request/Response`。
- 适配中使用 `Response::status()` 与 `Response::take_body()` 完成状态与字节聚合。
- 响应体通过 `http-body-util::BodyExt::collect` 聚合，兼容 Once/Chunks/Stream/Incoming/Boxed。

只读 Configs（重要）
- 由于 Wasm/Workers 的执行模型，实例的跨请求复用不可保证，且可能冷启动。处理器内对 `Configs` 的修改不会在后续请求中保持。
- 将 `Configs` 视为只读配置的载体，仅在初始化阶段注入不可变参数（常量、开关、外部服务句柄等）。
- 如需跨请求可变状态，请使用 Cloudflare 的持久化能力：KV、Durable Objects、D1、R2、Queues 等。

Env 与 Context 的使用
- `Env`：用于获取绑定（KV/DO/D1/R2/Queues 等）。建议将只读句柄注入到 `Configs`，供路由/处理器读取。
- `Context`：用于调度后台任务（`ctx.wait_until(fut)`），任务可在响应返回后继续执行。它是“每次请求”的上下文，不建议放入 `Configs`；如需在处理链上传递，可放入 `Request.extensions`。

示例：注入 KV 句柄并使用 Context 调度后台任务
```rust
use worker::{Context, Env, Request, Response, Result};
use silent::prelude::*;

#[worker::event(fetch)]
pub async fn main(req: Request, env: Env, ctx: Context) -> Result<Response> {
    console_error_panic_hook::set_once();

    // 1) 获取绑定（KV）
    let kv = env.kv("MY_KV_NAMESPACE")?; // 在 wrangler.toml 中配置

    // 2) 注入只读配置
    let mut cfg = Configs::default();
    cfg.insert(kv);

    // 3) 构建 WorkRoute 并注入只读 Configs
    let wr = WorkRoute::new(get_route()).with_configs(cfg);

    // 3.1) 使用 Context 调度后台任务（响应返回后执行）
    ctx.wait_until(async move {
        // 例如：写入日志、异步清理、异步持久化等
        Ok(())
    });

    // 4) 调用路由
    Ok(wr.call(req).await)
}

fn get_route() -> Route {
    Route::new_root().append(Route::new("hello").get(|req: Request| async move {
        use worker::kv::KvStore;
        let kv = req.get_config::<KvStore>()?;
        // kv.put("k", "v").execute().await?; // 示例持久化
        Ok("ok".to_string())
    }))
}
```

本地预览（wrangler dev）
```bash
cd examples/cloudflare-worker
wrangler dev
```
默认监听 `http://127.0.0.1:8787`，测试：
```bash
curl http://127.0.0.1:8787/hello
```

构建与部署
- 登录账号：`wrangler login`
- 构建 + 部署：
```bash
cd examples/cloudflare-worker
wrangler deploy
```
部署完成后，wrangler 会输出可访问的生产地址。

最小 wrangler.toml 配置
`examples/cloudflare-worker/wrangler.toml`：
```toml
name = "silent-cloudflare-worker"
main = "build/worker/shim.mjs"
compatibility_date = "2024-09-01"

[build]
command = "cargo install -q worker-build && worker-build --release"
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

范围与验收（本仓示例）
- 新增 `examples/cloudflare-worker`，产物 `cdylib`，目标 `wasm32-unknown-unknown`。
- 能执行 `cargo build -p example-cloudflare-worker --target wasm32-unknown-unknown` 成功。
- 在 `wrangler dev` 下本地预览，`/hello` 正常返回文本。
