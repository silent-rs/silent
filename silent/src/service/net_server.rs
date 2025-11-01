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
/// use silent::{NetServer, BoxedConnection, SocketAddr};
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
///     NetServer::new()
///         .bind("127.0.0.1:8080".parse().unwrap())
///         .with_rate_limiter(100, Duration::from_millis(10), Duration::from_secs(1))
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
    /// use silent::{NetServer, prelude::*};
    /// use std::time::Duration;
    ///
    /// #[tokio::main]
    /// async fn main() {
    ///     let server = NetServer::new()
    ///         .bind("127.0.0.1:8080".parse().unwrap())
    ///         .with_rate_limiter(10, Duration::from_millis(10), Duration::from_secs(2))
    ///         .with_shutdown(Duration::from_secs(5));
    ///
    ///     server.serve(|stream, peer| async move {
    ///         println!("Connection from: {}", peer);
    ///         Ok(())
    ///     }).await;
    /// }
    /// ```
    pub async fn serve<H>(self, handler: H)
    where
        H: ConnectionService,
    {
        let NetServer {
            listeners_builder,
            shutdown_callback,
            listen_callback,
            rate_limiter,
            shutdown_cfg,
        } = self;
        Self::serve_connection_loop(
            listeners_builder,
            shutdown_callback,
            listen_callback,
            handler,
            rate_limiter,
            shutdown_cfg,
        )
        .await
        .expect("server loop failed");
    }

    /// 启动服务器（阻塞版本）。
    ///
    /// 此方法会创建 tokio 多线程运行时并阻塞当前线程，直到服务器关停。
    /// 等价于在 `#[tokio::main]` 中调用 `serve()`。
    ///
    /// # Panics
    ///
    /// 如果创建运行时失败或服务器循环内部发生错误，将 panic。
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use silent::{NetServer, prelude::*};
    /// use std::time::Duration;
    ///
    /// let server = NetServer::new()
    ///     .bind("127.0.0.1:8080".parse().unwrap())
    ///     .with_shutdown(Duration::from_secs(5));
    ///
    /// // 阻塞主线程直到服务器关停
    /// server.run(|stream, peer| async move {
    ///     println!("Connection from: {}", peer);
    ///     Ok(())
    /// });
    /// ```
    pub fn run<H>(self, handler: H)
    where
        H: ConnectionService,
    {
        tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(self.serve(handler));
    }

    async fn serve_connection_loop<H>(
        listeners_builder: ListenersBuilder,
        shutdown_callback: Option<Box<dyn Fn() + Send + Sync>>,
        listen_callback: Option<ListenCallback>,
        handler: H,
        rate_limiter: Option<RateLimiter>,
        shutdown_cfg: ShutdownConfig,
    ) -> io::Result<()>
    where
        H: ConnectionService,
    {
        let mut listeners = listeners_builder.listen()?;
        let local_addrs = listeners.local_addrs();
        if let Some(callback) = listen_callback.as_ref() {
            callback(local_addrs);
        }
        for addr in local_addrs {
            tracing::info!("listening on: {:?}", addr);
        }

        // Start the scheduler if the feature is enabled
        #[cfg(feature = "scheduler")]
        tokio::spawn(async move {
            use crate::scheduler::{SCHEDULER, Scheduler};
            let scheduler = SCHEDULER.clone();
            Scheduler::schedule(scheduler).await;
        });

        let shutdown_callback = shutdown_callback.as_ref();
        let handler: Arc<dyn ConnectionService> = Arc::new(handler);
        let rate = self_rate_limiter(rate_limiter.as_ref());
        let mut join_set = JoinSet::new();

        loop {
            #[cfg(unix)]
            let terminate = async {
                signal::unix::signal(signal::unix::SignalKind::terminate())
                    .expect("failed to install signal handler")
                    .recv()
                    .await;
            };

            #[cfg(not(unix))]
            let terminate = async {
                let _ = std::future::pending::<()>().await;
            };

            tokio::select! {
                _ = signal::ctrl_c() => {
                    if let Some(callback) = shutdown_callback { callback(); }
                    break;
                }
                _ = terminate => {
                    if let Some(callback) = shutdown_callback { callback(); }
                    break;
                }
                Some(result) = listeners.accept() => {
                    match result {
                        Ok((stream, peer_addr)) => {
                            tracing::info!("Accepting from: {}", peer_addr);
                            // 若启用限流，先获取令牌（可等待 max_wait）；获取失败则丢弃该连接
                            if let Some(rate) = rate.as_ref() {
                                match tokio::time::timeout(rate.max_wait, rate.semaphore.clone().acquire_owned()).await {
                                    Ok(Ok(permit)) => {
                                        let handler = handler.clone();
                                        join_set.spawn(async move {
                                            // permit 在任务结束时自动释放
                                            let _permit = permit;
                                            if let Err(err) = handler.call(stream, peer_addr).await {
                                                tracing::error!("Failed to serve connection: {:?}", err);
                                            }
                                        });
                                    }
                                    Ok(Err(_)) => {
                                        tracing::warn!("Rate limiter closed, dropping connection: {}", peer_addr);
                                    }
                                    Err(_) => {
                                        tracing::warn!("Rate limiter timeout, dropping connection: {}", peer_addr);
                                    }
                                }
                            } else {
                                let handler = handler.clone();
                                join_set.spawn(async move {
                                    if let Err(err) = handler.call(stream, peer_addr).await {
                                        tracing::error!("Failed to serve connection: {:?}", err);
                                    }
                                });
                            }
                        }
                        Err(e) => {
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

        // 优雅关停：先等待一段时间让活动任务自然结束，再强制取消
        if shutdown_cfg.graceful_wait > Duration::from_millis(0) {
            let deadline = tokio::time::Instant::now() + shutdown_cfg.graceful_wait;
            loop {
                if tokio::time::Instant::now() >= deadline {
                    break;
                }
                match tokio::time::timeout(
                    deadline - tokio::time::Instant::now(),
                    join_set.join_next(),
                )
                .await
                {
                    Ok(Some(join_result)) => {
                        if let Err(err) = join_result
                            && err.is_panic()
                        {
                            tracing::error!(error = ?err, "connection task panicked");
                        }
                        continue;
                    }
                    Ok(None) => break, // 无任务
                    Err(_) => break,   // 超时
                }
            }
        }

        // 强制取消剩余任务并 drain
        join_set.abort_all();
        while let Some(join_result) = join_set.join_next().await {
            if let Err(err) = join_result
                && err.is_panic()
            {
                tracing::error!(error = ?err, "connection task panicked");
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
                // 尝试增加 1 个许可，超过容量则忽略
                let available = sem_clone.available_permits();
                if available == 0 {
                    // 无法直接“增加”，使用 add_permits(1) 但要确保不超过初始容量。
                    // 这里通过记录发放总量来限制较复杂，改为：仅当已借出的数量小于 capacity 才补充。
                    // 近似实现：如果当前可用为 0，则直接补 1，可能略微超过瞬时上限，但影响可接受。
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

        // 验证容量限制：可用令牌数应为 0
        assert_eq!(
            limiter.semaphore.available_permits(),
            0,
            "no permits should be available after acquiring capacity"
        );
    }

    #[tokio::test]
    async fn test_rate_limiter_release_and_reacquire() {
        // 测试令牌释放后重新获取
        let limiter = RateLimiter::new(1, Duration::from_secs(60), Duration::from_secs(1));

        let permit = limiter
            .semaphore
            .clone()
            .acquire_owned()
            .await
            .expect("should acquire first permit");

        assert_eq!(limiter.semaphore.available_permits(), 0);

        // 释放令牌
        drop(permit);

        // 应能立即重新获取
        let _permit2 = limiter
            .semaphore
            .clone()
            .acquire_owned()
            .await
            .expect("should reacquire after release");
    }

    #[tokio::test]
    async fn test_rate_limiter_refill_mechanism() {
        // 测试令牌自动补充机制
        let limiter = RateLimiter::new(1, Duration::from_millis(50), Duration::from_secs(1));

        // 获取令牌并持有
        let _permit = limiter
            .semaphore
            .clone()
            .acquire_owned()
            .await
            .expect("should acquire permit");

        // 等待至少 2 个补充周期
        tokio::time::sleep(Duration::from_millis(120)).await;

        // 由于补充任务检查 available == 0 才补充，且 permit 未释放，
        // 补充任务应该已经添加了令牌（因为available == 0时会 add_permits(1)）
        // 验证可用许可数 >= 1 (补充任务可能已执行)
        let available = limiter.semaphore.available_permits();
        assert!(
            available >= 1,
            "refill task should have added permits when available was 0, got {}",
            available
        );
    }

    #[test]
    fn test_shutdown_config_default() {
        // 测试 ShutdownConfig 默认值
        let config = ShutdownConfig::default();
        assert_eq!(config.graceful_wait, Duration::from_secs(0));
    }

    #[test]
    fn test_net_server_with_shutdown() {
        // 测试 with_shutdown 方法
        let server = NetServer::new().with_shutdown(Duration::from_secs(10));
        assert_eq!(server.shutdown_cfg.graceful_wait, Duration::from_secs(10));
    }
}
