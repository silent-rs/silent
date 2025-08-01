use std::future::Future;
use std::pin::Pin;

use hyper::service::Service as HyperService;
use hyper::{Request as HyperRequest, Response as HyperResponse};

use crate::core::socket_addr::SocketAddr;
use crate::core::{adapt::RequestAdapt, adapt::ResponseAdapt, res_body::ResBody};
use crate::prelude::ReqBody;
use crate::{Handler, Request, Response};

#[doc(hidden)]
#[derive(Clone)]
pub struct HyperServiceHandler<H: Handler> {
    pub(crate) remote_addr: SocketAddr,
    pub(crate) routes: H,
}

impl<H: Handler + Clone> HyperServiceHandler<H> {
    #[inline]
    pub fn new(remote_addr: SocketAddr, routes: H) -> Self {
        Self {
            remote_addr,
            routes,
        }
    }
    /// Handle [`Request`] and returns [`Response`].
    #[inline]
    pub fn handle(&self, mut req: Request) -> impl Future<Output = Response> + use<H> {
        let Self {
            remote_addr,
            routes,
        } = self.clone();
        req.set_remote(remote_addr);
        let routes = routes.clone();
        async move { routes.call(req).await.unwrap_or_else(Into::into) }
    }
}

impl<B, H: Handler + Clone> HyperService<HyperRequest<B>> for HyperServiceHandler<H>
where
    B: Into<ReqBody>,
{
    type Response = HyperResponse<ResBody>;
    type Error = hyper::Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    #[inline]
    fn call(&self, req: HyperRequest<B>) -> Self::Future {
        let (parts, body) = req.into_parts();
        let req = HyperRequest::from_parts(parts, body.into()).tran_to_request();
        let response = self.handle(req);
        Box::pin(async move {
            let res = response.await;
            Ok(ResponseAdapt::tran_from_response(res))
        })
    }
}
#[cfg(test)]
mod tests {
    use crate::route::Route;

    use super::*;

    #[tokio::test]
    async fn test_handle_request() {
        // Arrange
        let remote_addr = "127.0.0.1:8080"
            .parse::<std::net::SocketAddr>()
            .unwrap()
            .into();
        let routes = Route::new_root(); // 创建新的根路由实例
        let hsh = HyperServiceHandler::new(remote_addr, routes);
        let req = hyper::Request::builder().body(()).unwrap(); // Assuming Request::new() creates a new instance of Request

        // Act
        let _ = hsh.call(req).await;
    }
}
