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

pub use route_connection::RouteConnectionService;

use crate::core::socket_addr::SocketAddr as CoreSocketAddr;
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
        }
    }

    #[inline]
    pub fn bind(mut self, addr: SocketAddr) -> Result<Self, std::io::Error> {
        self.listeners_builder.bind(addr)?;
        Ok(self)
    }

    #[cfg(not(target_os = "windows"))]
    #[inline]
    pub fn bind_unix<P: AsRef<Path>>(mut self, path: P) -> Result<Self, std::io::Error> {
        self.listeners_builder.bind_unix(path)?;
        Ok(self)
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
    ///     .bind("127.0.0.1:8080".parse().unwrap()).unwrap()
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
    ///     .bind("127.0.0.1:8080".parse().unwrap()).unwrap()
    ///     .with_shutdown(Duration::from_secs(30));
    /// ```
    pub fn with_shutdown(mut self, graceful_wait: Duration) -> Self {
        self.graceful_shutdown_duration = Some(graceful_wait);
        self
    }

    pub async fn serve<H>(self, handler: H)
    where
        H: ConnectionService + Clone,
    {
        // 将网络层职责完全委托给通用 NetServer
        // 注意: 调度器会在 NetServer::serve_connection_loop 中启动
        let mut net_server = net_server::NetServer::from_parts(
            self.listeners_builder,
            self.shutdown_callback,
            self.listen_callback,
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
        let mut net_server = net_server::NetServer::from_parts(
            self.listeners_builder,
            self.shutdown_callback,
            self.listen_callback,
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

    #[tokio::test]
    async fn test_server_builder_chain() {
        let _ = Server::new()
            .bind("127.0.0.1:0".parse().unwrap()).unwrap()
            .on_listen(|_addrs| {})
            .with_rate_limiter(RateLimiterConfig {
                capacity: 1,
                refill_every: Duration::from_millis(10),
                max_wait: Duration::from_millis(10),
            })
            .with_shutdown(Duration::from_millis(1));
    }
}
