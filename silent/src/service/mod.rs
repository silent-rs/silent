pub mod connection;
mod hyper_service;
pub mod listener;
mod serve;
pub mod stream;

use crate::Configs;
use crate::core::socket_addr::SocketAddr as CoreSocketAddr;
use crate::route::RouteService;
#[cfg(feature = "scheduler")]
use crate::scheduler::{SCHEDULER, Scheduler, middleware::SchedulerMiddleware};
use crate::service::serve::Serve;
use connection::Connection;
use listener::{Listen, ListenersBuilder};
use std::error::Error as StdError;
use std::future::Future;
use std::io;
use std::net::SocketAddr;
#[cfg(not(target_os = "windows"))]
use std::path::Path;
use std::pin::Pin;
use std::sync::Arc;
use tokio::signal;
use tokio::task::JoinSet;

pub type BoxedConnection = Box<dyn Connection + Send + Sync>;
pub type BoxError = Box<dyn StdError + Send + Sync>;
pub type ConnectionFuture = Pin<Box<dyn Future<Output = Result<(), BoxError>> + Send>>;
type ListenCallback = Box<dyn Fn(&[CoreSocketAddr]) + Send + Sync>;

pub trait ConnectionService: Send + Sync + 'static {
    fn call(&self, stream: BoxedConnection, peer: CoreSocketAddr) -> ConnectionFuture;
}

impl<F, Fut> ConnectionService for F
where
    F: Send + Sync + 'static + Fn(BoxedConnection, CoreSocketAddr) -> Fut,
    Fut: Future<Output = Result<(), BoxError>> + Send + 'static,
{
    fn call(&self, stream: BoxedConnection, peer: CoreSocketAddr) -> ConnectionFuture {
        Box::pin((self)(stream, peer))
    }
}

pub struct Server {
    listeners_builder: ListenersBuilder,
    shutdown_callback: Option<Box<dyn Fn() + Send + Sync>>,
    configs: Option<Configs>,
    listen_callback: Option<ListenCallback>,
}

pub type NetServer = Server;

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
            configs: None,
            listen_callback: None,
        }
    }

    #[inline]
    pub fn set_configs(&mut self, configs: Configs) -> &mut Self {
        self.configs = Some(configs);
        self
    }

    #[inline]
    pub fn with_configs(mut self, configs: Configs) -> Self {
        self.configs = Some(configs);
        self
    }

    #[inline]
    pub fn bind(mut self, addr: SocketAddr) -> Self {
        self.listeners_builder.bind(addr);
        self
    }

    #[cfg(not(target_os = "windows"))]
    #[inline]
    pub fn bind_unix<P: AsRef<Path>>(mut self, path: P) -> Self {
        self.listeners_builder.bind_unix(path);
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

    pub async fn serve<S>(self, service: S)
    where
        S: RouteService,
    {
        let Self {
            listeners_builder,
            configs,
            shutdown_callback,
            listen_callback,
        } = self;

        let mut root_route = service.route();

        if let Some(config) = configs {
            root_route.set_configs(Some(config));
        }

        #[cfg(feature = "session")]
        root_route.check_session();
        #[cfg(feature = "cookie")]
        root_route.check_cookie();
        #[cfg(feature = "scheduler")]
        root_route.hook_first(SchedulerMiddleware::new());
        #[cfg(feature = "scheduler")]
        tokio::spawn(async move {
            let scheduler = SCHEDULER.clone();
            Scheduler::schedule(scheduler).await;
        });
        let route = Arc::new(root_route);

        Self::serve_connection_loop(
            listeners_builder,
            shutdown_callback,
            listen_callback,
            move |stream, peer_addr| {
                let route = route.clone();
                async move {
                    let routes = (*route).clone().convert_to_route_tree();
                    Serve::new(routes).call(stream, peer_addr).await
                }
            },
        )
        .await
        .expect("server loop failed");
    }

    pub fn run<S>(self, service: S)
    where
        S: RouteService,
    {
        tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(self.serve(service));
    }

    pub async fn serve_with_connection_handler<H>(self, handler: H)
    where
        H: ConnectionService,
    {
        self
            .run_with_connection_handler(handler)
            .await
            .expect("server loop failed");
    }

    pub async fn run_with_connection_handler<H>(self, handler: H) -> io::Result<()>
    where
        H: ConnectionService,
    {
        let Server {
            listeners_builder,
            shutdown_callback,
            configs,
            listen_callback,
        } = self;

        let _ = configs;
        Self::serve_connection_loop(
            listeners_builder,
            shutdown_callback,
            listen_callback,
            handler,
        )
        .await
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
                    if let Some(callback) = shutdown_callback {
                        callback();
                    }
                    break;
                }
                _ = terminate => {
                    if let Some(callback) = shutdown_callback {
                        callback();
                    }
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

        while let Some(join_result) = join_set.join_next().await {
            if let Err(err) = join_result {
                tracing::error!(error = ?err, "connection task panicked");
            }
        }

        Ok(())
    }
}
