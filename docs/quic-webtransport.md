# QUIC / HTTP/3 / WebTransport 使用与编写说明

> 状态：实验性（feature 可选，默认未启用）。当前已支持基于 `quinn + h3` 的 HTTP/3 与 WebTransport 基础能力，推荐在测试或内网环境先行验证。

## 功能概览

- QUIC 监听与同端口回退：UDP QUIC 与 TCP HTTPS 共享同一端口（混合监听）。
- HTTP/3（h3）：常规请求-响应模型已打通，可与现有路由/中间件协同。
- Alt-Svc：通过中间件向客户端宣告 h3 可用，浏览器可自动升级。
- WebTransport：已开启握手与数据帧收发，内置 Echo 示例（会话 ID 使用 `scru128` 生成）。

## 前置要求

- Rust 1.77+ 与 Tokio 运行时。
- 证书（PEM 或 DER），建议准备一套本地开发证书（仓库已提供示例）。
- 客户端需支持 HTTP/3（如 curl --http3、现代浏览器）。

## 启用 QUIC 特性

- 在你的可执行包或示例中，启用 `silent` 的 `quic` feature：

- Cargo.toml
```toml
[dependencies]
silent = { path = "../../silent", features = ["quic"] }
```

> 工作区示例可参考：`examples/quic/Cargo.toml`

## 证书准备

- 使用 `CertificateStore` 构建 rustls 服务器配置，并设置 ALPN：库内部会自动为 QUIC 配置 `h3`/`h3-29`。
- 你可以直接复用示例证书：`examples/tls/certs/localhost+2.pem` 与 `localhost+2-key.pem`。

- 代码示例
```rust
fn certificate_store() -> anyhow::Result<silent::CertificateStore> {
    let builder = silent::CertificateStore::builder()
        .cert_path("./examples/tls/certs/localhost+2.pem")
        .key_path("./examples/tls/certs/localhost+2-key.pem");
    Ok(builder.build()?)
}
```

> 若使用自签证书，浏览器需要手动信任根证书；curl 可使用 `-k` 跳过校验进行快速验证。

## 启动服务（QUIC + HTTPS 回退）

- 使用 `QuicEndpointListener::new(...).with_http_fallback()` 创建混合监听；
- 通过 `Route::with_quic_port(port)` 自动添加 Alt-Svc；

- 最小示例（与 `examples/quic/src/main.rs` 一致）：
```rust
use anyhow::{Result, anyhow};
use silent::prelude::*;
use silent::QuicEndpointListener;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<()> {
    init_tracing();
    install_rustls_provider()?;

    let routes = build_routes().with_quic_port(4433); // 自动添加 Alt-Svc

    let bind_addr: std::net::SocketAddr = "127.0.0.1:4433".parse().unwrap();
    let store = certificate_store()?;

    let listener = QuicEndpointListener::new(bind_addr, &store).with_http_fallback();
    Server::new().listen(listener).serve(routes).await;
    Ok(())
}

fn install_rustls_provider() -> Result<()> {
    rustls::crypto::ring::default_provider()
        .install_default()
        .map_err(|_| anyhow!("初始化 Rustls 加密提供者失败"))
}

fn build_routes() -> Route {
    async fn index(_req: Request) -> silent::Result<&'static str> {
        Ok("Hello from HTTP/3")
    }
    let mut root = Route::new_root();
    root.push(Route::new("").get(index));
    root
}
```

## Alt-Svc（HTTP/3 升级宣告）

- `with_quic_port(port)` 会自动挂载 Alt-Svc 中间件，效果等价于：
  `Alt-Svc: h3=":4433"; ma=86400`

> 注意：浏览器通常会先通过 HTTPS 访问一次，收到 Alt-Svc 后在后续请求中升级到 H3。

## 客户端验证

- curl：
```bash
curl --http3 -k https://127.0.0.1:4433/
```

- 浏览器：
  - 访问 `https://127.0.0.1:4433/`；
  - 打开 DevTools 网络面板，确认协议列为 `h3`；
  - 首次访问可能仍为 `http/2`，刷新后升级。

## WebTransport 使用说明（实验）

- 能力：
  - 服务端已启用 WebTransport 握手（扩展 CONNECT）、DATAGRAM；
  - 内置 Echo 处理器用于演示：收到数据即回发，便于验证链路。

- 设计要点：
  - 会话标识使用 `scru128` 生成，满足高可用 ID 需求；
  - API 当前未开放自定义 `WebTransportHandler` 的对外注册接口（默认 Echo）。

- 客户端验证思路：
  - 使用支持 WebTransport 的浏览器/JS API 或测试工具，发起 CONNECT + 发送数据帧；
  - 期望收到 `echo(webtransport): <your_message>` 响应。

- 可扩展建议（未来 API 方向）：
  - 在 `Route` 上提供 `with_webtransport(path, handler)` 用于注册自定义处理器；
  - 增加鉴权、中止条件、并发/帧大小/会话上限等保护；
  - 暴露 QUIC/H3 参数（如并发流、超时、拥塞控制）并纳入配置。

> 如需自定义 Handler，目前需要修改库内部 `quic::service` 中默认的 Echo 注册逻辑。

## 性能与安全建议

- 请求体大小与内存：HTTP/3 示例实现会将请求体聚合到内存，生产环境建议改为流式处理并设置上限。
- 证书与信任：本地自签证书仅用于开发测试，生产环境请使用可信 CA 证书并妥善保管私钥。
- 观测：配合 `tracing`/metrics 输出关键事件（握手、升级、错误、会话数量）。

## 常见问题

- 浏览器未升级到 H3：
  - 确认已返回 Alt-Svc；
  - 刷新后查看协议列；
  - 证书是否被信任（或使用域名而非 IP）。
- QUIC 无法建立：
  - 端口是否被占用；
  - 防火墙/UDP 是否放行；
  - 证书/私钥路径是否正确。

## 参考示例

- 示例入口：`examples/quic/src/main.rs`
- 证书样例：`examples/tls/certs/`
