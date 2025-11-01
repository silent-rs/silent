use super::connection::Connection;
use super::stream::Stream;
#[cfg(feature = "tls")]
use crate::CertificateStore;
use crate::core::socket_addr::SocketAddr;
use futures_util::StreamExt;
use futures_util::stream::FuturesUnordered;
use std::future::Future;
use std::io::Result;
#[cfg(not(target_os = "windows"))]
use std::path::Path;
use std::pin::Pin;
#[cfg(feature = "tls")]
use tokio_rustls::TlsAcceptor;

pub type AcceptFuture<'a> = Pin<
    Box<dyn Future<Output = Result<(Box<dyn Connection + Send + Sync>, SocketAddr)>> + Send + 'a>,
>;

pub trait Listen: Send + Sync {
    fn accept(&self) -> AcceptFuture<'_>;
    fn local_addr(&self) -> Result<SocketAddr>;
}

pub enum Listener {
    TcpListener(tokio::net::TcpListener),
    #[cfg(not(target_os = "windows"))]
    UnixListener(tokio::net::UnixListener),
}

impl From<std::net::TcpListener> for Listener {
    fn from(listener: std::net::TcpListener) -> Self {
        // 设置为非阻塞模式
        if let Err(e) = listener.set_nonblocking(true) {
            tracing::error!(error = ?e, "failed to set nonblocking mode for TcpListener");
            panic!("failed to set nonblocking: {}", e);
        }
        // 转换为 tokio TcpListener
        match tokio::net::TcpListener::from_std(listener) {
            Ok(tokio_listener) => Listener::TcpListener(tokio_listener),
            Err(e) => {
                tracing::error!(error = ?e, "failed to convert std::net::TcpListener to tokio::net::TcpListener");
                panic!("failed to convert TcpListener: {}", e);
            }
        }
    }
}

#[cfg(not(target_os = "windows"))]
impl From<std::os::unix::net::UnixListener> for Listener {
    fn from(value: std::os::unix::net::UnixListener) -> Self {
        match tokio::net::UnixListener::from_std(value) {
            Ok(tokio_listener) => Listener::UnixListener(tokio_listener),
            Err(e) => {
                tracing::error!(error = ?e, "failed to convert std::os::unix::net::UnixListener to tokio::net::UnixListener");
                panic!("failed to convert UnixListener: {}", e);
            }
        }
    }
}

impl From<tokio::net::TcpListener> for Listener {
    fn from(listener: tokio::net::TcpListener) -> Self {
        Listener::TcpListener(listener)
    }
}

#[cfg(not(target_os = "windows"))]
impl From<tokio::net::UnixListener> for Listener {
    fn from(value: tokio::net::UnixListener) -> Self {
        Listener::UnixListener(value)
    }
}

impl Listen for Listener {
    fn accept(&self) -> AcceptFuture<'_> {
        match self {
            Listener::TcpListener(listener) => {
                let accept_future = async move {
                    let (stream, addr) = listener.accept().await?;
                    Ok((
                        Box::new(Stream::TcpStream(stream)) as Box<dyn Connection + Send + Sync>,
                        SocketAddr::Tcp(addr),
                    ))
                };
                Box::pin(accept_future)
            }
            #[cfg(not(target_os = "windows"))]
            Listener::UnixListener(listener) => {
                let accept_future = async move {
                    let (stream, addr) = listener.accept().await?;
                    Ok((
                        Box::new(Stream::UnixStream(stream)) as Box<dyn Connection + Send + Sync>,
                        SocketAddr::Unix(addr.into()),
                    ))
                };
                Box::pin(accept_future)
            }
        }
    }

    fn local_addr(&self) -> Result<SocketAddr> {
        match self {
            Listener::TcpListener(listener) => listener.local_addr().map(SocketAddr::Tcp),
            #[cfg(not(target_os = "windows"))]
            Listener::UnixListener(listener) => Ok(SocketAddr::Unix(listener.local_addr()?.into())),
        }
    }
}

