# QUIC / HTTP/3 / WebTransport 生产化示例

本示例展示 Silent 在 HTTP/3 + WebTransport 场景下的生产配置：Alt-Svc 自动注入、连接/帧/Datagram 限制、自定义 WebTransport Handler、HTTP/3 共享中间件。

## 运行

```bash
cargo run -p example-quic
```

- 默认监听 `127.0.0.1:4433`（QUIC + HTTP/1.1 回退，TLS 证书复用 `examples/tls/certs/`）
- Alt-Svc 自动指向 4433，`curl --http3 -k https://127.0.0.1:4433/api/health`
- 响应会带上 `x-powered-by: silent-http3`（证明 HTTP/3 复用与 HTTP/1.1 相同的中间件链）

## WebTransport Handler

- 自定义 `ChatHandler` 替换默认 Echo：回显文本并附带 session id（输入 `bye` 结束）。
- WebTransport 限制：帧大小、读超时、Datagram 上限/速率均由 `ConnectionLimits` 设置。

## 证书切换与热载

- HTTP/1.1/HTTP/2 可用 `ReloadableCertificateStore` 直接热载。
- QUIC 证书需重建 `QuicEndpointListener`，请参考 `docs/quic-ops.md` 的“QUIC 证书切换验证流程”。
