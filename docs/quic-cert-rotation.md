# QUIC 证书切换示例流程

> 场景：quinn Endpoint 无法热更证书，需要新建 listener 并优雅切换流量。

## 步骤
1. **启动新实例**：使用新证书创建 `CertificateStore` + `QuicEndpointListener`，绑定备用端口（如 4434），启动 `Server`。
2. **验证流量**：本地或灰度节点执行：
   ```bash
   curl --http3 -k https://your.domain:4434/api/health
   ```
   确认握手与业务响应正常。
3. **切换入口**：
   - 反向代理/ALB：将 Alt-Svc/HTTP/3 路由指向 4434。
   - DNS：降低 TTL 后切换 A 记录或 SRV 记录到新端口（如需）。
4. **优雅关停旧实例**：
   - 调用 `Server::with_shutdown` 设置等待时长（例如 30s），收到信号后等待活动连接结束。
   - 观察日志/metrics，确认无新连接进入旧实例。
5. **回收旧实例**：确认连接数为 0 后停止旧进程。

## 关键代码片段（概念示例）
```rust
use silent::{Server, QuicEndpointListener, ServerConfig, ConnectionLimits};
use silent::quic::QuicTransportConfig;
use std::time::Duration;

fn make_server_config(cert: &silent::CertificateStore) -> (ServerConfig, QuicEndpointListener) {
    let mut cfg = ServerConfig::default();
    cfg.connection_limits = ConnectionLimits {
        max_body_size: Some(512 * 1024),
        h3_read_timeout: Some(Duration::from_secs(10)),
        max_webtransport_frame_size: Some(16 * 1024),
        ..Default::default()
    };
    cfg.quic_transport = Some(QuicTransportConfig::default());
    let listener = QuicEndpointListener::from_server_config("0.0.0.0:4434".parse().unwrap(), cert, &cfg);
    (cfg, listener)
}

async fn run_new_instance(routes: silent::Route) -> anyhow::Result<()> {
    let cert = silent::CertificateStore::builder()
        .cert_path("/path/new_cert.pem")
        .key_path("/path/new_key.pem")
        .build()?;
    let (cfg, listener) = make_server_config(&cert);
    Server::new()
        .with_config(cfg)
        .listen(listener)
        .with_shutdown(Duration::from_secs(30))
        .serve(routes)
        .await;
    Ok(())
}
```

## 运维要点
- **并行运行**：新旧实例可并行，依赖上层入口做流量切换。
- **观测**：监控 `silent.server.webtransport.handshake_ns`、`datagram_dropped`、`datagram_rate_limited` 等指标，确认新实例健康。
- **回退**：若切换失败，保持旧实例运行，恢复入口指向即可。
