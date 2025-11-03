use std::sync::Arc;
use std::time::Duration;

use quinn::Endpoint;
use quinn::ServerConfig;
use quinn::crypto::rustls::QuicServerConfig;
use tracing::error;

use crate::AcceptFuture;
use crate::BoxedConnection;
use crate::CertificateStore;
use crate::Listen;
use crate::server::listener::TlsListener;
use std::net::{SocketAddr, TcpListener as StdTcpListener};

pub struct QuicEndpointListener {
    endpoint: Endpoint,
    store: CertificateStore,
}

impl QuicEndpointListener {
    pub fn new(bind_addr: SocketAddr, store: &CertificateStore) -> Self {
        let rustls_config = store.rustls_server_config(&[b"h3", b"h3-29"]).unwrap();
        let mut server_config =
            ServerConfig::with_crypto(Arc::new(QuicServerConfig::try_from(rustls_config).unwrap()));

        if let Some(transport_config) = Arc::get_mut(&mut server_config.transport) {
            transport_config.keep_alive_interval(Some(Duration::from_secs(30)));
        }

        let endpoint = Endpoint::server(server_config, bind_addr)
            .map_err(|e| {
                error!("QUIC Endpoint 创建失败: {e}");
                e
            })
            .unwrap();
        Self {
            endpoint,
            store: store.clone(),
        }
    }

    /// 创建带有 HTTP 降级的混合 listener
    ///
    /// 此方法会自动在相同端口创建一个 TLS HTTP listener 作为降级，
    /// 当客户端不支持 QUIC 时可以使用 HTTP/1.1 或 HTTP/2。
    /// QUIC 使用 UDP，HTTP 使用 TCP，因此可以共享同一端口。
    ///
    /// # 示例
    /// ```no_run
    /// use silent::prelude::*;
    /// use silent_quic::{QuicEndpointListener, certificate_store};
    ///
    /// # tokio_test::block_on(async {
    /// let bind_addr = "127.0.0.1:4433".parse().unwrap();
    /// let store = certificate_store().unwrap();
    ///
    /// Server::new()
    ///     .listen(QuicEndpointListener::new(bind_addr, &store).with_http_fallback())
    ///     .serve(routes)
    ///     .await;
    /// # })
    /// ```
    pub fn with_http_fallback(self) -> HybridListener {
        let bind_addr = self.endpoint.local_addr().unwrap();

        // 在同一端口创建 TCP listener（HTTP 降级）
        let tcp_listener =
            StdTcpListener::bind(bind_addr).expect("Failed to bind TCP listener for HTTP fallback");
        let http_listener =
            crate::server::listener::Listener::from(tcp_listener).tls_with_cert(&self.store);

        HybridListener {
            quic: self,
            http: http_listener,
        }
    }
}

/// 混合 Listener，同时支持 QUIC 和 HTTP 降级
///
/// 此 listener 内部包含 QUIC (UDP) 和 HTTP TLS (TCP) 两个 listener，
/// 它们共享同一个端口，会同时监听两者的连接请求。
pub struct HybridListener {
    quic: QuicEndpointListener,
    http: TlsListener,
}

impl Listen for HybridListener {
    fn accept(&self) -> AcceptFuture<'_> {
        Box::pin(async move {
            tokio::select! {
                // 监听 QUIC 连接
                quic_result = self.quic.accept() => quic_result,
                // 监听 HTTP 连接
                http_result = self.http.accept() => http_result,
            }
        })
    }

    fn local_addr(&self) -> std::io::Result<crate::SocketAddr> {
        // 返回 QUIC listener 的地址
        self.quic.local_addr()
    }
}

impl Listen for QuicEndpointListener {
    fn accept(&self) -> AcceptFuture<'_> {
        Box::pin(async move {
            match self.endpoint.accept().await {
                Some(incoming) => {
                    let remote = crate::SocketAddr::from(incoming.remote_address());
                    let connection: BoxedConnection =
                        Box::new(super::connection::QuicConnection::new(incoming));
                    Ok((connection, remote))
                }
                None => Err(std::io::Error::other("QUIC Endpoint 已关闭")),
            }
        })
    }
    fn local_addr(&self) -> std::io::Result<crate::SocketAddr> {
        self.endpoint
            .local_addr()
            .map(crate::SocketAddr::from)
            .map_err(std::io::Error::other)
    }
}
