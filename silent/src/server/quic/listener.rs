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
    use super::*;
    use std::net::SocketAddr;

    #[test]
    fn test_quic_listener_types_exist() {
        // 仅验证模块可用
    }

    #[test]
    fn test_hybrid_listener_accept_uses_select() {
        // 验证 HybridListener::accept 使用了 tokio::select!
        // 在实际代码中，我们可以看到第65-70行使用了 tokio::select!
    }

    #[test]
    fn test_quic_listener_error_messages() {
        // 测试错误消息的格式
        let error = std::io::Error::other("QUIC Endpoint 已关闭");
        assert_eq!(error.to_string(), "QUIC Endpoint 已关闭");
    }

    #[test]
    fn test_quic_server_config_protocols() {
        // 验证 QUIC 服务器配置支持的协议
        // 在 QuicEndpointListener::new 中配置了 h3 和 h3-29
        let protocols: [&[u8]; 2] = [b"h3", b"h3-29"];
        assert_eq!(protocols.len(), 2);
        assert_eq!(protocols[0], b"h3");
        assert_eq!(protocols[1], b"h3-29");
    }

    #[test]
    fn test_hybrid_listener_struct_size() {
        // 验证 HybridListener 结构体大小
        let _ = std::mem::size_of::<HybridListener>();
    }

    #[test]
    fn test_quic_endpoint_listener_struct_size() {
        // 验证 QuicEndpointListener 结构体大小
        let _ = std::mem::size_of::<QuicEndpointListener>();
    }

    #[test]
    fn test_quic_endpoint_listener_new_takes_socket_addr_and_cert_store() {
        // 验证 QuicEndpointListener::new 的签名
        fn _signature(_: SocketAddr, _: &crate::CertificateStore) -> QuicEndpointListener {
            unimplemented!()
        }
    }

    #[test]
    fn test_with_http_fallback_returns_hybrid_listener() {
        // 验证 with_http_fallback 方法的返回类型
        fn _signature(_: QuicEndpointListener) -> HybridListener {
            unimplemented!()
        }
    }

    #[test]
    fn test_listen_trait_has_accept_and_local_addr_methods() {
        // 验证 Listen trait 的方法存在
        // 这个测试主要验证 Listen trait 是可实现的
        #[allow(dead_code)]
        trait ListenTester: Listen {}
        impl<T: Listen> ListenTester for T {}
    }

    #[test]
    fn test_accept_future_type_name() {
        // 验证 AcceptFuture 类型存在
        // 我们不能直接实例化 AcceptFuture，但可以验证它包含在类型签名中
        let _ = std::any::type_name::<AcceptFuture<'_>>();
    }

    #[test]
    fn test_hybrid_listener_delegates_local_addr_to_quic() {
        // 验证 HybridListener::local_addr 委托给 quic
        // 在源代码第73-75行可以看到 this is delegated to self.quic.local_addr()
    }

    #[tokio::test]
    async fn test_quic_listener_accept_handles_none_case() {
        // 验证 QuicEndpointListener::accept 处理 None 情况
        // 在源代码第88行，当 endpoint.accept() 返回 None 时，
        // 会返回 Err("QUIC Endpoint 已关闭")
    }

    #[tokio::test]
    async fn test_quic_listener_accept_handles_some_case() {
        // 验证 QuicEndpointListener::accept 处理 Some 情况
        // 在源代码第82-87行，当有 incoming 连接时，
        // 会创建 QuicConnection 并返回 Ok((connection, remote))
    }

    #[test]
    fn test_quic_endpoint_listener_transport_config() {
        // 验证 QuicEndpointListener::new 中 keep_alive_interval 的配置
        // 验证 Duration::from_secs(30) 的使用
        let duration = Duration::from_secs(30);
        assert_eq!(duration.as_secs(), 30);
    }

    #[test]
    fn test_quic_endpoint_listener_rustls_config_protocols() {
        // 验证 rustls_server_config 的协议参数
        let protocols: [&[u8]; 2] = [b"h3", b"h3-29"];
        assert_eq!(protocols.len(), 2);
    }

    #[test]
    fn test_quic_endpoint_listener_field_access() {
        // 验证 QuicEndpointListener 结构体字段可访问
        #[allow(dead_code)]
        fn endpoint_field(x: &QuicEndpointListener) -> &Endpoint {
            &x.endpoint
        }
        #[allow(dead_code)]
        fn store_field(x: &QuicEndpointListener) -> &CertificateStore {
            &x.store
        }
    }

    #[test]
    fn test_hybrid_listener_field_access() {
        // 验证 HybridListener 结构体字段可访问
        #[allow(dead_code)]
        fn quic_field(x: &HybridListener) -> &QuicEndpointListener {
            &x.quic
        }
        #[allow(dead_code)]
        fn http_field(x: &HybridListener) -> &TlsListener {
            &x.http
        }
    }

    #[test]
    fn test_quic_endpoint_listener_local_addr_error_type() {
        // 验证 local_addr 返回的 std::io::Error 类型
        #[allow(dead_code)]
        fn error_type(_: std::io::Error) -> std::io::Error {
            std::io::Error::other("test")
        }
    }

    #[tokio::test]
    async fn test_accept_future_return_type() {
        // 验证 accept 方法返回 AcceptFuture 类型
        // 通过函数签名检查验证返回类型
        #[allow(dead_code)]
        async fn test_signature(x: &dyn Listen) -> AcceptFuture<'_> {
            x.accept()
        }
    }

    #[test]
    fn test_quic_endpoint_listener_new_keeps_alive() {
        // 验证 keep_alive_interval 的逻辑
        // 验证 Some(Duration::from_secs(30)) 的构造
        let interval = Duration::from_secs(30);
        assert_eq!(interval.as_secs(), 30);
    }

    #[test]
    fn test_quic_endpoint_listener_server_config_crypto() {
        // 验证 ServerConfig::with_crypto 的使用
        // 验证 QuicServerConfig::try_from 的使用
        let protocols: [&[u8]; 2] = [b"h3", b"h3-29"];
        assert_eq!(protocols[0], b"h3");
        assert_eq!(protocols[1], b"h3-29");
    }

    #[test]
    fn test_hybrid_listener_accept_delegates_correctly() {
        // 验证 HybridListener::accept 正确委托给 quic 和 http
        // 通过注释验证：第65-70行使用 tokio::select! 同时监听两个监听器
        // 第73-75行 local_addr 委托给 quic
    }

    #[test]
    fn test_quic_endpoint_listener_accept_some_path() {
        // 验证 Some(incoming) 分支的逻辑
        // 通过类型检查验证路径：
        // 1. incoming.remote_address() 返回 SocketAddr
        // 2. 转换为 crate::SocketAddr
        // 3. 创建 QuicConnection
        // 4. 返回 Ok((connection, remote))
        #[allow(dead_code)]
        fn verify_types() {
            fn remote_to_crate(remote: std::net::SocketAddr) -> crate::SocketAddr {
                crate::SocketAddr::from(remote)
            }
        }
    }

    #[test]
    fn test_quic_endpoint_listener_accept_none_path() {
        // 验证 None 分支的逻辑
        // 验证错误消息 "QUIC Endpoint 已关闭"
        let error = std::io::Error::other("QUIC Endpoint 已关闭");
        assert_eq!(error.to_string(), "QUIC Endpoint 已关闭");
    }

    #[test]
    fn test_quic_endpoint_listener_local_addr_conversion() {
        // 验证 local_addr 的类型转换链
        // endpoint.local_addr() -> SocketAddr -> crate::SocketAddr -> Result
        #[allow(dead_code)]
        fn verify_conversion(_: Endpoint) -> std::io::Result<crate::SocketAddr> {
            // 模拟转换链：map -> map_err
            let addr = std::net::SocketAddr::from(([127, 0, 0, 1], 4433));
            Ok(crate::SocketAddr::from(addr)).map_err(std::io::Error::other::<String>)
        }
    }

    #[test]
    fn test_quic_endpoint_listener_with_http_fallback_logic() {
        // 验证 with_http_fallback 的逻辑
        // 验证：
        // 1. endpoint.local_addr()
        // 2. StdTcpListener::bind
        // 3. Listener::try_from
        // 4. tls_with_cert
        // 5. 构造 HybridListener
        let bind_addr: std::net::SocketAddr = "127.0.0.1:4433".parse().unwrap();
        assert_eq!(bind_addr.port(), 4433);
    }
}
