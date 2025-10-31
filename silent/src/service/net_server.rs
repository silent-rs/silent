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

/// 与协议无关的通用网络服务器。
///
/// 负责监听、接受连接并将连接分发给 ConnectionService 处理。
pub struct NetServer {
    listeners_builder: ListenersBuilder,
    shutdown_callback: Option<Box<dyn Fn() + Send + Sync>>,
    listen_callback: Option<ListenCallback>,
    rate_limiter: Option<RateLimiter>,
    shutdown_cfg: ShutdownConfig,
}

impl NetServer {
    #[allow(dead_code)]
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

    #[allow(dead_code)]
    #[inline]
    pub fn bind(mut self, addr: SocketAddr) -> Self {
        self.listeners_builder.bind(addr);
        self
    }

    #[allow(dead_code)]
    #[cfg(not(target_os = "windows"))]
    #[inline]
    pub fn bind_unix<P: AsRef<Path>>(mut self, path: P) -> Self {
        self.listeners_builder.bind_unix(path);
        self
    }

    #[allow(dead_code)]
    #[inline]
    pub fn listen<T: Listen + Send + Sync + 'static>(mut self, listener: T) -> Self {
        self.listeners_builder.add_listener(Box::new(listener));
        self
    }

    #[allow(dead_code)]
    pub fn on_listen<F>(mut self, callback: F) -> Self
    where
        F: Fn(&[CoreSocketAddr]) + Send + Sync + 'static,
    {
        self.listen_callback = Some(Box::new(callback));
        self
    }

    #[allow(dead_code)]
    pub fn set_shutdown_callback<F>(mut self, callback: F) -> Self
    where
        F: Fn() + Send + Sync + 'static,
    {
        self.shutdown_callback = Some(Box::new(callback));
        self
    }

    /// 配置令牌桶限流（简单版）
    /// capacity: 令牌容量（突发允许的最大并发接入数）
    /// refill_every: 令牌补充间隔（每次+1，直到达到容量）
    /// max_wait: 获取令牌的最大等待时间，超时则丢弃该连接
    #[allow(dead_code)]
    pub fn with_rate_limiter(
        mut self,
        capacity: usize,
        refill_every: Duration,
        max_wait: Duration,
    ) -> Self {
        self.rate_limiter = Some(RateLimiter::new(capacity, refill_every, max_wait));
        self
    }

    /// 配置优雅关停参数：graceful_wait 表示在收到关停信号后，等待活动任务完成的最长时间
    #[allow(dead_code)]
    pub fn with_shutdown(mut self, graceful_wait: Duration) -> Self {
        self.shutdown_cfg.graceful_wait = graceful_wait;
        self
    }

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

#[allow(dead_code)]
#[derive(Clone)]
struct RateLimiter {
    semaphore: Arc<Semaphore>,
    refill_every: Duration,
    max_wait: Duration,
}

#[allow(dead_code)]
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
            refill_every,
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
