use anyhow::{Result, anyhow};
use bytes::Bytes;
use silent::prelude::*;
use silent::quic::{QuicSession, QuicTransportConfig, WebTransportHandler, WebTransportStream};
use silent::{ConnectionLimits, QuicEndpointListener, RouteConnectionService, ServerConfig};
use std::sync::Arc;
use std::time::Duration;
use tracing::{info, warn};
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<()> {
    init_tracing();
    install_rustls_provider()?;

    let quic_port = 4433;
    let routes = build_routes(quic_port);
    let wt_handler: Arc<dyn WebTransportHandler> = Arc::new(ChatHandler);

    // 端口与绑定地址由用户显式设置
    let bind_addr: std::net::SocketAddr = format!("127.0.0.1:{quic_port}").parse().unwrap();
    let store = certificate_store()?;
    let server_config = ServerConfig {
        connection_limits: ConnectionLimits {
            max_body_size: Some(256 * 1024),
            h3_read_timeout: Some(Duration::from_secs(10)),
            max_webtransport_frame_size: Some(16 * 1024),
            webtransport_read_timeout: Some(Duration::from_secs(10)),
            max_webtransport_sessions: Some(32),
            webtransport_datagram_max_size: Some(1200),
            webtransport_datagram_rate: Some(100),
            webtransport_datagram_drop_metric: true,
            ..Default::default()
        },
        quic_transport: Some(QuicTransportConfig {
            keep_alive_interval: Some(Duration::from_secs(15)),
            max_idle_timeout: Some(Duration::from_secs(120)),
            max_bidirectional_streams: Some(256),
            max_unidirectional_streams: Some(64),
            max_datagram_recv_size: Some(128 * 1024),
            enable_datagram: true,
            alpn_protocols: Some(vec![b"h3".to_vec(), b"h3-29".to_vec()]),
        }),
    };

    // QUIC listener with HTTP fallback (自动附加 HTTP/1.1 + TLS listener)
    let listener = QuicEndpointListener::from_server_config(bind_addr, &store, &server_config)
        .with_http_fallback();

    let service = RouteConnectionService::new(routes).with_webtransport_handler(wt_handler);

    Server::new()
        .with_config(server_config)
        .listen(listener)
        .serve(service)
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

fn build_routes(quic_port: u16) -> Route {
    async fn index(_req: Request) -> silent::Result<&'static str> {
        Ok("Welcome to Silent HTTP/3 (Alt-Svc enabled)")
    }

    async fn health(_req: Request) -> silent::Result<&'static str> {
        Ok("ok")
    }

    let mut api = Route::new("api");
    api.push(Route::new("health").get(health));

    Route::new_root()
        .hook(PoweredByMiddleware)
        .append(Route::new("").get(index))
        .append(api)
        .with_quic_port(quic_port)
}

#[derive(Clone)]
struct PoweredByMiddleware;

#[async_trait::async_trait]
impl MiddleWareHandler for PoweredByMiddleware {
    async fn handle(&self, req: Request, next: &Next) -> silent::Result<Response> {
        let mut resp = next.call(req).await?;
        resp.headers_mut()
            .insert("x-powered-by", "silent-http3".parse().unwrap());
        Ok(resp)
    }
}

#[derive(Clone)]
struct ChatHandler;

#[async_trait::async_trait]
impl WebTransportHandler for ChatHandler {
    async fn handle(
        &self,
        session: Arc<QuicSession>,
        stream: &mut WebTransportStream,
    ) -> Result<()> {
        info!(session_id = session.id(), remote = %session.remote_addr(), "WebTransport session started");

        loop {
            let Some(frame) = stream.recv_data().await? else {
                break;
            };
            if frame.is_empty() {
                continue;
            }
            let text = String::from_utf8_lossy(&frame).trim().to_string();
            if text.eq_ignore_ascii_case("bye") {
                stream.send_data(Bytes::from_static(b"bye\n")).await?;
                break;
            }
            let reply = format!("session={} echo: {text}", session.id());
            if let Err(err) = stream.send_data(Bytes::from(reply)).await {
                warn!(session_id = session.id(), error = ?err, "failed to send reply");
            }
        }

        stream.finish().await?;
        info!(session_id = session.id(), "WebTransport session finished");
        Ok(())
    }
}
