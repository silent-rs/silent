use super::ConnectionService;
use super::listener::{Listen, ListenersBuilder};
use crate::core::socket_addr::SocketAddr as CoreSocketAddr;
use std::io;
use std::net::SocketAddr;
#[cfg(not(target_os = "windows"))]
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;
use tokio::signal;
use tokio::sync::Semaphore;
use tokio::task::JoinSet;

type ListenCallback = Box<dyn Fn(&[CoreSocketAddr]) + Send + Sync>;

/// 限流器配置（令牌桶算法）。
///
/// # 参数说明
///
/// - `capacity`: 令牌桶容量（允许的最大突发连接数）
/// - `refill_every`: 令牌补充间隔（每次补充 1 个令牌）
/// - `max_wait`: 获取令牌的最大等待时间，超时则拒绝连接
///
/// # Examples
///
/// ```
/// use silent::RateLimiterConfig;
/// use std::time::Duration;
///
/// let config = RateLimiterConfig {
///     capacity: 10,
///     refill_every: Duration::from_millis(10),
///     max_wait: Duration::from_secs(2),
/// };
/// ```
#[derive(Clone, Copy, Debug)]
pub struct RateLimiterConfig {
    /// 令牌桶容量（允许的最大突发连接数）
    pub capacity: usize,
    /// 令牌补充间隔（每次补充 1 个令牌）
    pub refill_every: Duration,
    /// 获取令牌的最大等待时间，超时则拒绝连接
    pub max_wait: Duration,
}

/// 与协议无关的通用网络服务器。
///
/// `NetServer` 提供底层网络监听和连接分发能力，支持任意协议的自定义处理逻辑。
/// 它负责：
/// - 监听一个或多个网络地址（TCP/Unix Socket）
/// - 接受新连接并分发给用户提供的 `ConnectionService` 处理器
/// - 可选的连接限流（令牌桶算法）
/// - 优雅关停（等待活动连接完成或超时强制取消）
///
/// # Examples
///
/// 基本的 TCP 回显服务器：
///
/// ```no_run
/// use silent::{NetServer, RateLimiterConfig, BoxedConnection, SocketAddr};
/// use std::time::Duration;
/// use tokio::io::{AsyncReadExt, AsyncWriteExt};
///
/// #[tokio::main]
/// async fn main() {
///     let handler = |mut stream: BoxedConnection, peer: SocketAddr| async move {
///         let mut buf = vec![0u8; 1024];
///         let n = stream.read(&mut buf).await?;
///         stream.write_all(&buf[..n]).await?;
///         Ok::<(), Box<dyn std::error::Error + Send + Sync>>(())
///     };
///
///     let rate_config = RateLimiterConfig {
///         capacity: 100,
///         refill_every: Duration::from_millis(10),
///         max_wait: Duration::from_secs(1),
///     };
///
///     NetServer::new()
///         .bind("127.0.0.1:8080".parse().unwrap())
///         .with_rate_limiter(rate_config)
///         .with_shutdown(Duration::from_secs(30))
///         .serve(handler)
///         .await;
/// }
/// ```
///
/// # 限流
///
/// 使用 [`with_rate_limiter`](Self::with_rate_limiter) 配置令牌桶限流器：
/// - `capacity`: 令牌桶容量（允许的最大突发连接数）
/// - `refill_every`: 补充间隔（每次补充 1 个令牌）
/// - `max_wait`: 获取令牌的最大等待时间
///
/// # 优雅关停
///
/// 使用 [`with_shutdown`](Self::with_shutdown) 配置关停行为：
/// - 收到 Ctrl-C 或 SIGTERM 信号后停止接受新连接
/// - 等待活动连接在指定时间内完成
/// - 超时后强制取消剩余连接
///
/// # 错误处理
///
/// 连接处理器返回的错误会被记录到日志，但不会影响服务器主循环。
/// 服务器会继续接受新连接，除非收到关停信号或遇到严重的监听器错误。
pub struct NetServer {
    listeners_builder: ListenersBuilder,
    shutdown_callback: Option<Box<dyn Fn() + Send + Sync>>,
    listen_callback: Option<ListenCallback>,
    rate_limiter: Option<RateLimiter>,
    shutdown_cfg: ShutdownConfig,
}

impl Default for NetServer {
    fn default() -> Self {
        Self::new()
    }
}

