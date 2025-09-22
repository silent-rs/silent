use crate::core::connection::Connection;
use crate::core::socket_addr::SocketAddr;
use crate::route::RouteTree;
use std::error::Error as StdError;
use std::future::Future;
use std::pin::Pin;

pub(crate) type TransportFuture<'a> =
    Pin<Box<dyn Future<Output = Result<(), Box<dyn StdError + Send + Sync>>> + Send + 'a>>;

pub(crate) trait HttpTransport: Send + Sync + 'static {
    fn serve<'a>(
        &'a self,
        stream: Box<dyn Connection + Send + Sync>,
        peer_addr: SocketAddr,
        routes: RouteTree,
    ) -> TransportFuture<'a>;
}

mod hyper_tokio;
pub use hyper_tokio::HyperTokioTransport;
mod async_io;
#[allow(unused_imports)]
pub use async_io::AsyncIoTransport;
