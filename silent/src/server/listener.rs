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

/// 接受连接的 Future。
///
/// 约定：
/// - `Ok((conn, peer))` 表示成功接受到一个连接；
/// - `Err(e)` 表示本次接受失败（可继续下一次 `accept()`）；
/// - `Listeners::accept()` 返回 `None` 表示所有监听器已关闭，应结束主循环。
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

impl TryFrom<std::net::TcpListener> for Listener {
    type Error = std::io::Error;

    fn try_from(listener: std::net::TcpListener) -> std::result::Result<Self, Self::Error> {
        // 设置为非阻塞模式
        listener.set_nonblocking(true)?;
        // 转换为 tokio TcpListener
        let tokio_listener = tokio::net::TcpListener::from_std(listener)?;
        Ok(Listener::TcpListener(tokio_listener))
    }
}

#[cfg(not(target_os = "windows"))]
impl TryFrom<std::os::unix::net::UnixListener> for Listener {
    type Error = std::io::Error;

    fn try_from(value: std::os::unix::net::UnixListener) -> std::result::Result<Self, Self::Error> {
        let tokio_listener = tokio::net::UnixListener::from_std(value)?;
        Ok(Listener::UnixListener(tokio_listener))
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
            Ok(listener) => match Listener::try_from(listener) {
                Ok(listener) => self.listeners.push(Box::new(listener)),
                Err(e) => {
                    tracing::error!(addr = ?addr, error = ?e, "failed to convert TCP listener");
                    panic!("failed to convert TCP listener for {}: {}", addr, e);
                }
            },
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
            Ok(listener) => match Listener::try_from(listener) {
                Ok(listener) => self.listeners.push(Box::new(listener)),
                Err(e) => {
                    tracing::error!(path = ?path, error = ?e, "failed to convert Unix socket listener");
                    panic!("failed to convert Unix socket listener for {:?}: {}", path, e);
                }
            },
            Err(e) => {
                tracing::error!(path = ?path, error = ?e, "failed to bind Unix socket listener");
                panic!("failed to bind Unix socket listener on {:?}: {}", path, e);
            }
        }
    }
    pub fn listen(mut self) -> Result<Listeners> {
        if self.listeners.is_empty() {
            match std::net::TcpListener::bind("127.0.0.1:0") {
                Ok(listener) => match Listener::try_from(listener) {
                    Ok(listener) => self.listeners.push(Box::new(listener)),
                    Err(e) => {
                        tracing::error!(error = ?e, "failed to convert default TCP listener");
                        panic!("failed to convert default listener: {}", e);
                    }
                },
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
    /// 等待任意一个底层监听器返回连接。
    ///
    /// 返回：
    /// - `Some(Ok((conn, peer)))`：成功接受连接；
    /// - `Some(Err(e))`：单次接受失败，调用者可记录日志后继续；
    /// - `None`：所有监听器已关闭，建议上层退出循环并进入关停阶段。
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

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_listener_from_tokio_tcp() {
        // 测试从 tokio TcpListener 转换
        let tokio_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = tokio_listener.local_addr().unwrap();

        let listener = Listener::from(tokio_listener);

        // 验证监听地址
        let local_addr = listener.local_addr().unwrap();
        match local_addr {
            SocketAddr::Tcp(socket_addr) => {
                assert_eq!(socket_addr, addr);
            }
            _ => panic!("Expected TCP socket address"),
        }
    }

    #[tokio::test]
    async fn test_listener_accept() {
        // 测试 Listener 的 accept 功能
        let tokio_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = tokio_listener.local_addr().unwrap();
        let listener = Listener::from(tokio_listener);

        // 启动一个客户端连接
        let client_handle =
            tokio::spawn(async move { tokio::net::TcpStream::connect(addr).await.unwrap() });

        // 接受连接
        let accept_result = listener.accept().await;
        assert!(accept_result.is_ok());

        let (_stream, peer_addr) = accept_result.unwrap();

        match peer_addr {
            SocketAddr::Tcp(_) => {}
            _ => panic!("Expected TCP peer address"),
        }
        client_handle.await.unwrap();
    }

    #[tokio::test]
    #[cfg(not(target_os = "windows"))]
    async fn test_unix_listener() {
        use std::fs;

        let socket_path = "/tmp/test_silent_listener.sock";
        let _ = fs::remove_file(socket_path);

        // 测试 Unix Socket listener
        let tokio_listener = tokio::net::UnixListener::bind(socket_path).unwrap();
        let listener = Listener::from(tokio_listener);

        // 验证监听地址
        let local_addr = listener.local_addr().unwrap();
        match local_addr {
            SocketAddr::Unix(_) => {}
            _ => panic!("Expected Unix socket address"),
        }

        // 清理
        let _ = fs::remove_file(socket_path);
    }

    #[test]
    fn test_listeners_builder_default() {
        // 测试 ListenersBuilder 默认构造
        let builder = ListenersBuilder::new();
        assert_eq!(builder.listeners.len(), 0);
    }

    #[tokio::test]
    async fn test_listeners_builder_listen_with_default() {
        // 测试空 builder 会自动绑定默认地址
        let builder = ListenersBuilder::new();
        let listeners = builder.listen().unwrap();

        // 应该有一个默认的监听器
        assert_eq!(listeners.local_addrs().len(), 1);

        // 默认监听器应该是 TCP
        match &listeners.local_addrs()[0] {
            SocketAddr::Tcp(addr) => {
                assert_eq!(addr.ip().to_string(), "127.0.0.1");
            }
            _ => panic!("Expected TCP address"),
        }
    }

    #[tokio::test]
    async fn test_listeners_builder_bind() {
        // 测试绑定指定地址（使用随机端口避免冲突）
        let mut builder = ListenersBuilder::new();
        builder.bind("127.0.0.1:0".parse().unwrap());

        assert_eq!(builder.listeners.len(), 1);
    }

    #[tokio::test]
    async fn test_local_addrs_slice_len() {
        let mut builder = ListenersBuilder::new();
        builder.bind("127.0.0.1:0".parse().unwrap());
        builder.bind("127.0.0.1:0".parse().unwrap());
        let listeners = builder.listen().unwrap();
        let addrs = listeners.local_addrs();
        assert!(addrs.len() >= 2);
    }

    #[tokio::test]
    #[cfg(feature = "tls")]
    async fn test_listener_tls_method_exists() {
        // 注意：这个测试仅验证 TLS 相关方法的存在性
        // 实际的 TLS 功能测试需要有效的证书文件
        let tokio_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let _listener = Listener::from(tokio_listener);

        // 验证 tls 方法存在（通过类型检查即可）
        let _: fn(Listener, tokio_rustls::TlsAcceptor) -> TlsListener = Listener::tls;
    }
}
