use super::HttpTransport;
use crate::core::connection::Connection;
use crate::core::socket_addr::SocketAddr;
use crate::route::RouteTree;
use crate::service::hyper_service::HyperServiceHandler;
use hyper_util::rt::{TokioExecutor, TokioIo};
use hyper_util::server::conn::auto::Builder;
use std::error::Error as StdError;
use std::future::Future;
use std::pin::Pin;
use tokio_util::compat::FuturesAsyncReadCompatExt;

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
        stream: Box<dyn Connection + Send + Sync>,
        peer_addr: SocketAddr,
        routes: RouteTree,
    ) -> Pin<Box<dyn Future<Output = Result<(), Box<dyn StdError + Send + Sync>>> + Send + 'a>>
    {
        // Adapt futures-io Connection back to tokio-io for Hyper
        let io_tokio = stream.compat();
        let io = TokioIo::new(io_tokio);
        let fut = self
            .builder
            .serve_connection_with_upgrades(io, HyperServiceHandler::new(peer_addr, routes));
        Box::pin(fut)
    }
}
