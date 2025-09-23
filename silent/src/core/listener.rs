use super::socket_addr::SocketAddr;
#[allow(unused_imports)]
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
// compat removed: use local adapters instead

pub type AcceptFuture<'a> =
    Pin<Box<dyn Future<Output = Result<(Box<dyn Connection + Send>, SocketAddr)>> + Send + 'a>>;

pub trait Listen: Send + Sync {
    fn accept(&self) -> AcceptFuture<'_>;
    fn local_addr(&self) -> Result<SocketAddr>;
}

pub struct Listener {
    inner: Box<dyn Listen + Send + Sync>,
}

impl Listener {
    fn new(inner: Box<dyn Listen + Send + Sync>) -> Self {
        Self { inner }
    }
}

impl From<std::net::TcpListener> for Listener {
    fn from(listener: std::net::TcpListener) -> Self {
        listener
            .set_nonblocking(true)
            .expect("failed to set nonblocking");
        let inner = tokio::net::TcpListener::from_std(listener).expect("failed to convert");
        Listener::new(Box::new(TokioTcpListener(Arc::new(inner))))
    }
}

#[cfg(not(target_os = "windows"))]
impl From<std::os::unix::net::UnixListener> for Listener {
    fn from(value: std::os::unix::net::UnixListener) -> Self {
        let inner = tokio::net::UnixListener::from_std(value).expect("failed to convert");
        Listener::new(Box::new(TokioUnixListener(Arc::new(inner))))
    }
}

impl From<tokio::net::TcpListener> for Listener {
    fn from(listener: tokio::net::TcpListener) -> Self {
        Listener::new(Box::new(TokioTcpListener(Arc::new(listener))))
    }
}

#[cfg(not(target_os = "windows"))]
impl From<tokio::net::UnixListener> for Listener {
    fn from(value: tokio::net::UnixListener) -> Self {
        Listener::new(Box::new(TokioUnixListener(Arc::new(value))))
    }
}

impl Listen for Listener {
    fn accept(&self) -> AcceptFuture<'_> {
        self.inner.accept()
    }

    fn local_addr(&self) -> Result<SocketAddr> {
        self.inner.local_addr()
    }
}

use std::sync::Arc;
struct TokioTcpListener(Arc<tokio::net::TcpListener>);

impl Listen for TokioTcpListener {
    fn accept(&self) -> AcceptFuture<'_> {
        let listener = self.0.clone();
        let accept_future = async move {
            let (stream, addr) = listener.accept().await?;
            let futs_stream = TokioAsFutures(stream);
            Ok((
                Box::new(futs_stream) as Box<dyn Connection + Send>,
                SocketAddr::Tcp(addr),
            ))
        };
        Box::pin(accept_future)
    }

    fn local_addr(&self) -> Result<SocketAddr> {
        self.0.local_addr().map(SocketAddr::Tcp)
    }
}

#[cfg(not(target_os = "windows"))]
struct TokioUnixListener(Arc<tokio::net::UnixListener>);

#[cfg(not(target_os = "windows"))]
impl Listen for TokioUnixListener {
    fn accept(&self) -> AcceptFuture<'_> {
        let listener = self.0.clone();
        let accept_future = async move {
            let (stream, addr) = listener.accept().await?;
            let futs_stream = TokioAsFutures(stream);
            Ok((
                Box::new(futs_stream) as Box<dyn Connection + Send>,
                SocketAddr::Unix(addr.into()),
            ))
        };
        Box::pin(accept_future)
    }

    fn local_addr(&self) -> Result<SocketAddr> {
        Ok(SocketAddr::Unix(self.0.local_addr()?.into()))
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
    fn accept(&self) -> AcceptFuture<'_> {
        let accept_future = async move {
            let (stream, addr) = self.listener.accept().await?;
            // futures-io -> tokio-io for TLS accept
            let tokio_in = FuturesAsTokio(stream);
            let tls_tokio = self.acceptor.accept(tokio_in).await?;
            // tokio-io -> futures-io for returning Connection
            let tls_futs = TokioAsFutures(tls_tokio);
            Ok((Box::new(tls_futs) as Box<dyn Connection + Send>, addr))
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

// tokio-io -> futures-io 适配器
struct TokioAsFutures<T>(T);

impl<T> futures::io::AsyncRead for TokioAsFutures<T>
where
    T: tokio::io::AsyncRead + Unpin + Send,
{
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &mut [u8],
    ) -> std::task::Poll<std::io::Result<usize>> {
        let mut rb = tokio::io::ReadBuf::new(buf);
        match Pin::new(&mut self.0).poll_read(cx, &mut rb) {
            std::task::Poll::Ready(Ok(())) => std::task::Poll::Ready(Ok(rb.filled().len())),
            std::task::Poll::Ready(Err(e)) => std::task::Poll::Ready(Err(e)),
            std::task::Poll::Pending => std::task::Poll::Pending,
        }
    }
}

impl<T> futures::io::AsyncWrite for TokioAsFutures<T>
where
    T: tokio::io::AsyncWrite + Unpin + Send,
{
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &[u8],
    ) -> std::task::Poll<std::io::Result<usize>> {
        Pin::new(&mut self.0).poll_write(cx, buf)
    }

    fn poll_flush(
        mut self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        Pin::new(&mut self.0).poll_flush(cx)
    }

    fn poll_close(
        mut self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        Pin::new(&mut self.0).poll_shutdown(cx)
    }
}

// futures-io -> tokio-io 适配器
struct FuturesAsTokio<T>(T);

impl<T> tokio::io::AsyncRead for FuturesAsTokio<T>
where
    T: futures::io::AsyncRead + Unpin + Send,
{
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        let unfilled = buf.initialize_unfilled();
        match Pin::new(&mut self.0).poll_read(cx, unfilled) {
            std::task::Poll::Ready(Ok(n)) => {
                unsafe { buf.assume_init(n) };
                buf.advance(n);
                std::task::Poll::Ready(Ok(()))
            }
            std::task::Poll::Ready(Err(e)) => std::task::Poll::Ready(Err(e)),
            std::task::Poll::Pending => std::task::Poll::Pending,
        }
    }
}

impl<T> tokio::io::AsyncWrite for FuturesAsTokio<T>
where
    T: futures::io::AsyncWrite + Unpin + Send,
{
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &[u8],
    ) -> std::task::Poll<std::io::Result<usize>> {
        Pin::new(&mut self.0).poll_write(cx, buf)
    }

    fn poll_flush(
        mut self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        Pin::new(&mut self.0).poll_flush(cx)
    }

    fn poll_shutdown(
        mut self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        Pin::new(&mut self.0).poll_close(cx)
    }
}

impl Listeners {
    pub(crate) async fn accept(
        &mut self,
    ) -> Option<Result<(Box<dyn Connection + Send>, SocketAddr)>> {
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
