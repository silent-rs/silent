pub mod connection;
mod hyper_service;
pub mod listener;
mod serve;
pub mod stream;
#[cfg(feature = "tls")]
pub mod tls;
#[cfg(feature = "tls")]
pub use tls::{CertificateStore, CertificateStoreBuilder};

use crate::core::socket_addr::SocketAddr as CoreSocketAddr;
use crate::route::Route;
#[cfg(feature = "scheduler")]
use crate::scheduler::middleware::SchedulerMiddleware;
use crate::service::connection::BoxedConnection;
use crate::service::serve::Serve;
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

impl ConnectionService for Route {
    fn call(&self, stream: BoxedConnection, peer: CoreSocketAddr) -> ConnectionFuture {
        // 尝试将连接转换为 QuicConnection
        #[cfg(feature = "quic")]
        {
            use crate::quic::connection::QuicConnection;
            match stream.downcast::<QuicConnection>() {
                Ok(quic) => {
                    // QUIC 连接处理
                    let routes = Arc::new(self.clone());
                    Box::pin(async move {
                        let incoming = quic.into_incoming();
                        crate::quic::service::handle_quic_connection(incoming, routes)
                            .await
                            .map_err(BoxError::from)
                    })
                }
                Err(stream) => {
                    // 不是 QUIC 连接，继续处理为 HTTP/1.1 或 HTTP/2
                    Self::handle_http_connection(self.clone(), stream, peer)
                }
            }
        }

        // 没有 QUIC feature 时的 HTTP/1.1 或 HTTP/2 连接处理
        #[cfg(not(feature = "quic"))]
        Self::handle_http_connection(self.clone(), stream, peer)
    }
}

impl Route {
    fn handle_http_connection(
        root_route: Route,
        stream: BoxedConnection,
        peer: CoreSocketAddr,
    ) -> ConnectionFuture {
        #[allow(unused_mut)]
        let mut root_route = root_route;
        #[cfg(feature = "session")]
        root_route.check_session();
        #[cfg(feature = "cookie")]
        root_route.check_cookie();
        #[cfg(feature = "scheduler")]
        root_route.hook_first(SchedulerMiddleware::new());

        let routes = root_route.convert_to_route_tree();
        Box::pin(async move { Serve::new(routes).call(stream, peer).await })
    }
}

pub struct Server {
    listeners_builder: ListenersBuilder,
    shutdown_callback: Option<Box<dyn Fn() + Send + Sync>>,
    listen_callback: Option<ListenCallback>,
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
        }
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

    pub async fn serve<H>(self, handler: H)
    where
        H: ConnectionService,
    {
        self.serve_with_connection_handler(handler).await
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

    async fn serve_with_connection_handler<H>(self, handler: H)
    where
        H: ConnectionService,
    {
        let Server {
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
