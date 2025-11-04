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

    pub fn with_http_fallback(self) -> HybridListener {
        let bind_addr = self.endpoint.local_addr().unwrap();
        let tcp_listener =
            StdTcpListener::bind(bind_addr).expect("Failed to bind TCP listener for HTTP fallback");
        let http_listener = crate::server::listener::Listener::try_from(tcp_listener)
            .expect("Failed to convert TCP listener")
            .tls_with_cert(&self.store);

        HybridListener {
            quic: self,
            http: http_listener,
        }
    }
}

pub struct HybridListener {
    quic: QuicEndpointListener,
    http: TlsListener,
}

impl Listen for HybridListener {
    fn accept(&self) -> AcceptFuture<'_> {
        Box::pin(async move {
            tokio::select! {
                quic_result = self.quic.accept() => quic_result,
                http_result = self.http.accept() => http_result,
            }
        })
    }

    fn local_addr(&self) -> std::io::Result<crate::SocketAddr> {
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

#[cfg(all(test, feature = "quic"))]
mod tests {
    #[test]
    fn test_quic_listener_types_exist() {
        // 仅验证模块可用
    }
}
