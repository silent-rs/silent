mod hyper_service;
mod serve;

use crate::route::RouteService;
use crate::service::serve::Serve;
use crate::Configs;
#[cfg(feature = "scheduler")]
use crate::Scheduler;
use std::net::SocketAddr;
use tokio::net::TcpListener;
use tokio::signal;
use tokio::task::JoinSet;
pub struct Server {
    addr: Option<SocketAddr>,
    listener: Option<TcpListener>,
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
            addr: None,
            listener: None,
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
        self.addr = Some(addr);
        self
    }

    #[inline]
    pub fn listen(mut self, listener: TcpListener) -> Self {
        self.listener = Some(listener);
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
            listener,
            configs,
            addr,
            ..
        } = self;

        let listener = match listener {
            None => match addr {
                None => TcpListener::bind("127.0.0.1:0")
                    .await
                    .expect("failed to listen"),
                Some(addr) => TcpListener::bind(addr)
                    .await
                    .unwrap_or_else(|_| panic!("failed to listen {}", addr)),
            },
            Some(listener) => listener,
        };
        tracing::info!(
            "listening on: http{}//{}",
            ":",
            listener.local_addr().unwrap()
        );
        let mut root_route = service.route();
        root_route.set_configs(configs.clone());
        #[cfg(feature = "session")]
        root_route.check_session();
        #[cfg(feature = "cookie")]
        root_route.check_cookie();
        #[cfg(feature = "scheduler")]
        let scheduler = root_route.scheduler.clone();
        #[cfg(feature = "scheduler")]
        tokio::spawn(async move {
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
                    join_set.abort_all();
                    if let Some(ref callback) = self.shutdown_callback { callback() };
                    break;
                }
                _ = terminate => {
                    join_set.abort_all();
                    if let Some(ref callback) = self.shutdown_callback { callback() };
                    break;
                }
                s = listener.accept() =>{
                    match s{
                        Ok((stream, peer_addr)) => {
                            tracing::info!("Accepting from: {}", stream.peer_addr().unwrap());
                            let routes = root_route.clone();
                            join_set.spawn(async move {
                                if let Err(err) = Serve::new(routes, peer_addr).call(stream).await {
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
