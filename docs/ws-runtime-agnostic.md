# WebSocket 运行时无关接入指南（server 关闭/wasm）

当禁用 `server` 特性（如 wasm/WASI 环境）时，不再依赖 hyper/OnUpgrade。
本框架提供了泛型化的升级通道与连接类型，允许你注入任意实现
`futures::io::{AsyncRead, AsyncWrite}` 的底层连接到请求上下文，再构造 WebSocket。

## 关键类型与入口
- `silent::ws::upgrade::AsyncUpgradeRx<S>`：注入式升级接收器，`S` 为底层 IO 类型。
- `silent::ws::upgrade::Upgraded<S>`：包含 `WebSocketParts` 与注入的底层流 `S`。
- `silent::ws::upgrade::on_generic<S>(req)`：从 `Request.extensions` 中提取 `AsyncUpgradeRx<S>` 并等待升级流。
- `silent::ws::WebSocket<S>`：基于 `S` 构造的 WebSocket，会话流类型为 `S`。

> 在启用 `server` 的情况下，也提供了 `ServerUpgradedIo` 与 `on(req)` 的便捷入口。

## 注入流程（非 server 环境）
1. 宿主或运行时（如 Workers/Spin/WasmEdge）完成 WS 握手，获得一个实现 `S` 的流。
2. 在 `Request.extensions_mut()` 注入 `AsyncUpgradeRx<S>::new(rx)`，其中 `rx` 是等待升级流的 `oneshot::Receiver<S>`。
3. 应用侧调用 `on_generic<S>(req)` 获取 `Upgraded<S>`，再用 `WebSocket::from_raw_socket(upgraded, Role::Server, None)` 构造 WS。

## 最小示例
参见 `examples/ws_injector`。核心步骤如下：

```rust
use futures::channel::oneshot;
use tokio_util::compat::TokioAsyncReadCompatExt;
use silent::ws::upgrade::{on_generic, AsyncUpgradeRx};

// 1) 宿主产生底层流 S（示例用 tokio::io::duplex 模拟），并适配到 futures-io
let (_client, server_side) = tokio::io::duplex(64);
let compat_stream = server_side.compat();

// 2) 注入 AsyncUpgradeRx<S> 到 Request
let (tx, rx) = oneshot::channel();
let mut req = silent::Request::default();
req.extensions_mut().insert(AsyncUpgradeRx::new(rx));
let _ = tx.send(compat_stream);

// 3) 提取 Upgraded<S> 并（可选）构造 WebSocket
let upgraded = on_generic::<tokio_util::compat::Compat<tokio::io::DuplexStream>>(req).await?;
// let ws = silent::ws::WebSocket::from_raw_socket(upgraded, protocol::Role::Server, None).await;
```

## 设计要点
- WS 模块完全依赖 `futures-io`，不再直接依赖 tokio；
- server 路径仅在 `hyper_service` 内部对 `OnUpgrade` 做一次 TokioIo + compat 适配，
  对上层透明；
- wasm/WASI 可以完全绕开 hyper：只需注入 `S` 即可构建 WS；
- 示例使用 tokio 仅作为演示运行时，实际 wasm 可替换为任意支持的异步环境。

## 常见问题
- 是否可以在 wasm32-unknown-unknown 使用 `async_tungstenite::tokio::TokioAdapter`？
  - 不建议；TokioAdapter 依赖 tokio IO，在浏览器/通用 wasm 环境不可用。
- 如果禁用 `server`，是否还能使用路由/Server？
  - 不能；Server 相关 API 由 `server` 特性提供。此时应由宿主提供请求/响应上下文，
    仅复用框架的 WS、SSE、模板等纯逻辑模块。
