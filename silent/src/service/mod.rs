use crate::core::listener::ListenersBuilder;
mod hyper_service;
mod serve;

use crate::Configs;
use crate::prelude::Listen;
use crate::route::RouteService;
#[cfg(feature = "scheduler")]
use crate::scheduler::{SCHEDULER, Scheduler, middleware::SchedulerMiddleware};
use crate::service::serve::Serve;
use std::net::SocketAddr;
#[cfg(not(target_os = "windows"))]
use std::path::Path;
use tokio::signal;
use tokio::task::JoinSet;

pub struct Server {
    listeners_builder: ListenersBuilder,
    shutdown_callback: Option<Box<dyn Fn() + Send + Sync>>,
    configs: Option<Configs>,
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

    pub async fn serve<S>(self, service: S)
    where
        S: RouteService,
    {
        let Self {
            listeners_builder,
            configs,
            ..
        } = self;

        let mut listener = listeners_builder.listen().expect("failed to listen");
        for addr in listener.local_addrs().iter() {
            tracing::info!("listening on: {:?}", addr);
        }
        let mut root_route = service.route();
        println!("🔍 Server::serve - 服务路由，路径: '{}', 有configs: {}", root_route.path, root_route.get_configs().is_some());
        // 只有当configs不是None时才设置，避免覆盖已有的configs
        if let Some(config) = configs {
            root_route.set_configs(Some(config));
        }
        println!("🔍 Server::serve - 设置configs后，路径: '{}', 有configs: {}", root_route.path, root_route.get_configs().is_some());
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
                            let routes = root_route.clone();
                            join_set.spawn(async move {
                                if let Err(err) = Serve::new(routes).call(stream,peer_addr).await {
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
