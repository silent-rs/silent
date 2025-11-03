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
    async fn test_stream_tcp_rw_and_peer() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let server = tokio::spawn(async move {
            let (stream, _peer) = listener.accept().await.unwrap();
            let mut s = Stream::TcpStream(stream);
            let pa = s.peer_addr().unwrap();
            match pa {
                SocketAddr::Tcp(_) => {}
                _ => panic!("expected tcp socket addr"),
            }
            let mut buf = [0u8; 2];
            tokio::io::AsyncReadExt::read_exact(&mut s, &mut buf)
                .await
                .unwrap();
            assert_eq!(&buf, b"hi");
            tokio::io::AsyncWriteExt::write_all(&mut s, b"ok")
                .await
                .unwrap();
        });

        let mut client = tokio::net::TcpStream::connect(addr).await.unwrap();
        tokio::io::AsyncWriteExt::write_all(&mut client, b"hi")
            .await
            .unwrap();
        let mut buf = [0u8; 2];
        tokio::io::AsyncReadExt::read_exact(&mut client, &mut buf)
            .await
            .unwrap();
        assert_eq!(&buf, b"ok");
        server.await.unwrap();
    }

    #[tokio::test]
    #[cfg(not(target_os = "windows"))]
    async fn test_stream_unix_peer_and_flags() {
        use std::fs;
        use tokio::net::UnixListener;
        let path = "/tmp/test_silent_unix_rw.sock";
        let _ = fs::remove_file(path);
        let listener = UnixListener::bind(path).unwrap();
        let server = tokio::spawn(async move {
            let (stream, _addr) = listener.accept().await.unwrap();
            let mut s = Stream::UnixStream(stream);
            assert!(s.is_unix());
            assert!(!s.is_tcp());
            tokio::io::AsyncWriteExt::write_all(&mut s, b"ux")
                .await
                .unwrap();
        });
        let mut client = tokio::net::UnixStream::connect(path).await.unwrap();
        let mut buf = [0u8; 2];
        tokio::io::AsyncReadExt::read_exact(&mut client, &mut buf)
            .await
            .unwrap();
        assert_eq!(&buf, b"ux");
        server.await.unwrap();
        let _ = fs::remove_file(path);
    }
}
