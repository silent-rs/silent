pub mod connection;
pub mod connection_service;
pub mod listener;
pub mod net_server;
pub mod protocol;
#[cfg(feature = "quic")]
pub mod quic;
pub mod route_connection;
pub mod stream;
#[cfg(feature = "tls")]
pub mod tls;
#[cfg(feature = "tls")]
pub use tls::{CertificateStore, CertificateStoreBuilder};
mod config;
#[cfg(feature = "metrics")]
pub mod metrics;

pub use config::{ConnectionLimits, ServerConfig};
pub use route_connection::RouteConnectionService;

use crate::core::socket_addr::SocketAddr as CoreSocketAddr;
use config::set_global_server_config;
pub use connection_service::{BoxError, ConnectionFuture, ConnectionService};
use listener::{Listen, ListenersBuilder};
pub use net_server::RateLimiterConfig;
use std::net::SocketAddr;
#[cfg(not(target_os = "windows"))]
use std::path::Path;
use std::time::Duration;
type ListenCallback = Box<dyn Fn(&[CoreSocketAddr]) + Send + Sync>;

pub struct Server {
    listeners_builder: ListenersBuilder,
    shutdown_callback: Option<Box<dyn Fn() + Send + Sync>>,
    listen_callback: Option<ListenCallback>,
    rate_limiter_config: Option<RateLimiterConfig>,
    graceful_shutdown_duration: Option<Duration>,
    config: ServerConfig,
}

impl Default for Server {
    fn default() -> Self {
        Self::new()
    }
}

impl Server {
    pub fn new() -> Self {
        Self {
            listeners_builder: ListenersBuilder::new(),
            shutdown_callback: None,
            listen_callback: None,
            rate_limiter_config: None,
            graceful_shutdown_duration: None,
            config: ServerConfig::default(),
        }
    }

    #[inline]
    pub fn bind(mut self, addr: SocketAddr) -> Self {
        self.listeners_builder
            .bind(addr)
            .expect("Failed to bind to address");
        self
    }

    #[cfg(not(target_os = "windows"))]
    #[inline]
    pub fn bind_unix<P: AsRef<Path>>(mut self, path: P) -> Self {
        self.listeners_builder
            .bind_unix(&path)
            .expect("Failed to bind to Unix socket");
        self
    }

