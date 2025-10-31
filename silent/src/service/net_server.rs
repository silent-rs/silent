use super::ConnectionService;
use super::listener::{Listen, ListenersBuilder};
use crate::core::socket_addr::SocketAddr as CoreSocketAddr;
use std::io;
use std::net::SocketAddr;
#[cfg(not(target_os = "windows"))]
use std::path::Path;
use std::sync::Arc;
use tokio::signal;
use tokio::task::JoinSet;

type ListenCallback = Box<dyn Fn(&[CoreSocketAddr]) + Send + Sync>;

/// 与协议无关的通用网络服务器。
///
/// 负责监听、接受连接并将连接分发给 ConnectionService 处理。
pub struct NetServer {
    listeners_builder: ListenersBuilder,
    shutdown_callback: Option<Box<dyn Fn() + Send + Sync>>,
    listen_callback: Option<ListenCallback>,
}

impl NetServer {
    #[allow(dead_code)]
    pub fn new() -> Self {
        Self {
            listeners_builder: ListenersBuilder::new(),
            shutdown_callback: None,
            listen_callback: None,
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

    pub async fn serve<H>(self, handler: H)
    where
        H: ConnectionService,
    {
        let NetServer {
            listeners_builder,
            shutdown_callback,
            listen_callback,
        } = self;
        Self::serve_connection_loop(
            listeners_builder,
            shutdown_callback,
            listen_callback,
            handler,
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
                            let handler = handler.clone();
                            join_set.spawn(async move {
                                if let Err(err) = handler.call(stream, peer_addr).await {
                                    tracing::error!("Failed to serve connection: {:?}", err);
                                }
                            });
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
