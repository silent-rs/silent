use super::HttpTransport;
use crate::core::connection::Connection;
use crate::core::socket_addr::SocketAddr;
use crate::handler::Handler;
use crate::service::hyper_service::HyperServiceHandler;
use hyper_util::rt::{TokioExecutor, TokioIo};
use hyper_util::server::conn::auto::Builder;
use std::error::Error as StdError;
use std::future::Future;
use std::pin::Pin;
// 使用自实现的 futures-io -> tokio-io 适配，避免 tokio-util 依赖

pub struct HyperTokioTransport {
    builder: Builder<TokioExecutor>,
}

impl HyperTokioTransport {
    pub fn new() -> Self {
        Self {
            builder: Builder::new(TokioExecutor::new()),
        }
    }
}

impl Default for HyperTokioTransport {
    fn default() -> Self {
        Self::new()
    }
}

impl HttpTransport for HyperTokioTransport {
    fn serve<'a>(
        &'a self,
        stream: Box<dyn Connection + Send>,
        peer_addr: SocketAddr,
        routes: std::sync::Arc<dyn Handler>,
    ) -> Pin<Box<dyn Future<Output = Result<(), Box<dyn StdError + Send + Sync>>> + Send + 'a>>
    {
        // Adapt futures-io Connection back to tokio-io for Hyper
        let io_tokio = FuturesAsTokio(stream);
        let io = TokioIo::new(io_tokio);
        let fut = self
            .builder
            .serve_connection_with_upgrades(io, HyperServiceHandler::new(peer_addr, routes));
        Box::pin(fut)
    }
}

// futures-io -> tokio-io 适配器
struct FuturesAsTokio<T>(T);

impl<T> tokio::io::AsyncRead for FuturesAsTokio<T>
where
    T: futures::io::AsyncRead + Unpin + Send,
{
    fn poll_read(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        let unfilled = buf.initialize_unfilled();
        match std::pin::Pin::new(&mut self.0).poll_read(cx, unfilled) {
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
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &[u8],
    ) -> std::task::Poll<std::io::Result<usize>> {
        std::pin::Pin::new(&mut self.0).poll_write(cx, buf)
    }

    fn poll_flush(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        std::pin::Pin::new(&mut self.0).poll_flush(cx)
    }

    fn poll_shutdown(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        std::pin::Pin::new(&mut self.0).poll_close(cx)
    }
}
