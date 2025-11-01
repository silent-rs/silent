use crate::core::socket_addr::SocketAddr;
use std::io;
use std::io::Error;
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio::net::TcpStream;
#[cfg(not(target_os = "windows"))]
use tokio::net::UnixStream;

pub enum Stream {
    TcpStream(TcpStream),
    #[cfg(not(target_os = "windows"))]
    UnixStream(UnixStream),
}

impl Stream {
    pub fn peer_addr(&self) -> io::Result<SocketAddr> {
        match self {
            Stream::TcpStream(s) => Ok(s.peer_addr()?.into()),
            #[cfg(not(target_os = "windows"))]
            Stream::UnixStream(s) => Ok(SocketAddr::Unix(s.peer_addr()?.into())),
        }
    }

    /// 判断是否为 TCP 流
    pub fn is_tcp(&self) -> bool {
        matches!(self, Stream::TcpStream(_))
    }

    /// 判断是否为 Unix Socket 流（仅非 Windows 平台）
    #[cfg(not(target_os = "windows"))]
    pub fn is_unix(&self) -> bool {
        matches!(self, Stream::UnixStream(_))
    }
}

impl AsyncRead for Stream {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        match self.get_mut() {
            Stream::TcpStream(s) => Pin::new(s).poll_read(cx, buf),
            #[cfg(not(target_os = "windows"))]
            Stream::UnixStream(s) => Pin::new(s).poll_read(cx, buf),
        }
    }
}

impl AsyncWrite for Stream {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<Result<usize, Error>> {
        match self.get_mut() {
            Stream::TcpStream(s) => Pin::new(s).poll_write(cx, buf),
            #[cfg(not(target_os = "windows"))]
            Stream::UnixStream(s) => Pin::new(s).poll_write(cx, buf),
        }
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Error>> {
        match self.get_mut() {
            Stream::TcpStream(s) => Pin::new(s).poll_flush(cx),
            #[cfg(not(target_os = "windows"))]
            Stream::UnixStream(s) => Pin::new(s).poll_flush(cx),
        }
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Error>> {
        match self.get_mut() {
            Stream::TcpStream(s) => Pin::new(s).poll_shutdown(cx),
            #[cfg(not(target_os = "windows"))]
            Stream::UnixStream(s) => Pin::new(s).poll_shutdown(cx),
        }
    }
}

impl Unpin for Stream {}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::net::TcpListener;

    #[tokio::test]
    async fn test_stream_is_tcp() {
        // 创建一个 TCP 连接
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let server_handle = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            stream
        });

        let client_stream = TcpStream::connect(addr).await.unwrap();
        let stream = Stream::TcpStream(client_stream);

        // 测试 is_tcp() 返回 true
        assert!(stream.is_tcp());

        // 清理
        server_handle.await.unwrap();
    }

    #[tokio::test]
    #[cfg(not(target_os = "windows"))]
    async fn test_stream_is_unix() {
        use std::fs;
        use tokio::net::UnixListener;

        // 创建临时 Unix Socket 路径
        let socket_path = "/tmp/test_silent_unix_socket.sock";
        let _ = fs::remove_file(socket_path);

        let listener = UnixListener::bind(socket_path).unwrap();

        let server_handle = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            stream
        });

        let client_stream = UnixStream::connect(socket_path).await.unwrap();
        let stream = Stream::UnixStream(client_stream);

        // 测试 is_unix() 返回 true
        assert!(stream.is_unix());
        // 测试 is_tcp() 返回 false
        assert!(!stream.is_tcp());

        // 清理
        server_handle.await.unwrap();
        let _ = fs::remove_file(socket_path);
    }

    #[tokio::test]
    async fn test_tcp_stream_not_unix() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let server_handle = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            stream
        });

        let client_stream = TcpStream::connect(addr).await.unwrap();
        let stream = Stream::TcpStream(client_stream);

        // TCP 流的 is_tcp() 应该返回 true
        assert!(stream.is_tcp());

        // 在非 Windows 平台，TCP 流的 is_unix() 应该返回 false
        #[cfg(not(target_os = "windows"))]
        assert!(!stream.is_unix());

        server_handle.await.unwrap();
    }
}