#[cfg(feature = "tls")]
impl Listener {
    pub fn tls(self, acceptor: TlsAcceptor) -> TlsListener {
        TlsListener {
            listener: self,
            acceptor,
        }
    }

    pub fn tls_with_cert(self, cert: &CertificateStore) -> TlsListener {
        self.tls(TlsAcceptor::from(cert.https_config().unwrap()))
    }
}

#[cfg(feature = "tls")]
pub struct TlsListener {
    pub listener: Listener,
    pub acceptor: TlsAcceptor,
}

#[cfg(feature = "tls")]
impl Listen for TlsListener {
    fn accept(&self) -> AcceptFuture<'_> {
        let accept_future = async move {
            let (stream, addr) = self.listener.accept().await?;
            let tls_stream = self.acceptor.accept(stream).await?;
            Ok((
                Box::new(tls_stream) as Box<dyn Connection + Send + Sync>,
                addr,
            ))
        };
        Box::pin(accept_future)
    }

    fn local_addr(&self) -> Result<SocketAddr> {
        self.listener.local_addr()?.tls()
    }
}

#[derive(Default)]
pub struct ListenersBuilder {
    listeners: Vec<Box<dyn Listen + Send + Sync + 'static>>,
}

impl ListenersBuilder {
    pub fn new() -> Self {
        Self { listeners: vec![] }
    }

    pub fn add_listener(&mut self, listener: Box<dyn Listen + Send + Sync>) {
        self.listeners.push(listener);
    }

    pub fn bind(&mut self, addr: std::net::SocketAddr) {
        match std::net::TcpListener::bind(addr) {
            Ok(listener) => self.listeners.push(Box::new(Listener::from(listener))),
            Err(e) => {
                tracing::error!(addr = ?addr, error = ?e, "failed to bind TCP listener");
                panic!("failed to bind TCP listener on {}: {}", addr, e);
            }
        }
    }

    #[cfg(not(target_os = "windows"))]
    pub fn bind_unix<P: AsRef<Path>>(&mut self, path: P) {
        let path = path.as_ref();
        match std::os::unix::net::UnixListener::bind(path) {
            Ok(listener) => self.listeners.push(Box::new(Listener::from(listener))),
            Err(e) => {
                tracing::error!(path = ?path, error = ?e, "failed to bind Unix socket listener");
                panic!("failed to bind Unix socket listener on {:?}: {}", path, e);
            }
        }
    }
    pub fn listen(mut self) -> Result<Listeners> {
        if self.listeners.is_empty() {
            match std::net::TcpListener::bind("127.0.0.1:0") {
                Ok(listener) => self.listeners.push(Box::new(Listener::from(listener))),
                Err(e) => {
                    tracing::error!(error = ?e, "failed to bind default TCP listener on 127.0.0.1:0");
                    panic!("failed to bind default listener: {}", e);
                }
            }
        }
        let local_addrs = self
            .listeners
            .iter()
            .flat_map(|listener| listener.local_addr())
            .collect();
        let listeners = self.listeners;
        Ok(Listeners {
            listeners,
            local_addrs,
        })
    }
}

pub struct Listeners {
    listeners: Vec<Box<dyn Listen + Send + Sync + 'static>>,
    local_addrs: Vec<SocketAddr>,
}

impl Listeners {
    pub async fn accept(
        &mut self,
    ) -> Option<Result<(Box<dyn Connection + Send + Sync>, SocketAddr)>> {
        let mut listener_futures: FuturesUnordered<AcceptFuture<'_>> = self
            .listeners
            .iter()
            .map(|listener| {
                let fut: AcceptFuture<'_> = Box::pin(async move {
                    let listener = listener.as_ref();
                    listener.accept().await
                });
                fut
            })
            .collect();
        listener_futures.next().await
    }

    pub fn local_addrs(&self) -> &[SocketAddr] {
        &self.local_addrs
    }
}
