use std::pin::Pin;
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};

pub struct QuicConnection {
    connecting: Option<quinn::Incoming>,
}
impl QuicConnection {
    pub(crate) fn new(connecting: quinn::Incoming) -> Self {
        Self {
            connecting: Some(connecting),
        }
    }
    pub(crate) fn into_incoming(mut self) -> quinn::Incoming {
        self.connecting.take().expect("connecting available")
    }
}
impl AsyncRead for QuicConnection {
    fn poll_read(
        self: Pin<&mut Self>,
        _cx: &mut std::task::Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        buf.clear();
        std::task::Poll::Ready(Ok(()))
    }
}
impl AsyncWrite for QuicConnection {
    fn poll_write(
        self: Pin<&mut Self>,
        _cx: &mut std::task::Context<'_>,
        _buf: &[u8],
    ) -> std::task::Poll<std::io::Result<usize>> {
        std::task::Poll::Ready(Ok(0))
    }
    fn poll_flush(
        self: Pin<&mut Self>,
        _cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        std::task::Poll::Ready(Ok(()))
    }
    fn poll_shutdown(
        self: Pin<&mut Self>,
        _cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        std::task::Poll::Ready(Ok(()))
    }
}
