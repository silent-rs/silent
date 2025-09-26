# Silent 运行时中立使用指南（用户执行的 runtime 决定）

本文档说明在不同异步运行时下使用 Silent 时的特性组合与启动方式。

## 要点
- 运行时选择由“用户如何启动程序”决定（Tokio/async-std/smol 等）。
- 框架内部已统一常用能力：spawn、timeout、定时器、RwLock、mpsc 通道。
- `hyper-server` 特性默认开启，提供原有 Hyper + Tokio 后端；若禁用，可获得完全无 Tokio 依赖的 async-io 服务器实现（目前为实验状态）。

## 特性与依赖建议
- 默认：`default-features = true`（等价于启用 `server` + `hyper-server`）。适合保持与历史版本一致的行为。
- 纯 async-io：`default-features = false`，显式启用 `features = ["server"]`。当前 HTTP 传输为实验实现，暂不支持 HTTP/2、WebSocket 升级。
- gRPC / Cloudflare Worker：需要保留 `hyper-server`（或显式启用）；这些扩展依赖原有 Tokio/H2 栈。
- 通用依赖：`async-compat`（当在非 Tokio 运行时中运行启用了 `hyper-server` 的构建时）
- Tokio：`tokio = { version = "1", features = ["full"] }`
- async-std：`async-std = { version = "1", features = ["attributes"] }`
- smol：`smol = "2"`, `async-global-executor = "2"`

## 启动方式示例

- Tokio 环境（默认推荐）
```rust
#[tokio::main]
async fn main() {
    silent::logger::fmt().init();
    let route = silent::Route::new("").get(|_req: silent::Request| async { Ok("ok") });
    silent::Server::new().run(route);
}
```

- async-std 环境（使用 async-compat 适配 Tokio 后端）
```rust
#[async_std::main]
async fn main() {
    silent::logger::fmt().init();
    let route = silent::Route::new("").get(|_req: silent::Request| async { Ok("ok") });
    // 直接调用 serve，并用 async-compat 兼容 Tokio 相关 Future
    async_compat::Compat::new(async move {
        silent::Server::new().serve(route).await;
    })
    .await;
}
```

- smol 环境（使用 async-compat 适配 Tokio 后端）
```rust
fn main() {
    silent::logger::fmt().init();
    smol::block_on(async {
        let route = silent::Route::new("").get(|_req: silent::Request| async { Ok("ok") });
        async_compat::Compat::new(async move {
            silent::Server::new().serve(route).await;
        })
        .await;
    });
}
```

## 说明与限制
- `hyper-server` 开启时：仍使用 Hyper 的 Tokio I/O 适配（TokioIo）。非 Tokio 环境通过 `async-compat` 可正常运行。
- `hyper-server` 关闭时：提供基础 HTTP/1.1 支持，含 chunked 请求解码与 keep-alive（默认 32 次流水线 / 15 秒超时），暂不支持 WebSocket 升级、HTTP/2、TLS；该路径仍在迭代中，建议先行验证再用于生产。
- 信号处理：默认使用 `async-ctrlc`，与运行时无关。
- WebSocket/SSE/中间件/Session：已统一为运行时中立实现，不依赖 tokio 同步原语。

## 常见问题
- Q: 不使用 Tokio 也能跑吗？
  - A: 可以。请按上面的 async-std/smol 示例，通过 `async-compat` 包装 `Server::serve`。
- Q: 我已有自己的执行器/线程池？
  - A: 可以。框架内部使用 `async_global_executor::spawn` 做兜底，不强制要求特定执行器。
