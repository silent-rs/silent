use crate::core::socket_addr::SocketAddr;
use std::io;
use std::io::Error;
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio::net::{TcpStream, UnixStream};

pub enum Stream {
    TcpStream(TcpStream),
    UnixStream(UnixStream),
}

impl Stream {
    pub fn peer_addr(&self) -> io::Result<SocketAddr> {
        match self {
            Stream::TcpStream(s) => Ok(s.peer_addr()?.into()),
            Stream::UnixStream(s) => Ok(SocketAddr::Unix(s.peer_addr()?.into())),
        }
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
            Stream::UnixStream(s) => Pin::new(s).poll_write(cx, buf),
        }
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Error>> {
        match self.get_mut() {
            Stream::TcpStream(s) => Pin::new(s).poll_flush(cx),
            Stream::UnixStream(s) => Pin::new(s).poll_flush(cx),
        }
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Error>> {
        match self.get_mut() {
            Stream::TcpStream(s) => Pin::new(s).poll_shutdown(cx),
            Stream::UnixStream(s) => Pin::new(s).poll_shutdown(cx),
        }
    }
}

impl Unpin for Stream {}
