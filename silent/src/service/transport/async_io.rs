use super::HttpTransport;
use crate::core::connection::Connection;
use crate::core::socket_addr::SocketAddr;
use crate::route::RouteTree;
use std::error::Error as StdError;
use std::future::Future;
use std::pin::Pin;

/// 占位的 Async-IO HTTP 传输实现。
///
/// 注意：当前仅作为占位，返回未实现错误。
/// 后续将以 async-io/async-net/async-h1 实现真正的 HTTP 编解码与服务流程。
#[allow(dead_code)]
pub struct AsyncIoTransport;

#[allow(dead_code)]
impl AsyncIoTransport {
    pub fn new() -> Self {
        Self
    }
}

impl Default for AsyncIoTransport {
    fn default() -> Self {
        Self::new()
    }
}

impl HttpTransport for AsyncIoTransport {
    fn serve<'a>(
        &'a self,
        _stream: Box<dyn Connection + Send + Sync>,
        _peer_addr: SocketAddr,
        _routes: RouteTree,
    ) -> Pin<Box<dyn Future<Output = Result<(), Box<dyn StdError + Send + Sync>>> + Send + 'a>>
    {
        Box::pin(async { Err("AsyncIoTransport not implemented yet".into()) })
    }
}
