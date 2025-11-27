# QUIC 运维注意事项（Alt-Svc / ALPN / 证书热载 / 回归）

## Alt-Svc / ALPN 对齐
- 推荐使用 `QuicEndpointListener::alt_svc_middleware()`，自动注入当前 QUIC 端口，避免与 HTTP 端口漂移。
- 自定义 ALPN：在 `QuicTransportConfig.alpn_protocols` 中配置（默认 `["h3", "h3-29"]`）。确保 TLS 证书的 SAN 覆盖对应域名。

## 证书热载
- HTTP/1.1/HTTP/2 已内置支持：使用 `ReloadableCertificateStore::from_paths` + `Listener::tls_with_reloadable`，文件变更后调用 `reload()` 即可。
- 示例：
```rust
use silent::{ReloadableCertificateStore, Server};
use silent::server::listener::Listener;

let store = ReloadableCertificateStore::from_paths("cert.pem", "key.pem", None)?;
let listener = Listener::bind(("0.0.0.0", 443))?.tls_with_reloadable(store.clone());

tokio::spawn(async move {
    // 监听文件变更后调用 reload（示意）
    loop {
        // watch/fsnotify ...
        store.reload()?;
    }
});

Server::new().listen(listener).serve(routes).await;
```
- QUIC 证书仍需重建 `QuicEndpointListener`（Quinn Endpoint 配置不可热更），可先构建新 listener 后优雅关停旧服务。
- 详见 `docs/quic-cert-rotation.md` 获取完整切换流程与示例代码。

## QUIC 证书切换验证流程
1. 使用新证书构建新的 `CertificateStore` 与 `QuicEndpointListener`，挂到备用端口（如 4434），启动新 `Server`。
2. 通过 ALB/反向代理或 DNS 将 HTTP/3/Alt-Svc 指向新端口，确认 `curl --http3`/浏览器握手成功。
3. 使用 `Server::with_shutdown` 触发旧实例优雅关停，确保已有会话完成后再关闭旧 QUIC endpoint。
4. 可选：HTTP/1.1/2 路径可直接用 `ReloadableCertificateStore` 热载证书，保持回退链路可用。

## 高延迟/丢包回归建议
- 客户端建议使用 `quinn`/`quinn-cli` 或 Chromium/`curl --http3`。
- 注入网络条件：`tc netem`（Linux）或 Clumsy（Windows）模拟 RTT/丢包/抖动。
  - 示例：`tc qdisc add dev lo root netem delay 100ms 20ms loss 1%`
- 建议覆盖场景：
  - HTTP/3 请求/响应大体积与分块回压（已在服务端增加响应让步以减轻 backpressure）。
  - WebTransport 会话并发、frame 上限、datagram 开关/限速（现限速/观测为占位，需要底层 datagram API 对接）。
  - 0-RTT/重传/迁移（需依赖客户端能力，记录观察结果）。

## 监控与埋点
- 关键指标：`silent.server.webtransport.handshake_ns`、`datagram_dropped`、`datagram_rate_limited`（datagram send/recv 已接入限速/体积校验，指标有效；超限/限速时丢弃并计数，不中断会话）。
- HTTP/3 响应发送已在大块数据后 `yield_now`，避免长时间占用 executor。
