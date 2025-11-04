# NetServer 设计对齐（Design Alignment）

> 版本：v0.1（实现中）
> 关联：`rfcs/2025-10-01-net-server-decoupling.md`（状态：Implementing）
> 模块：`silent::server::{net_server, listener, connection}`

---

## 概览

NetServer 提供与协议无关的网络接入循环：统一负责监听、接受连接、分发给连接处理器（ConnectionService），并处理关停信号与（后续）限流策略。HTTP/QUIC 等协议处理由上层 `Route` 或自定义 `ConnectionService` 实现完成。

目标：
- 保持现有 `Server` 对外行为兼容（HTTP 能力不变）。
- 收敛监听与接入循环，形成可复用的网络层。
- 为限流与优雅关停预留扩展点。

---

## ConnectionService 抽象

契约（Contract）：
- 输入：`(stream: BoxedConnection, peer: CoreSocketAddr)`
- 输出：`Future<Output = Result<(), BoxError>>`（错误仅记录，不影响主循环）
- 线程/生命周期：`Send + Sync + 'static`

签名与别名（已实现于 `service/mod.rs`）：

```rust
pub type BoxError = Box<dyn StdError + Send + Sync>;
pub type ConnectionFuture = Pin<Box<dyn Future<Output = Result<(), BoxError>> + Send>>;

pub trait ConnectionService: Send + Sync + 'static {
    fn call(&self, stream: BoxedConnection, peer: CoreSocketAddr) -> ConnectionFuture;
}

// 闭包到服务的泛型实现
impl<F, Fut> ConnectionService for F
where
    F: Send + Sync + 'static + Fn(BoxedConnection, CoreSocketAddr) -> Fut,
    Fut: Future<Output = Result<(), BoxError>> + Send + 'static,
{ /* ... */ }
```

注意：`Route` 已实现 `ConnectionService`，可直接作为处理器传入。

---

## NetServer API（构造与运行）

核心类型：`service::net_server::NetServer`

现有接口：
- `new() -> Self`：默认空监听器，后续可 `bind`/`listen`；若最终无监听器，则回退到 `127.0.0.1:0`。
- `from_parts(ListenersBuilder, shutdown_cb, listen_cb) -> Self`：供 `Server` 复用，保持兼容。
- `bind(SocketAddr) -> Self`：绑定 TCP 监听；可多次调用形成多监听器。
- `bind_unix<P: AsRef<Path>>(P) -> Self`（非 Windows）：绑定 Unix 监听。
- `listen<T: Listen>(T) -> Self`：接入自定义监听源（含 TLS 包装、H3、H2C 等）。
- `on_listen(Fn(&[CoreSocketAddr])) -> Self`：监听成功回调（只读视图）。
- `set_shutdown_callback(Fn() ) -> Self`：关停触发时回调（如刷新指标、清理资源）。
- `serve<H: ConnectionService>(self, handler: H) -> impl Future<Output = ()>`：异步运行。
- `run<H: ConnectionService>(self, handler: H) -> ()`：创建多线程 Tokio 运行时并阻塞执行。

预留接口（下一步）：
- `with_rate_limiter(...) -> Self`：令牌桶/队列上限/超时策略配置。
- `with_shutdown(...) -> Self` 或等价：优雅关停窗口、强制取消超时等。

错误与日志：
- 监听失败：`listen()` 返回 `io::Result`；在 `serve_connection_loop` 中传播并在上层 `.expect("server loop failed")`。
- 连接处理错误：记录 `tracing::error!` 并继续循环。
- 关停信号：`ctrl_c` 与（Unix）`SIGTERM`；触发 `shutdown_callback` 后退出 accept 循环。

---

## 监听器暴露（ListenersBuilder / Listeners）

最小公开能力（已存在于 `server::listener`）：
- `ListenersBuilder::bind(_)/bind_unix(_)`、`add_listener(_)`、`listen() -> io::Result<Listeners>`
- `Listeners::accept() -> Option<Result<(Box<dyn Connection>, CoreSocketAddr)>>`
- `Listeners::local_addrs(&self) -> &[CoreSocketAddr]`（只读切片视图）

说明：
- `local_addrs()` 返回借用切片，避免拷贝；保证 `on_listen` 回调读到的地址与日志一致。
- `Listen` trait 支持自定义监听源；`Listener`/`TlsListener` 提供常见封装。

---

## 最小时序（PoC）

```mermaid
description
sequenceDiagram
    autonumber
    participant App as App (Server/NetServer)
    participant L as Listeners
    participant OS as OS
    participant H as ConnectionService

    App->>L: listen()
    L->>OS: bind()/listen()
    L-->>App: Listeners + local_addrs
    App->>App: on_listen(&[addr])
    loop accept loop
        App->>L: accept()
        L-->>App: (stream, peer)
        App->>H: spawn handler.call(stream, peer)
        H-->>App: Result<(), BoxError>
    end
    App->>App: ctrl_c/SIGTERM => shutdown_callback()
    App->>App: 停止 accept，等待活动任务（后续：优雅关停超时）
```

---

## 成功准则与边界

成功：
- `Server::serve/run` 对外行为不变，示例可运行；NetServer 可被自定义协议复用。
- 在 `fmt`/`clippy`/`check`/`test`/`deny` 全通过的前提下，引入最小 API 表面。

边界：
- 限流与优雅关停的策略与参数将于下一步实现；当前仅有监听、分发与信号退出。

---

## 示例（草案）

```rust
use silent::server::{Server, ConnectionService};
use silent::prelude::*; // 假定 re-export Route 等

#[tokio::main]
async fn main() {
    let route = Route::new().get("/", |_| async { "hello" });

    // 兼容路径
    Server::new()
        .bind("127.0.0.1:18080".parse().unwrap())
        .on_listen(|addrs| tracing::info!(?addrs, "listening"))
        .serve(route)
        .await;
}
```

---

## 后续（Next）
- `with_rate_limiter()` 与 `with_shutdown()` 的参数设计与实现。
- 示例：`examples/net_server_basic/`、`examples/net_server_custom_protocol/`。
- 文档：为公共 API 增补 rustdoc（含 Errors/Panics/Examples）。
