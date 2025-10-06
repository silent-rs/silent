use super::connection::Connection;
use crate::core::socket_addr::SocketAddr;
use crate::route::RouteTree;
use crate::service::hyper_service::HyperServiceHandler;
use hyper_util::rt::{TokioExecutor, TokioIo};
use hyper_util::server::conn::auto::Builder;
use std::error::Error as StdError;

pub(crate) struct Serve<E = TokioExecutor> {
    pub(crate) routes: RouteTree,
    pub(crate) builder: Builder<E>,
}

impl Serve {
    pub(crate) fn new(routes: RouteTree) -> Self {
        Self {
            routes,
            builder: Builder::new(TokioExecutor::new()),
        }
    }
    pub(crate) async fn call<S: Connection + Send + Sync + 'static>(
        &self,
        stream: S,
        peer_addr: SocketAddr,
    ) -> Result<(), Box<dyn StdError + Send + Sync>> {
        let io = TokioIo::new(stream);
        self.builder
            .serve_connection_with_upgrades(
                io,
                HyperServiceHandler::new(peer_addr, self.routes.clone()),
            )
            .await
    }
}
