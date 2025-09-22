use crate::core::listener::ListenersBuilder;
mod hyper_service;
mod serve;

use crate::Configs;
use crate::prelude::Listen;
use crate::route::RouteService;
#[cfg(feature = "scheduler")]
use crate::scheduler::{SCHEDULER, Scheduler, middleware::SchedulerMiddleware};
// use crate::service::serve::Serve; // moved into transport backend
mod transport;
use std::net::SocketAddr;
#[cfg(not(target_os = "windows"))]
use std::path::Path;
use std::sync::Arc;
use tokio::signal;
use transport::HttpTransport;
use transport::HyperTokioTransport;
// 使用运行时中立的 spawn，而不强依赖 tokio JoinSet

pub struct Server {
    listeners_builder: ListenersBuilder,
    shutdown_callback: Option<Box<dyn Fn() + Send + Sync>>,
    configs: Option<Configs>,
    transport: Arc<dyn HttpTransport>,
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
            configs: None,
            transport: Arc::new(HyperTokioTransport::new()),
        }
    }

    /// 替换传输后端（内部使用）。
    #[inline]
    #[allow(dead_code)]
    pub(crate) fn with_transport<T>(mut self, transport: T) -> Self
    where
        T: HttpTransport,
    {
        self.transport = Arc::new(transport);
        self
    }

    /// 动态设置传输后端（内部使用）。
    #[inline]
    #[allow(dead_code)]
    pub(crate) fn set_transport<T>(&mut self, transport: T) -> &mut Self
    where
        T: HttpTransport,
    {
        self.transport = Arc::new(transport);
        self
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

    pub async fn serve<S>(self, service: S)
    where
        S: RouteService,
    {
        let Self {
            listeners_builder,
            configs,
            transport,
            ..
        } = self;

        let mut listener = listeners_builder.listen().expect("failed to listen");
        for addr in listener.local_addrs().iter() {
            tracing::info!("listening on: {:?}", addr);
        }
        let mut root_route = service.route();

        // 只有当configs不是None时才设置，避免覆盖已有的configs
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
        crate::runtime::spawn(async move {
            let scheduler = SCHEDULER.clone();
            Scheduler::schedule(scheduler).await;
        });
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
                    if let Some(ref callback) = self.shutdown_callback { callback() };
                    break;
                }
                _ = terminate => {
                    if let Some(ref callback) = self.shutdown_callback { callback() };
                    break;
                }
                Some(s) = listener.accept() =>{
                    match s{
                        Ok((stream, peer_addr)) => {
                            tracing::info!("Accepting from: {}", peer_addr);
                            let routes = root_route.clone().convert_to_route_tree();
                            let transport = transport.clone();
                            crate::runtime::spawn(async move {
                                if let Err(err) = transport.serve(stream, peer_addr, routes).await {
                                    tracing::error!("Failed to serve connection: {:?}", err);
                                }
                            });
                        }
                        Err(e) => {
                            tracing::error!(error = ?e, "accept connection failed");
                        }
                    }
                }
            }
        }
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
}
