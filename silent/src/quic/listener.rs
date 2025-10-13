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
use std::net::SocketAddr;

pub struct QuicEndpointListener {
    endpoint: Endpoint,
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
        Self { endpoint }
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
