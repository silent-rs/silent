use std::future::Future;
use std::pin::Pin;

use hyper::service::Service as HyperService;
use hyper::{Request as HyperRequest, Response as HyperResponse};
use tracing::debug;

use crate::core::res_body::ResBody;
use crate::core::socket_addr::SocketAddr;
use crate::prelude::ReqBody;
use crate::server::protocol::Protocol;
use crate::server::protocol::hyper_http::HyperHttpProtocol;
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
    #[inline]
    pub fn handle(&self, mut req: Request) -> impl Future<Output = Response> + use<H> {
        let remote_addr = self.remote_addr.clone();
        let routes = self.routes.clone();
        req.set_remote(remote_addr);
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
        let request = HyperRequest::from_parts(parts, body.into());
        let request = HyperHttpProtocol::into_internal(request);
        debug!("Request: \n{:#?}", request);
        let response = self.handle(request);
        Box::pin(async move {
            let res = response.await;
            debug!("Response: \n{:?}", res);
            Ok(HyperHttpProtocol::from_internal(res))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::route::Route;

    #[tokio::test]
    async fn test_hyper_service_handler_basic() {
        let remote_addr = "127.0.0.1:0"
            .parse::<std::net::SocketAddr>()
            .unwrap()
            .into();
        // 使用空路由树（不做具体处理），只要能完整走一遍转换流程即可
        let routes = Route::new_root();
        let svc = HyperServiceHandler::new(remote_addr, routes);
        let req = hyper::Request::builder().body(()).unwrap();
        let _ = svc.call(req).await.unwrap();
    }
}
