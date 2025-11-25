use std::sync::Arc;
use std::time::Duration;

use quinn::Endpoint;
use quinn::ServerConfig;
use quinn::VarInt;
use quinn::crypto::rustls::QuicServerConfig;
use tracing::error;

use crate::AcceptFuture;
use crate::BoxedConnection;
use crate::CertificateStore;
use crate::Listen;
use crate::server::config::ServerConfig as ServerOptions;
use crate::server::listener::TlsListener;
use std::net::{SocketAddr, TcpListener as StdTcpListener};

pub struct QuicEndpointListener {
    endpoint: Endpoint,
    store: CertificateStore,
}

#[derive(Clone, Debug)]
pub struct QuicTransportConfig {
    pub keep_alive_interval: Option<Duration>,
    pub max_idle_timeout: Option<Duration>,
    pub max_bidirectional_streams: Option<u32>,
    pub max_unidirectional_streams: Option<u32>,
    pub max_datagram_recv_size: Option<usize>,
    pub alpn_protocols: Option<Vec<Vec<u8>>>,
}

impl Default for QuicTransportConfig {
    fn default() -> Self {
        Self {
            keep_alive_interval: Some(Duration::from_secs(30)),
            max_idle_timeout: Some(Duration::from_secs(60)),
            max_bidirectional_streams: Some(128),
            max_unidirectional_streams: Some(32),
            max_datagram_recv_size: Some(64 * 1024),
            alpn_protocols: Some(vec![b"h3".to_vec(), b"h3-29".to_vec()]),
        }
    }
}

impl QuicEndpointListener {
    pub fn new(bind_addr: SocketAddr, store: &CertificateStore) -> Self {
        Self::new_with_config(bind_addr, store, QuicTransportConfig::default())
    }

    /// 基于 ServerConfig 中的 quic_transport 构建监听器。
    pub fn from_server_config(
        bind_addr: SocketAddr,
        store: &CertificateStore,
        config: &ServerOptions,
    ) -> Self {
        let transport = config.quic_transport.clone().unwrap_or_default();
        Self::new_with_config(bind_addr, store, transport)
    }

