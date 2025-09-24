# Cloudflare Workers 适配 Silent 路由 需求整理

- 目标：在 Cloudflare Workers（workers-rs）环境中直接运行 Silent 的 `Route` 路由，开发 Serverless 程序。
- 参考：cloudflare/workers-rs（crate: `worker`）。

## 范围
- 新增一个 `examples/cloudflare-worker` 示例，产物为 `cdylib`，目标平台 `wasm32-unknown-unknown`。
- 在示例中把 `worker::Request` 映射为 `silent::Request`，执行 `Route`，再把 `silent::Response` 映射为 `worker::Response`。
- 不依赖 `server` 特性（不启用 tokio 网络、fs 等），仅启用最小必要的同步能力以便 `OnceCell` 等可编译。

## 非目标
- 不在本迭代内跑本地 Wrangler 预览与部署，仅保证 `cargo build --target wasm32-unknown-unknown -p example-cloudflare-worker` 能通过。
- 不引入 Silent 行为上的破坏性调整。

## 技术要点
- `workers-rs` 版本：`worker = "0.6.6"`。
- `silent` 以 `default-features = false` 引入，开启一个轻量特性（本次新增 `wasi`）仅启用 `tokio/sync`。
- `silent::Response` 需要暴露 `status()` 和 `take_body()` 以便在 Worker 端完成响应转换。
- `ResBody` 通过 `http-body-util::BodyExt::collect` 聚合为字节，兼容 `Once/Chunks/Stream/Incoming/Boxed` 情况。

## 重要限制：Workers 中的 Configs 仅支持只读

- 原因：Cloudflare Workers 以 Wasm 隔离执行，实例的跨请求复用不可保证，且运行环境可能频繁冷启动。处理函数内对 `Configs` 中值的修改既不具备持久性，也不应依赖实例级“全局”状态。
- 约束：将 `Configs` 视为只读配置的载体，用于在初始化阶段注入不可变的运行参数（例如常量、连接信息、只读策略）。在请求处理过程中对 `Configs` 的修改不会在后续请求中可见。
- 推荐做法：
  - 仅在 Workers 初始化/路由装配阶段通过 `WorkRoute::with_configs(configs)` 注入只读配置。
  - 如需跨请求可变状态（计数、缓存等），请使用 Cloudflare 提供的持久化能力（KV、Durable Objects、D1、Queues 等），或在调用外部服务存储状态。
  - 若确需在单次请求链路中传递动态数据，请使用 `Request.extensions_mut()`/`Response.extensions_mut()` 或中间件上下文，但不要假设其跨请求可见。

示例（只读配置注入）：

```rust
// 初始化阶段注入只读配置
let mut cfg = Configs::default();
cfg.insert(MyReadOnlyCfg { feature_flag: true });
let wr = WorkRoute::new(route).with_configs(cfg);

// 处理器中读取
async fn handler(mut req: Request) -> Result<String, SilentError> {
    let ro = req.get_config::<MyReadOnlyCfg>()?;
    Ok(format!("flag = {}", ro.feature_flag))
}
```

## 验收标准
- 能在仓库根执行：`cargo build --target wasm32-unknown-unknown -p example-cloudflare-worker` 成功。
- 示例路由可在 Worker 入口中被执行（如 `/hello` 返回文本）。