    #[inline]
    pub fn listen<T: Listen + Send + Sync + 'static>(mut self, listener: T) -> Self {
        self.listeners_builder.add_listener(Box::new(listener));
        self
    }

    pub fn set_shutdown_callback<F>(mut self, callback: F) -> Self
    where
        F: Fn() + Send + Sync + 'static,
    {
        self.shutdown_callback = Some(Box::new(callback));
        self
    }

    pub fn on_listen<F>(mut self, callback: F) -> Self
    where
        F: Fn(&[CoreSocketAddr]) + Send + Sync + 'static,
    {
        self.listen_callback = Some(Box::new(callback));
        self
    }

    /// 配置连接限流器（令牌桶算法）。
    ///
    /// 限流器用于控制连接接受速率，防止服务器过载。
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use silent::{Server, RateLimiterConfig};
    /// use std::time::Duration;
    ///
    /// let config = RateLimiterConfig {
    ///     capacity: 10,
    ///     refill_every: Duration::from_millis(10),
    ///     max_wait: Duration::from_secs(2),
    /// };
    ///
    /// let server = Server::new()
    ///     .bind("127.0.0.1:8080".parse().unwrap())
    ///     .with_rate_limiter(config);
    /// ```
    pub fn with_rate_limiter(mut self, config: RateLimiterConfig) -> Self {
        self.rate_limiter_config = Some(config);
        self
    }

    /// 配置优雅关停等待时间。
    ///
    /// 当收到关停信号（Ctrl-C 或 SIGTERM）时：
    /// 1. 停止接受新连接
    /// 2. 等待活动连接在 `graceful_wait` 时间内完成
    /// 3. 超时后强制取消剩余连接
    ///
    /// 默认值为 0，表示立即强制关停。
    ///
    /// # Examples
    ///
    /// 等待最多 30 秒让连接优雅完成：
    ///
    /// ```no_run
    /// use silent::Server;
    /// use std::time::Duration;
    ///
    /// let server = Server::new()
    ///     .bind("127.0.0.1:8080".parse().unwrap())
    ///     .with_shutdown(Duration::from_secs(30));
    /// ```
    pub fn with_shutdown(mut self, graceful_wait: Duration) -> Self {
        self.graceful_shutdown_duration = Some(graceful_wait);
        self
    }

    /// 配置统一入口（连接限速、超时、请求体大小等）。
    #[inline]
    pub fn with_config(mut self, config: ServerConfig) -> Self {
        self.config = config;
        self
    }

    /// 设置连接级别超时/请求体大小限制。
    #[inline]
    pub fn with_connection_limits(mut self, limits: ConnectionLimits) -> Self {
        self.config.connection_limits = limits;
        self
    }

    pub async fn serve<H>(self, handler: H)
    where
        H: ConnectionService + Clone,
    {
        // 将网络层职责完全委托给通用 NetServer
        // 注意: 调度器会在 NetServer::serve_connection_loop 中启动
        set_global_server_config(self.config.clone());
        let mut net_server = net_server::NetServer::from_parts(
            self.listeners_builder,
            self.shutdown_callback,
            self.listen_callback,
            self.config.clone(),
        );

        // 应用限流配置
        if let Some(config) = self.rate_limiter_config {
            net_server = net_server.with_rate_limiter(config);
        }

        // 应用优雅关停配置
        if let Some(duration) = self.graceful_shutdown_duration {
            net_server = net_server.with_shutdown(duration);
        }

        net_server.serve(handler).await
    }

    pub fn run<H>(self, handler: H)
    where
        H: ConnectionService + Clone,
    {
        // 将网络层职责完全委托给通用 NetServer
        // 注意: 调度器会在 NetServer::serve_connection_loop 中启动
        set_global_server_config(self.config.clone());
        let mut net_server = net_server::NetServer::from_parts(
            self.listeners_builder,
            self.shutdown_callback,
            self.listen_callback,
            self.config.clone(),
        );

        // 应用限流配置
        if let Some(config) = self.rate_limiter_config {
            net_server = net_server.with_rate_limiter(config);
        }

        // 应用优雅关停配置
        if let Some(duration) = self.graceful_shutdown_duration {
            net_server = net_server.with_shutdown(duration);
        }

        net_server.run(handler)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    // ==================== Server 构造函数测试 ====================

    #[test]
    fn test_server_new() {
        let server = Server::new();
        // 验证服务器可以创建
        let _ = server.listeners_builder;
    }

    #[test]
    fn test_server_default() {
        let server = Server::default();
        // 验证服务器可以通过 Default trait 创建
        let _ = server.listeners_builder;
    }

    // ==================== Server 配置方法测试 ====================

    #[tokio::test]
    async fn test_server_bind() {
        let server = Server::new().bind("127.0.0.1:0".parse().unwrap());
        // 验证 bind 方法不会 panic
        let _ = server.listeners_builder;
    }

    #[tokio::test]
    async fn test_server_bind_multiple() {
        let server = Server::new()
            .bind("127.0.0.1:0".parse().unwrap())
            .bind("127.0.0.1:0".parse().unwrap());
        // 验证可以绑定多个地址
        let _ = server.listeners_builder;
    }

    #[cfg(not(target_os = "windows"))]
    #[test]
    fn test_server_bind_unix_type_check() {
        // 只测试类型约束，不实际调用 bind_unix
        // 因为在 Tokio runtime 中不能使用阻塞的 Unix socket
        use std::path::Path;

        // 验证 bind_unix 方法存在且接受 Path 参数
        fn assert_bind_unix<T: AsRef<Path>>() {}

        // PathBuf 实现了 AsRef<Path>
        assert_bind_unix::<std::path::PathBuf>();
        assert_bind_unix::<&str>();
    }

    #[test]
    fn test_server_listen() {
        // listen 方法需要实现了 Listen trait 的类型
        // 这里只测试类型约束，不实际运行
        use crate::server::listener::Listen;

        fn assert_listen<T: Listen + Send + Sync + 'static>() {}

        // 验证 Listener 实现了 Listen trait
        assert_listen::<crate::server::listener::Listener>();
    }

    #[test]
    fn test_server_set_shutdown_callback() {
        let callback_called = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let callback_called_clone = callback_called.clone();

        let _server = Server::new().set_shutdown_callback(move || {
            callback_called_clone.store(true, std::sync::atomic::Ordering::SeqCst);
        });

        // 验证回调已设置（通过 server 的字段）
        // 实际调用需要在关停时触发
    }

    #[test]
    fn test_server_on_listen() {
        let _server = Server::new().on_listen(|addrs| {
            // 验证可以访问地址列表
            assert!(!addrs.is_empty() || addrs.is_empty()); // 总是为真，仅验证编译
        });

        // 验证回调已设置
    }

    #[test]
    fn test_server_with_rate_limiter() {
        let config = RateLimiterConfig {
            capacity: 100,
            refill_every: Duration::from_millis(100),
            max_wait: Duration::from_secs(5),
        };

        let server = Server::new().with_rate_limiter(config);
        // 验证限流配置已设置
        assert!(server.rate_limiter_config.is_some());
    }

    #[test]
    fn test_server_with_shutdown() {
        let duration = Duration::from_secs(30);
        let server = Server::new().with_shutdown(duration);
        // 验证优雅关停配置已设置
        assert_eq!(server.graceful_shutdown_duration, Some(duration));
    }

    #[test]
    fn test_server_with_config() {
        let config = ServerConfig::default();
        let server = Server::new().with_config(config.clone());
        // 验证配置已设置
        assert_eq!(server.config.connection_limits.handler_timeout, None);
        assert_eq!(server.config.connection_limits.max_body_size, None);
    }

    #[test]
    fn test_server_with_connection_limits() {
        let limits = ConnectionLimits {
            handler_timeout: Some(Duration::from_secs(30)),
            max_body_size: Some(1024 * 1024),
            h3_read_timeout: None,
            max_webtransport_frame_size: None,
            webtransport_read_timeout: None,
            max_webtransport_sessions: None,
            webtransport_datagram_max_size: None,
            webtransport_datagram_rate: None,
            webtransport_datagram_drop_metric: false,
        };

        let server = Server::new().with_connection_limits(limits);
        // 验证连接限制已设置
        assert_eq!(
            server.config.connection_limits.handler_timeout,
            Some(Duration::from_secs(30))
        );
        assert_eq!(
            server.config.connection_limits.max_body_size,
            Some(1024 * 1024)
        );
    }

    // ==================== Server 构建链测试 ====================

    #[tokio::test]
    async fn test_server_builder_chain() {
        let server = Server::new()
            .bind("127.0.0.1:0".parse().unwrap())
            .on_listen(|_addrs| {})
            .with_rate_limiter(RateLimiterConfig {
                capacity: 1,
                refill_every: Duration::from_millis(10),
                max_wait: Duration::from_millis(10),
            })
            .with_shutdown(Duration::from_millis(1));

        // 验证所有配置都已应用
        assert!(server.rate_limiter_config.is_some());
        assert!(server.graceful_shutdown_duration.is_some());
        assert!(server.listen_callback.is_some());
    }

    #[tokio::test]
    async fn test_server_full_builder_chain() {
        let limits = ConnectionLimits {
            handler_timeout: Some(Duration::from_secs(60)),
            max_body_size: Some(512 * 1024),
            h3_read_timeout: None,
            max_webtransport_frame_size: None,
            webtransport_read_timeout: None,
            max_webtransport_sessions: None,
            webtransport_datagram_max_size: None,
            webtransport_datagram_rate: None,
            webtransport_datagram_drop_metric: false,
        };

        let server = Server::new()
            .bind("127.0.0.1:0".parse().unwrap())
            .bind("127.0.0.1:0".parse().unwrap())
            .set_shutdown_callback(|| {})
            .on_listen(|_addrs| {})
            .with_rate_limiter(RateLimiterConfig {
                capacity: 10,
                refill_every: Duration::from_millis(100),
                max_wait: Duration::from_secs(2),
            })
            .with_shutdown(Duration::from_secs(30))
            .with_connection_limits(limits);

        // 验证所有配置都已应用
        assert!(server.shutdown_callback.is_some());
        assert!(server.listen_callback.is_some());
        assert!(server.rate_limiter_config.is_some());
        assert!(server.graceful_shutdown_duration.is_some());
        assert_eq!(
            server.config.connection_limits.handler_timeout,
            Some(Duration::from_secs(60))
        );
    }

    // ==================== ServerConfig 测试 ====================

    #[test]
    fn test_server_config_default() {
        let config = ServerConfig::default();
        // 验证默认配置
        assert_eq!(config.connection_limits.handler_timeout, None);
        assert_eq!(config.connection_limits.max_body_size, None);
        assert_eq!(config.connection_limits.h3_read_timeout, None);
    }

    #[test]
    fn test_server_config_with_limits() {
        let limits = ConnectionLimits {
            handler_timeout: Some(Duration::from_secs(120)),
            max_body_size: Some(2048 * 1024),
            h3_read_timeout: Some(Duration::from_secs(30)),
            max_webtransport_frame_size: None,
            webtransport_read_timeout: None,
            max_webtransport_sessions: None,
            webtransport_datagram_max_size: None,
            webtransport_datagram_rate: None,
            webtransport_datagram_drop_metric: false,
        };

        let config = ServerConfig {
            connection_limits: limits.clone(),
            ..Default::default()
        };

        assert_eq!(
            config.connection_limits.handler_timeout,
            Some(Duration::from_secs(120))
        );
        assert_eq!(config.connection_limits.max_body_size, Some(2048 * 1024));
        assert_eq!(
            config.connection_limits.h3_read_timeout,
            Some(Duration::from_secs(30))
        );
    }

    #[test]
    fn test_server_config_clone() {
        let config = ServerConfig::default();
        let config_clone = config.clone();

        // 验证配置可以克隆
        assert_eq!(
            config.connection_limits.handler_timeout,
            config_clone.connection_limits.handler_timeout
        );
        assert_eq!(
            config.connection_limits.max_body_size,
            config_clone.connection_limits.max_body_size
        );
    }

    // ==================== RateLimiterConfig 测试 ====================

    #[test]
    fn test_rate_limiter_config() {
        let config = RateLimiterConfig {
            capacity: 1000,
            refill_every: Duration::from_millis(50),
            max_wait: Duration::from_secs(10),
        };

        assert_eq!(config.capacity, 1000);
        assert_eq!(config.refill_every, Duration::from_millis(50));
        assert_eq!(config.max_wait, Duration::from_secs(10));
    }

    #[test]
    fn test_rate_limiter_config_copy() {
        let config = RateLimiterConfig {
            capacity: 500,
            refill_every: Duration::from_millis(100),
            max_wait: Duration::from_secs(5),
        };

        let config_copy = config;

        assert_eq!(config.capacity, config_copy.capacity);
        assert_eq!(config.refill_every, config_copy.refill_every);
        assert_eq!(config.max_wait, config_copy.max_wait);
    }

    // ==================== ConnectionLimits 测试 ====================

    #[test]
    fn test_connection_limits_default() {
        let limits = ConnectionLimits::default();

        assert_eq!(limits.handler_timeout, None);
        assert_eq!(limits.max_body_size, None);
        assert_eq!(limits.h3_read_timeout, None);
    }

    #[test]
    fn test_connection_limits_custom() {
        let limits = ConnectionLimits {
            handler_timeout: Some(Duration::from_secs(30)),
            max_body_size: Some(1024 * 1024),
            h3_read_timeout: Some(Duration::from_secs(20)),
            max_webtransport_frame_size: Some(4096),
            webtransport_read_timeout: None,
            max_webtransport_sessions: Some(10),
            webtransport_datagram_max_size: None,
            webtransport_datagram_rate: None,
            webtransport_datagram_drop_metric: true,
        };

        assert_eq!(limits.handler_timeout, Some(Duration::from_secs(30)));
        assert_eq!(limits.max_body_size, Some(1024 * 1024));
        assert_eq!(limits.h3_read_timeout, Some(Duration::from_secs(20)));
        assert_eq!(limits.max_webtransport_frame_size, Some(4096));
        assert_eq!(limits.max_webtransport_sessions, Some(10));
        assert!(limits.webtransport_datagram_drop_metric);
    }

    #[test]
    fn test_connection_limits_no_timeout() {
        let limits = ConnectionLimits {
            handler_timeout: None,
            max_body_size: Some(512 * 1024),
            h3_read_timeout: None,
            max_webtransport_frame_size: None,
            webtransport_read_timeout: None,
            max_webtransport_sessions: None,
            webtransport_datagram_max_size: None,
            webtransport_datagram_rate: None,
            webtransport_datagram_drop_metric: false,
        };

        assert_eq!(limits.handler_timeout, None);
        assert_eq!(limits.max_body_size, Some(512 * 1024));
    }

    // ==================== Duration 相关测试 ====================

    #[test]
    fn test_duration_values() {
        // 测试不同的 Duration 值
        let millis = Duration::from_millis(100);
        let secs = Duration::from_secs(1);
        let zero = Duration::ZERO;

        assert_eq!(millis.as_millis(), 100);
        assert_eq!(secs.as_secs(), 1);
        assert_eq!(zero.as_secs(), 0);
    }

    // ==================== SocketAddr 测试 ====================

    #[test]
    fn test_socket_addr_parsing() {
        // 测试地址解析
        let addr1: SocketAddr = "127.0.0.1:8080".parse().unwrap();
        let addr2: SocketAddr = "[::1]:8080".parse().unwrap();

        assert_eq!(addr1.port(), 8080);
        assert_eq!(addr2.port(), 8080);
        assert!(addr1.is_ipv4());
        assert!(addr2.is_ipv6());
    }

    #[test]
    fn test_socket_addr_any_port() {
        // 测试使用端口 0（让系统分配）
        let addr: SocketAddr = "127.0.0.1:0".parse().unwrap();
        assert_eq!(addr.port(), 0);
    }
}
