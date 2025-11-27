use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use async_lock::Mutex;
use hyper::body::Incoming;
use hyper::service::Service as HyperService;
use tonic::body::Body;
use tonic::codegen::Service;
use tracing::error;

#[doc(hidden)]
#[derive(Clone)]
pub struct GrpcService<S>
where
    S: Service<http::Request<Body>, Response = http::Response<Body>>,
    S: Clone + Send + 'static,
    S: Sync + Send + 'static,
    S::Future: Send + 'static,
    S::Error: Into<Box<dyn std::error::Error + Send + Sync>> + Send,
{
    pub(crate) handler: Arc<Mutex<S>>,
}

impl<S> GrpcService<S>
where
    S: Service<http::Request<Body>, Response = http::Response<Body>>,
    S: Clone + Send + 'static,
    S: Sync + Send + 'static,
    S::Future: Send + 'static,
    S::Error: Into<Box<dyn std::error::Error + Send + Sync>> + Send,
{
    #[inline]
    pub fn new(handler: Arc<Mutex<S>>) -> Self {
        Self { handler }
    }
}

impl<S> HyperService<hyper::Request<Incoming>> for GrpcService<S>
where
    S: Service<http::Request<Body>, Response = http::Response<Body>>,
    S: Clone + Send + 'static,
    S: Sync + Send + 'static,
    S::Future: Send + 'static,
    S::Error: Into<Box<dyn std::error::Error + Send + Sync>> + Send,
{
    type Response = http::Response<Body>;
    type Error = S::Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    #[inline]
    fn call(&self, req: hyper::Request<Incoming>) -> Self::Future {
        let (parts, body) = req.into_parts();
        let req = http::Request::from_parts(parts, Body::new(body));
        let handler = self.handler.clone();
        Box::pin(async move {
            let res = handler
                .lock()
                .await
                .call(req)
                .await
                .map_err(|e| {
                    error!("call grpc failed: {:?}", e.into());
                })
                .unwrap();
            Ok(res)
        })
    }
}
