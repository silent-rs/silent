# QUIC 传输参数配置说明

`ServerConfig` 新增 `quic_transport` 字段，可用来统一配置 QUIC 传输参数，并通过 `QuicEndpointListener::from_server_config` 复用：

```rust
use silent::quic::QuicTransportConfig;
use silent::{ServerConfig, QuicEndpointListener, Server};
use std::time::Duration;

let mut server_config = ServerConfig::default();
server_config.quic_transport = Some(QuicTransportConfig {
    keep_alive_interval: Some(Duration::from_secs(15)),   // QUIC PING 周期
    max_idle_timeout: Some(Duration::from_secs(120)),     // 空闲超时
    max_bidirectional_streams: Some(256),                 // 双向流并发
    max_unidirectional_streams: Some(64),                 // 单向流并发
    max_datagram_recv_size: Some(128 * 1024),             // Datagram 接收上限
    alpn_protocols: Some(vec![b"h3".to_vec(), b"h3-29".to_vec()]),
});

let listener = QuicEndpointListener::from_server_config(bind_addr, &store, &server_config)
    .with_http_fallback();

Server::new()
    .with_config(server_config)
    .listen(listener)
    .serve(routes)
    .await;
```

## 字段说明（均为 `Option`，`None` 时使用 Quinn 默认）
- `keep_alive_interval`: QUIC keep-alive 间隔（PING），避免 NAT 超时。
- `max_idle_timeout`: 连接空闲超时。
- `max_bidirectional_streams` / `max_unidirectional_streams`: 并发流上限。
- `max_datagram_recv_size`: Datagram 接收缓冲上限，避免大包占用内存。
- `alpn_protocols`: ALPN 列表，默认 `["h3", "h3-29"]`，可根据业务调整。
