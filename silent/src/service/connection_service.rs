use crate::core::socket_addr::SocketAddr as CoreSocketAddr;
use crate::service::connection::BoxedConnection;
use std::error::Error as StdError;
use std::future::Future;
use std::pin::Pin;

pub type BoxError = Box<dyn StdError + Send + Sync>;
pub type ConnectionFuture = Pin<Box<dyn Future<Output = Result<(), BoxError>> + Send>>;

pub trait ConnectionService: Send + Sync + 'static {
    fn call(&self, stream: BoxedConnection, peer: CoreSocketAddr) -> ConnectionFuture;
}

impl<F, Fut> ConnectionService for F
where
    F: Send + Sync + 'static + Fn(BoxedConnection, CoreSocketAddr) -> Fut,
    Fut: Future<Output = Result<(), BoxError>> + Send + 'static,
{
    fn call(&self, stream: BoxedConnection, peer: CoreSocketAddr) -> ConnectionFuture {
        Box::pin((self)(stream, peer))
    }
}
