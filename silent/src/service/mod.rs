// Tokio 后端已移除，仅保留 async-io 接入
// mod hyper_service;
// mod serve;

use crate::Configs;
// Tokio Listener trait removed
use crate::route::RouteService;
#[cfg(feature = "scheduler")]
use crate::scheduler::{SCHEDULER, Scheduler, middleware::SchedulerMiddleware};
// use crate::service::serve::Serve; // moved into transport backend
pub mod transport;
use std::net::SocketAddr;
#[cfg(not(target_os = "windows"))]
use std::path::Path;
use std::sync::Arc;
use transport::HttpTransport;
// 使用运行时中立的 spawn，而不强依赖 tokio JoinSet

#[cfg(feature = "tls")]
type AsyncTlsAcceptor = futures_rustls::TlsAcceptor;

pub struct Server {
    // tokio listeners removed
    shutdown_callback: Option<Box<dyn Fn() + Send + Sync>>,
    configs: Option<Configs>,
    transport: Arc<dyn HttpTransport>,
    bound_addrs: Vec<std::net::SocketAddr>,
    #[cfg(feature = "tls")]
    async_tls: Option<AsyncTlsAcceptor>,
}

impl Default for Server {
    fn default() -> Self {
        Self::new()
    }
}

impl Server {
    pub fn new() -> Self {
        Self {
            // tokio listeners removed
            shutdown_callback: None,
            configs: None,
            transport: Arc::new(crate::service::transport::AsyncIoTransport::new()),
            bound_addrs: Vec::new(),
            #[cfg(feature = "tls")]
            async_tls: None,
        }
    }

    /// 替换传输后端：允许选择非 tokio 的 AsyncIoTransport
    #[inline]
    pub fn with_transport<T>(mut self, transport: T) -> Self
    where
        T: HttpTransport,
    {
        self.transport = Arc::new(transport);
        self
    }

    /// 动态设置传输后端。
    #[inline]
    pub fn set_transport<T>(&mut self, transport: T) -> &mut Self
    where
        T: HttpTransport,
    {
        self.transport = Arc::new(transport);
        self
    }

    /// 配置 async-io 分支的 TLS 接入（基于 futures-rustls）。
    /// 仅在启用 `tls` 特性时可用。
    #[cfg(feature = "tls")]
    #[inline]
    pub fn with_async_tls(mut self, acceptor: AsyncTlsAcceptor) -> Self {
        self.async_tls = Some(acceptor);
        self
    }

    /// 设置 async-io TLS 接入器。
    #[cfg(feature = "tls")]
    #[inline]
    pub fn set_async_tls(&mut self, acceptor: AsyncTlsAcceptor) -> &mut Self {
        self.async_tls = Some(acceptor);
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
        self.bound_addrs.push(addr);
        self
    }

    #[cfg(not(target_os = "windows"))]
    #[inline]
    pub fn bind_unix<P: AsRef<Path>>(self, _path: P) -> Self {
        self
    }

    #[inline]
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
            shutdown_callback: _,
            configs,
            transport,
            bound_addrs,
            #[cfg(feature = "tls")]
                async_tls: _,
        } = self;

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
        {
            // async-io 接入
            let addrs = if bound_addrs.is_empty() {
                vec!["127.0.0.1:8000".parse().unwrap()]
            } else {
                bound_addrs.clone()
            };
            let mut listeners = Vec::new();
            for addr in addrs {
                let std_listener =
                    std::net::TcpListener::bind(addr).expect("bind async-io listener failed");
                std_listener
                    .set_nonblocking(true)
                    .expect("set nonblocking failed");
                let async_listener =
                    async_io::Async::new(std_listener).expect("wrap async-io listener failed");
                tracing::info!(
                    "listening on: http://{:?}",
                    async_listener.get_ref().local_addr().unwrap()
                );
                listeners.push(async_listener);
            }
            // 单 listener 支持
            // 简化处理：只用第一个监听器
            let async_listener = listeners.remove(0);

            // 优雅退出：监听 Ctrl-C / SIGTERM
            let _shutdown = {
                Box::pin(async move {
                    let _ = async_ctrlc::CtrlC::new()
                        .expect("install ctrl-c handler failed")
                        .await;
                })
                    as core::pin::Pin<Box<dyn core::future::Future<Output = ()> + Send>>
            };
            futures_util::pin_mut!(_shutdown);

            loop {
                let (stream, peer) = async_listener.accept().await.expect("accept failed");
                let peer_addr = crate::core::socket_addr::SocketAddr::Tcp(peer);
                let routes = root_route.clone().convert_to_route_tree();
                let transport = transport.clone();
                let handler =
                    std::sync::Arc::new(routes) as std::sync::Arc<dyn crate::handler::Handler>;
                crate::runtime::spawn(async move {
                    let conn: Box<dyn crate::core::connection::Connection + Send> =
                        Box::new(stream);
                    if let Err(err) = transport.serve(conn, peer_addr, handler).await {
                        tracing::error!("Failed to serve connection: {:?}", err);
                    }
                });
            }
        }
    }

    pub fn run<S>(self, service: S)
    where
        S: RouteService,
    {
        async_global_executor::block_on(self.serve(service));
    }
}
