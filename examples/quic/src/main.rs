use anyhow::{Result, anyhow};
use silent::QuicEndpointListener;
use silent::prelude::*;
use silent::{ServerConfig, quic::QuicTransportConfig};
use std::time::Duration;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<()> {
    init_tracing();
    install_rustls_provider()?;

    let routes = build_routes().with_quic_port(4433); // 自动添加 Alt-Svc 中间件

    // 端口与绑定地址由用户显式设置
    let bind_addr: std::net::SocketAddr = "127.0.0.1:4433".parse().unwrap();
    let store = certificate_store()?;
    let server_config = ServerConfig {
        quic_transport: Some(QuicTransportConfig {
            keep_alive_interval: Some(Duration::from_secs(15)),
            max_idle_timeout: Some(Duration::from_secs(120)),
            max_bidirectional_streams: Some(256),
            max_unidirectional_streams: Some(64),
            max_datagram_recv_size: Some(128 * 1024),
            enable_datagram: true,
            alpn_protocols: Some(vec![b"h3".to_vec(), b"h3-29".to_vec()]),
        }),
        ..Default::default()
    };

    // QUIC listener with HTTP fallback (自动附加 HTTP/1.1 + TLS listener)
    let listener = QuicEndpointListener::from_server_config(bind_addr, &store, &server_config)
        .with_http_fallback();

    Server::new()
        .with_config(server_config)
        .listen(listener)
        .serve(routes)
        .await;

    Ok(())
}

fn init_tracing() {
    let env_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    tracing_subscriber::fmt()
        .with_env_filter(env_filter)
        .with_target(false)
        .with_thread_ids(true)
        .compact()
        .init();
}

fn install_rustls_provider() -> Result<()> {
    rustls::crypto::ring::default_provider()
        .install_default()
        .map_err(|_| anyhow!("初始化 Rustls 加密提供者失败"))
}

// 为示例提供一个证书加载方法：复用 tls 示例的本地证书
fn certificate_store() -> Result<silent::CertificateStore> {
    let builder = silent::CertificateStore::builder()
        .cert_path("./examples/tls/certs/localhost+2.pem")
        .key_path("./examples/tls/certs/localhost+2-key.pem");
    builder.build()
}

fn build_routes() -> Route {
    async fn index(_req: Request) -> silent::Result<&'static str> {
        Ok("Hello from HTTP/3")
    }

    let mut root = Route::new_root();
    root.push(Route::new("").get(index));
    root
}
