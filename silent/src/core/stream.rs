use std::io;
use std::io::Error;
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio::net::TcpStream;
#[cfg(not(target_os = "windows"))]
use tokio::net::UnixStream;

pub(crate) enum Stream {
    TcpStream(TcpStream),
    #[cfg(not(target_os = "windows"))]
    UnixStream(UnixStream),
}

// NOTE: 如需 peer_addr，请在上层保存并传递 SocketAddr。

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
