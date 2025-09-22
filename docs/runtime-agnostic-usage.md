# Silent 运行时中立使用指南（用户执行的 runtime 决定）

本文档说明在不新增 feature 的前提下，如何在不同异步运行时下使用 Silent。

## 要点
- 运行时选择由“用户如何启动程序”决定（Tokio/async-std/smol 等）。
- 框架内部已统一常用能力：spawn、timeout、定时器、RwLock、mpsc 通道。
- 当前 HTTP 传输基于 Hyper + Tokio 后端；非 Tokio 场景可通过 `async-compat` 适配。

## 依赖建议
- 通用：`async-compat`（当在非 Tokio 运行时中运行需要 Tokio 后端的代码时）
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
- 传输层：当前仍使用 Hyper 的 Tokio I/O 适配（TokioIo）。非 Tokio 环境通过 `async-compat` 可正常运行；后续版本将继续推进更中立的传输层抽象。
- 信号处理：默认依赖 tokio 的 ctrl_c/terminate 监听；非 Tokio 环境同样建议通过 `async-compat` 运行。
- WebSocket/SSE/中间件/Session：已统一为运行时中立实现，不依赖 tokio 同步原语。

## 常见问题
- Q: 不使用 Tokio 也能跑吗？
  - A: 可以。请按上面的 async-std/smol 示例，通过 `async-compat` 包装 `Server::serve`。
- Q: 我已有自己的执行器/线程池？
  - A: 可以。框架内部使用 `async_global_executor::spawn` 做兜底，不强制要求特定执行器。
