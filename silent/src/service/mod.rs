pub mod connection;
pub mod connection_service;
mod hyper_service;
pub mod listener;
pub mod net_server;
pub mod route_service;
pub mod stream;
#[cfg(feature = "tls")]
pub mod tls;
#[cfg(feature = "tls")]
pub use tls::{CertificateStore, CertificateStoreBuilder};

pub use route_service::RouteConnectionService;

use crate::core::socket_addr::SocketAddr as CoreSocketAddr;
pub use connection_service::{BoxError, ConnectionFuture, ConnectionService};
use listener::{Listen, ListenersBuilder};
use std::net::SocketAddr;
#[cfg(not(target_os = "windows"))]
use std::path::Path;
type ListenCallback = Box<dyn Fn(&[CoreSocketAddr]) + Send + Sync>;

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
        // 启动调度器（如果启用了 scheduler feature）
        #[cfg(feature = "scheduler")]
        {
            use crate::scheduler::{SCHEDULER, Scheduler};
            tokio::spawn(async move {
                let scheduler = SCHEDULER.clone();
                Scheduler::schedule(scheduler).await;
            });
        }

        // 将网络层职责完全委托给通用 NetServer
        net_server::NetServer::from_parts(
            self.listeners_builder,
            self.shutdown_callback,
            self.listen_callback,
        )
        .serve(handler)
        .await
    }

    pub fn run<H>(self, handler: H)
    where
        H: ConnectionService,
    {
        // 启动调度器（如果启用了 scheduler feature）
        #[cfg(feature = "scheduler")]
        {
            use crate::scheduler::{SCHEDULER, Scheduler};
            tokio::spawn(async move {
                let scheduler = SCHEDULER.clone();
                Scheduler::schedule(scheduler).await;
            });
        }

        // 将网络层职责完全委托给通用 NetServer
        net_server::NetServer::from_parts(
            self.listeners_builder,
            self.shutdown_callback,
            self.listen_callback,
        )
        .run(handler)
    }
}