impl NetServer {
    /// 创建一个新的 NetServer 实例。
    ///
    /// 默认配置：
    /// - 无监听器（需要调用 [`bind`](Self::bind) 或 [`listen`](Self::listen) 添加）
    /// - 无限流限制
    /// - 立即强制关停（graceful_wait = 0）
    ///
    /// # Examples
    ///
    /// ```
    /// use silent::NetServer;
    ///
    /// let server = NetServer::new();
    /// ```
    pub fn new() -> Self {
        Self {
            listeners_builder: ListenersBuilder::new(),
            shutdown_callback: None,
            listen_callback: None,
            rate_limiter: None,
            shutdown_cfg: ShutdownConfig::default(),
        }
    }

    pub(crate) fn from_parts(
        listeners_builder: ListenersBuilder,
        shutdown_callback: Option<Box<dyn Fn() + Send + Sync>>,
        listen_callback: Option<ListenCallback>,
    ) -> Self {
        Self {
            listeners_builder,
            shutdown_callback,
            listen_callback,
            rate_limiter: None,
            shutdown_cfg: ShutdownConfig::default(),
        }
    }

    /// 绑定 TCP 监听地址。
    ///
    /// 可以多次调用以监听多个地址。
    ///
    /// # Examples
    ///
    /// ```
    /// use silent::NetServer;
    ///
    /// let server = NetServer::new()
    ///     .bind("127.0.0.1:8080".parse().unwrap())
    ///     .bind("127.0.0.1:8081".parse().unwrap());
    /// ```
    #[inline]
    pub fn bind(mut self, addr: SocketAddr) -> Self {
        self.listeners_builder.bind(addr);
        self
    }

    /// 绑定 Unix Domain Socket 监听路径（仅非 Windows 平台）。
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # #[cfg(not(target_os = "windows"))]
    /// # {
    /// use silent::NetServer;
    ///
    /// let server = NetServer::new()
    ///     .bind_unix("/tmp/my_service.sock");
    /// # }
    /// ```
    #[cfg(not(target_os = "windows"))]
    #[inline]
    pub fn bind_unix<P: AsRef<Path>>(mut self, path: P) -> Self {
        self.listeners_builder.bind_unix(path);
        self
    }

