use super::ConnectionService;
use super::listener::{Listen, ListenersBuilder};
use crate::core::socket_addr::SocketAddr as CoreSocketAddr;
use std::io;
use std::net::SocketAddr;
#[cfg(not(target_os = "windows"))]
use std::path::Path;
use std::sync::Arc;
#[cfg(test)]
use std::sync::OnceLock;
use std::time::Duration;
use tokio::signal;
use tokio::sync::Semaphore;
use tokio::task::JoinSet;
#[cfg(test)]
static SHUTDOWN_NOTIFY: OnceLock<tokio::sync::Notify> = OnceLock::new();

#[cfg(test)]
fn trigger_test_shutdown() {
    SHUTDOWN_NOTIFY
        .get_or_init(tokio::sync::Notify::new)
        .notify_waiters();
}

fn test_shutdown_future() -> impl std::future::Future<Output = ()> {
    #[cfg(test)]
    {
        SHUTDOWN_NOTIFY
            .get_or_init(tokio::sync::Notify::new)
            .notified()
    }
    #[cfg(not(test))]
    {
        futures_util::future::pending::<()>()
    }
}

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
///         .bind("127.0.0.1:8080".parse().unwrap()).unwrap()
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
    /// ```no_run
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
    /// ```no_run
    /// use silent::NetServer;
    ///
    /// let server = NetServer::new()
    ///     .bind("127.0.0.1:8080".parse().unwrap()).unwrap()
    ///     .bind("127.0.0.1:8081".parse().unwrap()).unwrap();
    /// ```
    #[inline]
    pub fn bind(mut self, addr: SocketAddr) -> Result<Self, io::Error> {
        self.listeners_builder.bind(addr)?;
        Ok(self)
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
    pub fn bind_unix<P: AsRef<Path>>(mut self, path: P) -> Result<Self, io::Error> {
        self.listeners_builder.bind_unix(path)?;
        Ok(self)
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
    ///     .listen(silent::Listener::from(custom_listener));
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
    ///     .bind("127.0.0.1:0".parse().unwrap()).unwrap()  // 随机端口
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
    ///     .bind("127.0.0.1:8080".parse().unwrap()).unwrap()
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
    ///     .bind("127.0.0.1:8080".parse().unwrap()).unwrap()
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
    /// let _server = NetServer::new()
    ///     .bind("127.0.0.1:8080".parse().unwrap()).unwrap()
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
    /// use silent::{NetServer, BoxedConnection, SocketAddr};
    /// use std::time::Duration;
    ///
    /// #[tokio::main]
    /// async fn main() {
    ///     let handler = |_s: BoxedConnection, _p: SocketAddr| async move {
    ///         Ok::<_, Box<dyn std::error::Error + Send + Sync>>(())
    ///     };
    ///
    ///     NetServer::new()
    ///         .bind("127.0.0.1:8080".parse().unwrap()).unwrap()
    ///         .serve(handler)
    ///         .await;
    /// }
    /// ```
    ///
    /// 提示：若处理器包含状态或不易 `Clone`，可使用 [`serve_arc`](Self::serve_arc)
    /// 或 [`serve_dyn`](Self::serve_dyn) 传入 `Arc` 包装。
    pub async fn serve<H>(self, handler: H)
    where
        H: ConnectionService + 'static,
    {
        if let Err(e) = self.serve_arc(std::sync::Arc::new(handler)).await {
            panic!("server loop failed: {}", e);
        }
    }

    /// 启动服务器（阻塞版本），内部创建多线程 Tokio 运行时。
    ///
    /// 同 [`serve`](Self::serve)。若处理器不易 `Clone`，推荐使用
    /// [`serve_arc`](Self::serve_arc) 或 [`serve_dyn`](Self::serve_dyn)。
    pub fn run<H>(self, handler: H)
    where
        H: ConnectionService + 'static,
    {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("failed to build Tokio runtime");
        runtime.block_on(async move {
            if let Err(e) = self.serve_arc(std::sync::Arc::new(handler)).await {
                panic!("server loop failed: {}", e);
            }
        })
    }

    /// 使用 Arc 包装的处理器（泛型版）。
    ///
    /// 适用于携带共享状态的处理器，实现 `ConnectionService` 即可。
    pub async fn serve_arc<H>(self, handler: std::sync::Arc<H>) -> io::Result<()>
    where
        H: ConnectionService + 'static,
    {
        // 向下转为 trait 对象，复用 dyn 版本
        self.serve_dyn(handler as std::sync::Arc<dyn ConnectionService>)
            .await
    }

    /// 使用 `Arc<dyn ConnectionService>` 的处理器。
    ///
    /// 适用于动态分发场景或需要跨 crate 以 trait 对象形式传递处理器的情况。
    pub async fn serve_dyn(self, handler: std::sync::Arc<dyn ConnectionService>) -> io::Result<()> {
        self.serve_connection_loop(handler).await
    }

    async fn serve_connection_loop(
        mut self,
        handler: std::sync::Arc<dyn ConnectionService>,
    ) -> io::Result<()> {
        let mut listeners = self.listeners_builder.listen()?;
        let addrs = listeners.local_addrs().to_vec();
        if let Some(cb) = &self.listen_callback {
            (cb)(&addrs);
        } else {
            // 默认打印监听地址（逐行展示，更清晰）
            if addrs.len() == 1 {
                tracing::info!("listening on {}", format!("{:?}", addrs[0]));
            } else {
                let lines = addrs
                    .iter()
                    .map(|a| format!("  - {:?}", a))
                    .collect::<Vec<_>>()
                    .join("\n");
                tracing::info!("listening on:\n{}", lines);
            }
        }

        let mut join_set: JoinSet<()> = JoinSet::new();
        let mut shutdown = ShutdownHandle::new(self.shutdown_callback.take(), self.shutdown_cfg);
        let rate = self_rate_limiter(self.rate_limiter.as_ref());
        // 启动限流器补充任务（若配置）
        let mut refill_handle = rate.as_ref().map(|r| r.spawn_refill_task());

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
                // 测试关停注入点（非测试构建为 pending，不影响选择其他分支）
                _ = test_shutdown_future() => {
                    tracing::info!("test shutdown notify received");
                    break;
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

        // 结束限流补充任务
        if let Some(h) = &mut refill_handle {
            h.abort();
            let _ = h.await;
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
    capacity: usize,
    refill_every: Duration,
}

impl RateLimiter {
    fn new(capacity: usize, refill_every: Duration, max_wait: Duration) -> Self {
        let semaphore = Arc::new(Semaphore::new(capacity));
        Self {
            semaphore,
            max_wait,
            capacity,
            refill_every,
        }
    }

    fn spawn_refill_task(&self) -> tokio::task::JoinHandle<()> {
        let sem = self.semaphore.clone();
        let capacity = self.capacity;
        let refill_every = self.refill_every;
        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(refill_every);
            loop {
                ticker.tick().await;
                if sem.available_permits() < capacity {
                    sem.add_permits(1);
                }
            }
        })
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
    use crate::server::connection;
    use crate::server::connection::BoxedConnection;
    use crate::server::listener::Listen;
    use crate::{AcceptFuture, BoxError};
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

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

    #[tokio::test]
    async fn test_rate_limiter_refill_adds_permit() {
        // 容量为 1，间隔很短，验证补充后可用许可数恢复
        let limiter = RateLimiter::new(1, Duration::from_millis(20), Duration::from_millis(10));
        // 先消耗掉唯一的许可
        let _permit = limiter
            .semaphore
            .clone()
            .acquire_owned()
            .await
            .expect("permit should be available");
        assert_eq!(limiter.semaphore.available_permits(), 0);

        // 启动补充任务，并等待一小段时间
        let handle = limiter.spawn_refill_task();
        tokio::time::sleep(Duration::from_millis(30)).await;
        // 至少应补回 1 个许可
        assert!(limiter.semaphore.available_permits() >= 1);
        handle.abort();
        let _ = handle.await;
    }

    struct TestListener {
        addr: std::net::SocketAddr,
        accepts: Arc<AtomicUsize>,
        once_conn: tokio::sync::Mutex<Option<BoxedConnection>>,
    }

    impl TestListener {
        fn new(conn: BoxedConnection, addr: std::net::SocketAddr) -> Self {
            Self {
                addr,
                accepts: Arc::new(AtomicUsize::new(0)),
                once_conn: tokio::sync::Mutex::new(Some(conn)),
            }
        }
    }

    impl Listen for TestListener {
        fn accept(&self) -> AcceptFuture<'_> {
            let accepts = self.accepts.clone();
            let addr = self.addr;
            let once = self.once_conn.try_lock();
            // 第一次返回一个连接，之后挂起（避免忙等）
            if let Ok(mut guard) = once
                && let Some(conn) = guard.take()
            {
                accepts.fetch_add(1, Ordering::SeqCst);
                return Box::pin(async move {
                    Ok((conn, crate::core::socket_addr::SocketAddr::from(addr)))
                });
            }
            Box::pin(async move {
                futures_util::future::pending::<
                    std::io::Result<(
                        Box<dyn connection::Connection + Send + Sync>,
                        crate::core::socket_addr::SocketAddr,
                    )>,
                >()
                .await
            })
        }

        fn local_addr(&self) -> std::io::Result<crate::core::socket_addr::SocketAddr> {
            Ok(crate::core::socket_addr::SocketAddr::from(self.addr))
        }
    }

    #[tokio::test]
    async fn test_net_server_on_listen_and_handler_called_then_abort() {
        // 构造一个一次性连接（不会真正读写）
        let (_a, b) = tokio::io::duplex(8);
        let boxed: BoxedConnection = Box::new(b);
        let addr: std::net::SocketAddr = "127.0.0.1:0".parse().unwrap();
        let listener = TestListener::new(boxed, addr);

        // 标记 on_listen 是否被调用
        let on_listen_called = Arc::new(AtomicBool::new(false));
        let flag = on_listen_called.clone();

        // 处理器：什么都不做，直接返回 Ok
        let handler =
            |_s: BoxedConnection, _p: CoreSocketAddr| async move { Ok::<(), BoxError>(()) };

        // 启动 NetServer（在后台任务中），短暂等待回调触发后中止
        let server = NetServer::new().listen(listener).on_listen(move |_addrs| {
            flag.store(true, Ordering::SeqCst);
        });

        let jh = tokio::spawn(async move {
            server.serve(handler).await;
        });

        tokio::time::sleep(Duration::from_millis(50)).await;
        assert!(on_listen_called.load(Ordering::SeqCst));
        // 中止后台任务，避免等待关停信号
        jh.abort();
        let _ = jh.await;
    }

    struct TestErrListener {
        addr: std::net::SocketAddr,
        sent_err: Arc<AtomicBool>,
    }

    impl TestErrListener {
        fn new(addr: std::net::SocketAddr) -> Self {
            Self {
                addr,
                sent_err: Arc::new(AtomicBool::new(false)),
            }
        }
    }

    impl Listen for TestErrListener {
        fn accept(&self) -> AcceptFuture<'_> {
            let sent = self.sent_err.clone();
            Box::pin(async move {
                if !sent.swap(true, Ordering::SeqCst) {
                    Err(std::io::Error::other("accept failed (test)"))
                } else {
                    futures_util::future::pending::<
                        std::io::Result<(
                            Box<dyn connection::Connection + Send + Sync>,
                            crate::core::socket_addr::SocketAddr,
                        )>,
                    >()
                    .await
                }
            })
        }

        fn local_addr(&self) -> std::io::Result<crate::core::socket_addr::SocketAddr> {
            Ok(crate::core::socket_addr::SocketAddr::from(self.addr))
        }
    }

    #[tokio::test]
    async fn test_net_server_accept_error_path() {
        let addr: std::net::SocketAddr = "127.0.0.1:0".parse().unwrap();
        let listener = TestErrListener::new(addr);
        let on_listen_called = Arc::new(AtomicBool::new(false));
        let flag = on_listen_called.clone();
        let handler_calls = Arc::new(AtomicUsize::new(0));
        let hc = handler_calls.clone();

        let handler = move |_s: BoxedConnection, _p: CoreSocketAddr| {
            let hc = hc.clone();
            async move {
                hc.fetch_add(1, Ordering::SeqCst);
                Ok::<(), BoxError>(())
            }
        };

        let server = NetServer::new().listen(listener).on_listen(move |_addrs| {
            flag.store(true, Ordering::SeqCst);
        });

        let jh = tokio::spawn(async move { server.serve(handler).await });
        tokio::time::sleep(Duration::from_millis(50)).await;
        assert!(on_listen_called.load(Ordering::SeqCst));
        assert_eq!(handler_calls.load(Ordering::SeqCst), 0);
        jh.abort();
        let _ = jh.await;
    }

    #[tokio::test]
    async fn test_net_server_rate_limiter_timeout_drops_connection() {
        // 连接一次：由于容量=0 且 max_wait 极短，应超时丢弃，不调用处理器
        let (_a, b) = tokio::io::duplex(8);
        let boxed: BoxedConnection = Box::new(b);
        let addr: std::net::SocketAddr = "127.0.0.1:0".parse().unwrap();
        let listener = TestListener::new(boxed, addr);

        let calls = Arc::new(AtomicUsize::new(0));
        let calls_cl = calls.clone();
        let handler = move |_s: BoxedConnection, _p: CoreSocketAddr| {
            let calls_cl = calls_cl.clone();
            async move {
                calls_cl.fetch_add(1, Ordering::SeqCst);
                Ok::<(), BoxError>(())
            }
        };

        let server = NetServer::new()
            .with_rate_limiter(RateLimiterConfig {
                capacity: 0,
                refill_every: Duration::from_millis(100),
                max_wait: Duration::from_millis(5),
            })
            .listen(listener);

        let jh = tokio::spawn(async move { server.serve(handler).await });
        tokio::time::sleep(Duration::from_millis(50)).await;
        assert_eq!(
            calls.load(Ordering::SeqCst),
            0,
            "handler should not be called due to timeout"
        );
        jh.abort();
        let _ = jh.await;
    }

    #[tokio::test]
    async fn test_net_server_handler_panic_logged() {
        // 任务内部 panic，join_next 分支应被驱动（仅覆盖，不断言日志）
        let (_a, b) = tokio::io::duplex(8);
        let boxed: BoxedConnection = Box::new(b);
        let addr: std::net::SocketAddr = "127.0.0.1:0".parse().unwrap();
        let listener = TestListener::new(boxed, addr);

        let handler = |_s: BoxedConnection, _p: CoreSocketAddr| async move {
            panic!("panic in handler (test)");
            #[allow(unreachable_code)]
            Ok::<(), BoxError>(())
        };

        let server = NetServer::new().listen(listener);
        let jh = tokio::spawn(async move { server.serve(handler).await });
        tokio::time::sleep(Duration::from_millis(50)).await;
        jh.abort();
        let _ = jh.await;
    }

    #[tokio::test]
    async fn test_net_server_graceful_shutdown_timeout() {
        // 一次连接，handler 故意延迟，触发优雅关停等待超时
        let (_a, b) = tokio::io::duplex(8);
        let boxed: BoxedConnection = Box::new(b);
        let addr: std::net::SocketAddr = "127.0.0.1:0".parse().unwrap();
        let listener = TestListener::new(boxed, addr);

        let handler = |_s: BoxedConnection, _p: CoreSocketAddr| async move {
            tokio::time::sleep(Duration::from_millis(50)).await;
            Ok::<(), BoxError>(())
        };

        let server = NetServer::new()
            .with_shutdown(Duration::from_millis(10))
            .listen(listener);

        let jh = tokio::spawn(async move { server.serve(handler).await });
        // 等 on_listen 后小等，然后触发测试关停通知
        tokio::time::sleep(Duration::from_millis(10)).await;
        trigger_test_shutdown();
        // 若优雅关停未正确处理，此处会卡住超时
        let _ = jh.await;
    }

    #[tokio::test]
    async fn test_net_server_rate_limiter_permit_calls_handler() {
        // 容量=1，允许一次连接调用
        let (_a, b) = tokio::io::duplex(8);
        let boxed: BoxedConnection = Box::new(b);
        let addr: std::net::SocketAddr = "127.0.0.1:0".parse().unwrap();
        let listener = TestListener::new(boxed, addr);

        let calls = Arc::new(AtomicUsize::new(0));
        let calls_cl = calls.clone();
        let handler = move |_s: BoxedConnection, _p: CoreSocketAddr| {
            let calls_cl = calls_cl.clone();
            async move {
                calls_cl.fetch_add(1, Ordering::SeqCst);
                Ok::<(), BoxError>(())
            }
        };

        let server = NetServer::new()
            .with_rate_limiter(RateLimiterConfig {
                capacity: 1,
                refill_every: Duration::from_millis(1000),
                max_wait: Duration::from_millis(50),
            })
            .listen(listener);

        let jh = tokio::spawn(async move { server.serve(handler).await });
        tokio::time::sleep(Duration::from_millis(50)).await;
        assert_eq!(
            calls.load(Ordering::SeqCst),
            1,
            "handler should be called exactly once"
        );
        jh.abort();
        let _ = jh.await;
    }

    struct TestListenerDelay {
        addr: std::net::SocketAddr,
        once_conn: tokio::sync::Mutex<Option<BoxedConnection>>,
        delay: Duration,
    }

    impl TestListenerDelay {
        fn new(conn: BoxedConnection, addr: std::net::SocketAddr, delay: Duration) -> Self {
            Self {
                addr,
                once_conn: tokio::sync::Mutex::new(Some(conn)),
                delay,
            }
        }
    }

    impl Listen for TestListenerDelay {
        fn accept(&self) -> AcceptFuture<'_> {
            let delay = self.delay;
            let addr = self.addr;
            let once = self.once_conn.try_lock();
            if let Ok(mut guard) = once
                && let Some(conn) = guard.take()
            {
                return Box::pin(async move {
                    tokio::time::sleep(delay).await;
                    Ok((conn, crate::core::socket_addr::SocketAddr::from(addr)))
                });
            }
            Box::pin(async move {
                futures_util::future::pending::<
                    std::io::Result<(
                        Box<dyn connection::Connection + Send + Sync>,
                        crate::core::socket_addr::SocketAddr,
                    )>,
                >()
                .await
            })
        }

        fn local_addr(&self) -> std::io::Result<crate::core::socket_addr::SocketAddr> {
            Ok(crate::core::socket_addr::SocketAddr::from(self.addr))
        }
    }

    #[tokio::test]
    async fn test_net_server_multi_listeners_race() {
        // 快慢两个 listener，优先处理较快的连接一次
        let (_a1, b1) = tokio::io::duplex(8);
        let boxed1: BoxedConnection = Box::new(b1);
        let (_a2, b2) = tokio::io::duplex(8);
        let boxed2: BoxedConnection = Box::new(b2);
        let addr: std::net::SocketAddr = "127.0.0.1:0".parse().unwrap();

        let fast = TestListenerDelay::new(boxed1, addr, Duration::from_millis(1));
        let slow = TestListenerDelay::new(boxed2, addr, Duration::from_millis(50));

        let calls = Arc::new(AtomicUsize::new(0));
        let calls_cl = calls.clone();
        let handler = move |_s: BoxedConnection, _p: CoreSocketAddr| {
            let calls_cl = calls_cl.clone();
            async move {
                calls_cl.fetch_add(1, Ordering::SeqCst);
                Ok::<(), BoxError>(())
            }
        };

        let server = NetServer::new().listen(fast).listen(slow);
        let jh = tokio::spawn(async move { server.serve(handler).await });
        tokio::time::sleep(Duration::from_millis(80)).await;
        assert_eq!(
            calls.load(Ordering::SeqCst),
            1,
            "only fast listener's connection handled"
        );
        jh.abort();
        let _ = jh.await;
    }

    #[tokio::test]
    async fn test_net_server_on_listen_addrs_content() {
        let (_a, b) = tokio::io::duplex(8);
        let boxed: BoxedConnection = Box::new(b);
        let addr: std::net::SocketAddr = "127.0.0.1:5555".parse().unwrap();
        let listener = TestListener::new(boxed, addr);

        let seen = Arc::new(tokio::sync::Mutex::new(Vec::<CoreSocketAddr>::new()));
        let seen_cl = seen.clone();
        let server = NetServer::new().listen(listener).on_listen(move |addrs| {
            let addrs = addrs.to_vec();
            let seen_cl = seen_cl.clone();
            tokio::spawn(async move {
                *seen_cl.lock().await = addrs;
            });
        });

        let handler =
            |_s: BoxedConnection, _p: CoreSocketAddr| async move { Ok::<(), BoxError>(()) };
        let jh = tokio::spawn(async move { server.serve(handler).await });
        tokio::time::sleep(Duration::from_millis(20)).await;
        let addrs = seen.lock().await.clone();
        assert_eq!(addrs.len(), 1);
        assert!(matches!(addrs[0], CoreSocketAddr::Tcp(_)));
        jh.abort();
        let _ = jh.await;
    }

    #[tokio::test]
    async fn test_net_server_on_listen_multi_addrs() {
        let (_a1, b1) = tokio::io::duplex(8);
        let boxed1: BoxedConnection = Box::new(b1);
        let (_a2, b2) = tokio::io::duplex(8);
        let boxed2: BoxedConnection = Box::new(b2);
        let addr1: std::net::SocketAddr = "127.0.0.1:60000".parse().unwrap();
        let addr2: std::net::SocketAddr = "127.0.0.1:60001".parse().unwrap();
        let l1 = TestListener::new(boxed1, addr1);
        let l2 = TestListener::new(boxed2, addr2);

        let seen = Arc::new(tokio::sync::Mutex::new(Vec::<CoreSocketAddr>::new()));
        let seen_cl = seen.clone();
        let server = NetServer::new()
            .listen(l1)
            .listen(l2)
            .on_listen(move |addrs| {
                let addrs = addrs.to_vec();
                let seen_cl = seen_cl.clone();
                tokio::spawn(async move {
                    *seen_cl.lock().await = addrs;
                });
            });

        let handler =
            |_s: BoxedConnection, _p: CoreSocketAddr| async move { Ok::<(), BoxError>(()) };
        let jh = tokio::spawn(async move { server.serve(handler).await });
        tokio::time::sleep(Duration::from_millis(20)).await;
        let addrs = seen.lock().await.clone();
        assert_eq!(addrs.len(), 2);
        jh.abort();
        let _ = jh.await;
    }
}
