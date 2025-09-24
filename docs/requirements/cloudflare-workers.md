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

## 验收标准
- 能在仓库根执行：`cargo build --target wasm32-unknown-unknown -p example-cloudflare-worker` 成功。
- 示例路由可在 Worker 入口中被执行（如 `/hello` 返回文本）。
