use crate::core::connection::Connection;
use crate::core::socket_addr::SocketAddr;
use crate::handler::Handler;
use std::error::Error as StdError;
use std::future::Future;
use std::pin::Pin;

pub type TransportFuture<'a> =
    Pin<Box<dyn Future<Output = Result<(), Box<dyn StdError + Send + Sync>>> + Send + 'a>>;

pub trait HttpTransport: Send + Sync + 'static {
    fn serve<'a>(
        &'a self,
        stream: Box<dyn Connection + Send>,
        peer_addr: SocketAddr,
        routes: std::sync::Arc<dyn Handler>,
    ) -> TransportFuture<'a>;

    /// 指示是否需要基于 Tokio 的监听/接入流程。
    /// 默认返回 true；基于 async-io 的实现应返回 false，以便 Server 采用 async-io 接入。
    fn requires_tokio(&self) -> bool {
        true
    }
}

mod hyper_tokio;
pub use hyper_tokio::HyperTokioTransport;
pub mod async_io;
pub use async_io::AsyncIoTransport;