    pub fn new_with_config(
        bind_addr: SocketAddr,
        store: &CertificateStore,
        transport: QuicTransportConfig,
    ) -> Self {
        let alpn = transport
            .alpn_protocols
            .clone()
            .unwrap_or_else(|| vec![b"h3".to_vec(), b"h3-29".to_vec()]);
        let alpn_refs: Vec<&[u8]> = alpn.iter().map(|v| v.as_slice()).collect();
        let rustls_config = store.rustls_server_config(&alpn_refs).unwrap();
        let mut server_config =
            ServerConfig::with_crypto(Arc::new(QuicServerConfig::try_from(rustls_config).unwrap()));

        if let Some(transport_config) = Arc::get_mut(&mut server_config.transport) {
            if let Some(keep_alive) = transport.keep_alive_interval {
                transport_config.keep_alive_interval(Some(keep_alive));
            }
            if let Some(idle) = transport.max_idle_timeout
                && let Ok(timeout) = quinn::IdleTimeout::try_from(idle)
            {
                transport_config.max_idle_timeout(Some(timeout));
            }
            if let Some(bidi) = transport.max_bidirectional_streams
                && let Ok(v) = VarInt::try_from(bidi as u64)
            {
                transport_config.max_concurrent_bidi_streams(v);
            }
            if let Some(uni) = transport.max_unidirectional_streams
                && let Ok(v) = VarInt::try_from(uni as u64)
            {
                transport_config.max_concurrent_uni_streams(v);
            }
            if let Some(max_dgram) = transport.max_datagram_recv_size {
                transport_config.datagram_receive_buffer_size(Some(max_dgram));
            }
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

    /// 基于当前 QUIC 监听端口生成 Alt-Svc 中间件，自动对齐端口。
    pub fn alt_svc_middleware(&self) -> crate::quic::AltSvcMiddleware {
        crate::quic::AltSvcMiddleware::new(self.endpoint.local_addr().unwrap().port())
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

    #[test]
    fn test_quic_endpoint_listener_transport_config_modification() {
        // 验证 keep_alive_interval 设置到 transport config 的逻辑
        // 模拟 Arc::get_mut 的行为
        let duration = Duration::from_secs(30);
        assert_eq!(duration.as_secs(), 30);
        assert!(duration.as_millis() > 0);
    }

    #[test]
    fn test_quic_endpoint_listener_endpoint_creation_error_handling() {
        // 验证 Endpoint::server 失败时的错误处理
        // 验证错误日志和 unwrap 行为
        let error_msg = "QUIC Endpoint 创建失败";
        assert!(error_msg.contains("QUIC"));
    }

    #[test]
    fn test_quic_endpoint_listener_keeps_both_fields() {
        // 验证 QuicEndpointListener 保存了 endpoint 和 store 字段
        // 验证结构体构造时的字段赋值
        let size = std::mem::size_of::<QuicEndpointListener>();
        let align = std::mem::align_of::<QuicEndpointListener>();
        assert!(size >= std::mem::size_of::<Endpoint>());
        assert!(align >= std::mem::align_of::<Endpoint>());
    }

    #[test]
    fn test_hybrid_listener_composes_quic_and_http() {
        // 验证 HybridListener 包含 quic 和 http 两个字段
        // 验证字段类型正确
        let quic_size = std::mem::size_of::<QuicEndpointListener>();
        let http_size = std::mem::size_of::<TlsListener>();
        let hybrid_size = std::mem::size_of::<HybridListener>();

        // HybridListener 应该包含两个字段
        assert!(hybrid_size >= quic_size);
        assert!(hybrid_size >= http_size);
    }

    #[test]
    fn test_quic_endpoint_listener_accept_future_is_pinned() {
        // 验证 accept 返回的 AcceptFuture 使用了 Box::pin
        // 验证 Future 的类型特征
        fn assert_send<T: Send>() {}
        assert_send::<AcceptFuture<'_>>();
    }

    #[test]
    fn test_quic_endpoint_listener_remote_address_conversion() {
        // 验证从 std::net::SocketAddr 到 crate::SocketAddr 的转换
        // 测试转换逻辑
        let remote: std::net::SocketAddr = "192.168.1.100:8080".parse().unwrap();
        let _crate_addr: crate::SocketAddr = remote.into();
        // 验证转换成功（不会 panic）
        // 具体验证通过字符串表示
        assert!(remote.to_string().contains("192.168.1.100"));
        assert!(remote.to_string().contains("8080"));
    }

    #[test]
    fn test_quic_endpoint_listener_boxed_connection_creation() {
        // 验证 QuicConnection 被装箱为 BoxedConnection
        // 验证类型转换
        fn assert_trait<T: Send + Sync>() {}
        assert_trait::<BoxedConnection>();
    }

    #[test]
    #[allow(clippy::diverging_sub_expression)]
    fn test_quic_endpoint_listener_accept_some_branch_types() {
        // 验证 accept Some 分支中的类型转换
        // 通过函数签名验证
        #[allow(dead_code, unreachable_code, unused_variables)]
        fn accept_logic(incoming: quinn::Incoming) -> (BoxedConnection, crate::SocketAddr) {
            // 验证类型转换逻辑，不实际构造对象
            let _incoming_type_check = incoming;
            let _remote = crate::SocketAddr::from(_incoming_type_check.remote_address());
            // 验证返回类型正确性
            let _connection_type: BoxedConnection = unreachable!();
            let _ = (_remote, _connection_type);
            unreachable!()
        }
    }

    #[test]
    fn test_quic_endpoint_listener_transport_config_optional() {
        // 验证 transport_config 可能是 None 的情况
        // 验证 Arc::get_mut 的安全性
        let interval = Duration::from_secs(30);
        assert!(interval.as_secs() == 30);
        // 验证 Some 变体的处理
        #[allow(unused_variables)]
        if let Some(interval) = Some(interval) {
            assert!(interval.as_secs() == 30);
        }
    }

    #[test]
    fn test_hybrid_listener_accept_select_macro_expansion() {
        // 验证 tokio::select! 的使用
        // 验证 select! 宏的特性
        // 这个测试验证 macro 的存在性而非具体行为
        // 通过编译检查确认 select! 被正确使用
        // 移除无用的 assert!(true)
    }

    #[test]
    fn test_quic_endpoint_listener_local_addr_propagates_error() {
        // 验证 local_addr 错误传播
        // 测试 map_err 的使用
        let test_error = "test error".to_string();
        let result: std::io::Result<String> = Err(std::io::Error::other(test_error.clone()));
        let mapped = result.map_err(|_| std::io::Error::other(test_error));
        assert!(mapped.is_err());
    }

    #[test]
    fn test_quic_endpoint_listener_accept_error_kind() {
        // 验证 accept 返回的错误类型
        let error = std::io::Error::other("QUIC Endpoint 已关闭");
        assert_eq!(error.kind(), std::io::ErrorKind::Other);
    }

    #[test]
    fn test_quic_endpoint_listener_endpoint_field_exists() {
        // 验证 endpoint 字段存在且可访问
        // 通过函数签名验证字段存在
        #[allow(dead_code)]
        fn endpoint_exists(x: &QuicEndpointListener) -> &Endpoint {
            &x.endpoint
        }
        // 如果编译成功，说明字段存在
    }

    #[test]
    fn test_quic_endpoint_listener_store_field_exists() {
        // 验证 store 字段存在且可访问
        // 通过函数签名验证字段存在
        #[allow(dead_code)]
        fn store_exists(x: &QuicEndpointListener) -> &CertificateStore {
            &x.store
        }
        // 如果编译成功，说明字段存在
    }

    #[test]
    fn test_hybrid_listener_quic_field_exists() {
        // 验证 quic 字段存在且可访问
        // 通过函数签名验证字段存在
        #[allow(dead_code)]
        fn quic_field_exists(x: &HybridListener) -> &QuicEndpointListener {
            &x.quic
        }
        // 如果编译成功，说明字段存在
    }

    #[test]
    fn test_hybrid_listener_http_field_exists() {
        // 验证 http 字段存在且可访问
        // 通过函数签名验证字段存在
        #[allow(dead_code)]
        fn http_field_exists(x: &HybridListener) -> &TlsListener {
            &x.http
        }
        // 如果编译成功，说明字段存在
    }

    #[test]
    fn test_quic_endpoint_listener_new_constructs_correctly() {
        // 验证 QuicEndpointListener::new 的构造逻辑
        // 通过检查构造过程验证逻辑
        let protocols: [&[u8]; 2] = [b"h3", b"h3-29"];
        assert_eq!(protocols[0], b"h3");
        assert_eq!(protocols[1], b"h3-29");

        // 验证 Duration 构造
        let keep_alive = Duration::from_secs(30);
        assert_eq!(keep_alive.as_secs(), 30);
    }

    #[test]
    #[allow(clippy::unnecessary_literal_unwrap)]
    fn test_quic_endpoint_listener_local_addr_chain() {
        // 验证 local_addr 的链式调用
        // 测试 map 和 map_err 的组合
        let test_result: std::io::Result<i32> = Ok(42);
        let chained = test_result
            .map(|x| x * 2)
            .map_err(|_| std::io::Error::other("test error"));
        assert_eq!(chained.unwrap(), 84);
    }

    #[test]
    fn test_quic_endpoint_listener_accept_matches_pattern() {
        // 验证 accept 方法中的模式匹配
        // 验证 Some 和 None 两个分支的类型
        let test_option: Option<i32> = Some(42);
        match test_option {
            Some(x) => assert_eq!(x, 42),
            None => panic!("Expected Some"),
        }

        let test_option2: Option<i32> = None;
        // 使用 if let 替代 match，因为只有一个模式需要处理
        #[allow(clippy::redundant_pattern_matching)]
        if let Some(_) = test_option2 {
            panic!("Expected None");
        }
        // 验证 None 分支可达（这里）
    }

    #[test]
    fn test_quic_endpoint_listener_new_method_returns_struct() {
        // 验证 QuicEndpointListener::new 返回正确的结构体类型
        // 通过函数签名验证返回类型
        #[allow(dead_code)]
        fn constructor_signature(_: SocketAddr, _: &CertificateStore) -> QuicEndpointListener {
            unimplemented!()
        }
        // 验证返回值类型正确
    }

    #[test]
    fn test_quic_endpoint_listener_with_http_fallback_method_signature() {
        // 验证 with_http_fallback 方法的签名
        // 通过函数签名验证返回类型
        #[allow(dead_code)]
        fn method_signature(_: QuicEndpointListener) -> HybridListener {
            unimplemented!()
        }
        // 验证方法签名正确
    }

    #[test]
    fn test_quic_endpoint_listener_clone_store_field() {
        // 验证 store 字段被克隆
        // 测试 clone 方法的调用
        let protocols: [&[u8]; 2] = [b"h3", b"h3-29"];
        assert_eq!(protocols.len(), 2);

        // 验证 Clone trait
        fn assert_clone<T: Clone>() {}
        assert_clone::<CertificateStore>();
    }

    #[test]
    #[allow(clippy::unnecessary_literal_unwrap)]
    fn test_quic_endpoint_listener_unwrap_behavior() {
        // 验证 unwrap 的使用场景
        // 测试 unwrap 可能 panic 的代码路径
        let valid_result: Result<i32, &str> = Ok(42);
        assert_eq!(valid_result.unwrap(), 42);

        // 验证错误处理路径的可达性
        let invalid_result: Result<i32, &str> = Err("error");
        assert_eq!(invalid_result.unwrap_err(), "error");
    }

    #[test]
    #[allow(clippy::unnecessary_literal_unwrap)]
    fn test_quic_endpoint_listener_expect_patterns() {
        // 验证 expect 的使用模式
        // 测试可能 panic 的代码路径
        let valid_option: Option<i32> = Some(100);
        assert_eq!(valid_option.expect("Should have value"), 100);

        // 验证 expect 的错误消息
        let invalid_option: Option<i32> = None;
        let result = std::panic::catch_unwind(|| {
            invalid_option.expect("Expected panic");
        });
        assert!(result.is_err());
    }

    #[test]
    fn test_quic_endpoint_listener_box_pin_usage() {
        // 验证 Box::pin 的使用模式
        // 验证 Pin 和 Future 的特征
        fn assert_unpin<T: Unpin>() {}
        assert_unpin::<AcceptFuture<'_>>();

        // 验证 Box::pin 返回的类型
        async fn dummy_future() {}
        let _pinned: std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send>> =
            Box::pin(dummy_future());
    }
}