    /// 添加自定义监听器。
    ///
    /// 用于高级场景，允许使用自定义的监听器实现。
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use silent::NetServer;
    /// use tokio::net::TcpListener;
    ///
    /// # #[tokio::main]
    /// # async fn main() {
    /// let custom_listener = TcpListener::bind("127.0.0.1:8080").await.unwrap();
    /// let server = NetServer::new()
    ///     .listen(custom_listener);
    /// # }
    /// ```
    #[inline]
    pub fn listen<T: Listen + Send + Sync + 'static>(mut self, listener: T) -> Self {
        self.listeners_builder.add_listener(Box::new(listener));
        self
    }

    /// 设置监听成功后的回调函数。
    ///
    /// 回调函数会在所有监听器成功绑定后被调用，接收实际监听的地址列表。
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use silent::NetServer;
    ///
    /// let server = NetServer::new()
    ///     .bind("127.0.0.1:0".parse().unwrap())  // 随机端口
    ///     .on_listen(|addrs| {
    ///         println!("Server listening on: {:?}", addrs);
    ///     });
    /// ```
    pub fn on_listen<F>(mut self, callback: F) -> Self
    where
        F: Fn(&[CoreSocketAddr]) + Send + Sync + 'static,
    {
        self.listen_callback = Some(Box::new(callback));
        self
    }

    /// 设置关停时的回调函数。
    ///
    /// 回调函数会在收到关停信号后、开始关停流程前被调用。
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use silent::NetServer;
    ///
    /// let server = NetServer::new()
    ///     .bind("127.0.0.1:8080".parse().unwrap())
    ///     .set_shutdown_callback(|| {
    ///         println!("Server is shutting down...");
    ///     });
    /// ```
    pub fn set_shutdown_callback<F>(mut self, callback: F) -> Self
    where
        F: Fn() + Send + Sync + 'static,
    {
        self.shutdown_callback = Some(Box::new(callback));
        self
    }

    /// 配置连接限流器（令牌桶算法）。
    ///
    /// 限流器用于控制连接接受速率，防止服务器过载。
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use silent::{NetServer, RateLimiterConfig};
    /// use std::time::Duration;
    ///
    /// let config = RateLimiterConfig {
    ///     capacity: 10,
    ///     refill_every: Duration::from_millis(10),
    ///     max_wait: Duration::from_secs(2),
    /// };
    ///
    /// let server = NetServer::new()
    ///     .bind("127.0.0.1:8080".parse().unwrap())
    ///     .with_rate_limiter(config);
    /// ```
    pub fn with_rate_limiter(mut self, config: RateLimiterConfig) -> Self {
        self.rate_limiter = Some(RateLimiter::new(
            config.capacity,
            config.refill_every,
            config.max_wait,
        ));
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
    /// use silent::NetServer;
    /// use std::time::Duration;
    ///
    /// let server = NetServer::new()
    ///     .bind("127.0.0.1:8080".parse().unwrap())
    ///     .with_shutdown(Duration::from_secs(30));
    /// ```
    pub fn with_shutdown(mut self, graceful_wait: Duration) -> Self {
        self.shutdown_cfg.graceful_wait = graceful_wait;
        self
    }

    /// 启动服务器（异步版本）。
    ///
    /// 此方法会阻塞当前任务，直到收到关停信号（Ctrl-C 或 SIGTERM）。
    ///
    /// # 行为
    ///
    /// 1. 绑定所有配置的监听器
    /// 2. 调用 `on_listen` 回调（如果设置）
    /// 3. 进入主事件循环：
    ///    - 接受新连接（受限流器控制）
    ///    - 为每个连接调用 `handler`
    ///    - 监听关停信号（Ctrl-C 或 SIGTERM）
    /// 4. 收到信号后执行优雅关停
    ///
    /// # Panics
    ///
    /// 如果服务器循环内部发生错误，将 panic。
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use silent::{NetServer, RateLimiterConfig, prelude::*};
    /// use std::time::Duration;
    ///
    /// #[tokio::main]
    /// async fn main() {
    ///     let handler = |mut req: Request| async move {
    ///         Response::ok().body("hello")
    ///     };
    ///
    ///     NetServer::new()
    ///         .bind("127.0.0.1:8080".parse().unwrap())
    ///         .serve(handler)
    ///         .await;
    /// }
    /// ```
    pub async fn serve<H>(self, handler: H)
    where
        H: ConnectionService + Clone,
    {
        if let Err(e) = self.serve_connection_loop(handler).await {
            panic!("server loop failed: {}", e);
        }
    }

    /// 启动服务器（阻塞版本），内部创建多线程 Tokio 运行时。
    pub fn run<H>(self, handler: H)
    where
        H: ConnectionService + Clone,
    {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("failed to build Tokio runtime");
        runtime.block_on(async move {
            if let Err(e) = self.serve_connection_loop(handler).await {
                panic!("server loop failed: {}", e);
            }
        })
    }

    async fn serve_connection_loop<H>(mut self, handler: H) -> io::Result<()>
    where
        H: ConnectionService + Clone,
    {
        let mut listeners = self.listeners_builder.listen()?;
        let addrs = listeners.local_addrs().to_vec();
        if let Some(cb) = &self.listen_callback {
            (cb)(&addrs);
        }

        let mut join_set: JoinSet<()> = JoinSet::new();
        let mut shutdown = ShutdownHandle::new(self.shutdown_callback.take(), self.shutdown_cfg);
        let rate = self_rate_limiter(self.rate_limiter.as_ref());

        loop {
            tokio::select! {
                biased;
                _ = shutdown.signal() => {
                    tracing::info!("shutdown signal received");
                    break;
                }
                accept_result = listeners.accept() => {
                    match accept_result {
                        None => {
                            tracing::info!("listener closed, shutting down");
                            break;
                        }
                        Some(Ok((stream, peer_addr))) => {
                            if let Some(rate) = &rate {
                                let semaphore = rate.semaphore.clone();
                                let max_wait = rate.max_wait;
                                let handler = handler.clone();
                                join_set.spawn(async move {
                                    match tokio::time::timeout(max_wait, semaphore.acquire_owned()).await {
                                        Ok(Ok(_permit)) => {
                                            if let Err(err) = handler.call(stream, peer_addr).await {
                                                tracing::error!("Failed to serve connection: {:?}", err);
                                            }
                                        }
                                        Ok(Err(_)) => {
                                            tracing::warn!("Rate limiter closed, dropping connection: {}", peer_addr);
                                        }
                                        Err(_) => {
                                            tracing::warn!("Rate limiter timeout, dropping connection: {}", peer_addr);
                                        }
                                    }
                                });
                            } else {
                                let handler = handler.clone();
                                join_set.spawn(async move {
                                    if let Err(err) = handler.call(stream, peer_addr).await {
                                        tracing::error!("Failed to serve connection: {:?}", err);
                                    }
                                });
                            }
                        }
                        Some(Err(e)) => {
                            tracing::error!(error = ?e, "accept connection failed");
                        }
                    }
                }
                Some(join_result) = join_set.join_next() => {
                    if let Err(err) = join_result {
                        tracing::error!(error = ?err, "connection task panicked");
                    }
                }
            }
        }

        // 优雅关停：等待活动任务在指定时间内完成
        if shutdown.shutdown_cfg.graceful_wait > Duration::from_millis(0) {
            // 使用 timeout 等待所有任务完成，超时后自动结束
            let _ = tokio::time::timeout(shutdown.shutdown_cfg.graceful_wait, async {
                while let Some(join_result) = join_set.join_next().await {
                    if let Err(err) = join_result
                        && err.is_panic()
                    {
                        tracing::error!(error = ?err, "connection task panicked during graceful shutdown");
                    }
                }
            })
            .await;
        }

        // 强制取消剩余任务并清理
        join_set.abort_all();
        while let Some(join_result) = join_set.join_next().await {
            if let Err(err) = join_result
                && err.is_panic()
            {
                tracing::error!(error = ?err, "connection task panicked during forced shutdown");
            }
        }

        Ok(())
    }
}

