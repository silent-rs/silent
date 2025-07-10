use super::socket_addr::SocketAddr;
use super::stream::Stream;
use crate::core::connection::Connection;
use futures_util::StreamExt;
use futures_util::stream::FuturesUnordered;
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
    fn accept(&self) -> AcceptFuture;
    fn local_addr(&self) -> Result<SocketAddr>;
}

pub enum Listener {
    TcpListener(tokio::net::TcpListener),
    #[cfg(not(target_os = "windows"))]
    UnixListener(tokio::net::UnixListener),
}

impl From<std::net::TcpListener> for Listener {
    fn from(listener: std::net::TcpListener) -> Self {
        listener
            .set_nonblocking(true)
            .expect("failed to set nonblocking");
        Listener::TcpListener(
            tokio::net::TcpListener::from_std(listener).expect("failed to convert"),
        )
    }
}

#[cfg(not(target_os = "windows"))]
impl From<std::os::unix::net::UnixListener> for Listener {
    fn from(value: std::os::unix::net::UnixListener) -> Self {
        Listener::UnixListener(
            tokio::net::UnixListener::from_std(value).expect("failed to convert"),
        )
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
    fn accept(&self) -> AcceptFuture {
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
}

#[cfg(feature = "tls")]
pub struct TlsListener {
    pub listener: Listener,
    pub acceptor: TlsAcceptor,
}

#[cfg(feature = "tls")]
impl Listen for TlsListener {
    fn accept(&self) -> AcceptFuture {
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

pub(crate) struct ListenersBuilder {
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
        self.listeners.push(Box::new(Listener::from(
            std::net::TcpListener::bind(addr).expect("failed to bind listener"),
        )));
    }

    #[cfg(not(target_os = "windows"))]
    pub fn bind_unix<P: AsRef<Path>>(&mut self, path: P) {
        self.listeners.push(Box::new(Listener::from(
            std::os::unix::net::UnixListener::bind(path).expect("failed to bind listener"),
        )));
    }
    pub fn listen(mut self) -> Result<Listeners> {
        if self.listeners.is_empty() {
            self.listeners.push(Box::new(Listener::from(
                std::net::TcpListener::bind("127.0.0.1:0").expect("failed to bind listener"),
            )));
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

pub(crate) struct Listeners {
    listeners: Vec<Box<dyn Listen + Send + Sync + 'static>>,
    local_addrs: Vec<SocketAddr>,
}

impl Listeners {
    pub(crate) async fn accept(
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

    pub(crate) fn local_addrs(&self) -> &Vec<SocketAddr> {
        &self.local_addrs
    }
}