#[derive(Clone)]
struct RateLimiter {
    semaphore: Arc<Semaphore>,
    max_wait: Duration,
}

impl RateLimiter {
    fn new(capacity: usize, refill_every: Duration, max_wait: Duration) -> Self {
        let semaphore = Arc::new(Semaphore::new(capacity));
        // 补充任务：后台周期性增加 1 个令牌，直至容量上限
        let sem_clone = semaphore.clone();
        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(refill_every);
            loop {
                ticker.tick().await;
                // 正确的令牌补充机制：只有当可用许可数小于容量时才补充
                // 通过 available_permits() < capacity 判断是否需要补充
                if sem_clone.available_permits() < capacity {
                    sem_clone.add_permits(1);
                }
            }
        });

        Self {
            semaphore,
            max_wait,
        }
    }
}

#[derive(Clone, Copy)]
struct ShutdownConfig {
    graceful_wait: Duration,
}

impl Default for ShutdownConfig {
    fn default() -> Self {
        Self {
            graceful_wait: Duration::from_secs(0),
        }
    }
}

fn self_rate_limiter(rate: Option<&RateLimiter>) -> Option<RateLimiter> {
    rate.cloned()
}

struct ShutdownHandle {
    shutdown_callback: Option<Box<dyn Fn() + Send + Sync>>,
    shutdown_cfg: ShutdownConfig,
}

impl ShutdownHandle {
    fn new(callback: Option<Box<dyn Fn() + Send + Sync>>, shutdown_cfg: ShutdownConfig) -> Self {
        let shutdown_callback = callback;
        Self {
            shutdown_callback,
            shutdown_cfg,
        }
    }

    async fn signal(&mut self) {
        #[cfg(unix)]
        {
            let mut term =
                tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
                    .expect("Failed to install SIGTERM handler");
            tokio::select! {
                _ = signal::ctrl_c() => (),
                _ = term.recv() => (),
            }
        }

        #[cfg(not(unix))]
        {
            tokio::select! {
                _ = signal::ctrl_c() => (),
            }
        }

        if let Some(cb) = &self.shutdown_callback {
            (cb)();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_rate_limiter_capacity_limit() {
        // 测试容量限制：初始容量为 2，应能获取 2 个令牌
        let limiter = RateLimiter::new(2, Duration::from_secs(60), Duration::from_secs(1));

        // 获取前 2 个令牌应该成功
        let _permit1 = limiter
            .semaphore
            .clone()
            .acquire_owned()
            .await
            .expect("first permit should be available");

        let _permit2 = limiter
            .semaphore
            .clone()
            .acquire_owned()
            .await
            .expect("second permit should be available");

        // 第 3 个令牌应该不可用
        assert_eq!(limiter.semaphore.available_permits(), 0);
    }
}
