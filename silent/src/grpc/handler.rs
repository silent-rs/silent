use std::sync::Arc;

use super::utils::merge_grpc_response;
use crate::grpc::service::GrpcService;
use crate::{Handler, Response, SilentError};
use async_lock::Mutex;
use async_trait::async_trait;
use http::{HeaderValue, StatusCode, header};
use hyper::upgrade::OnUpgrade;
use hyper_util::rt::TokioExecutor;
use tonic::body::Body;
use tonic::codegen::Service;
use tonic::server::NamedService;
use tracing::{error, info};

trait GrpcRequestAdapter {
    fn into_grpc_request(self) -> http::Request<Body>;
}

impl GrpcRequestAdapter for crate::Request {
    fn into_grpc_request(self) -> http::Request<Body> {
        let (parts, body) = self.into_http().into_parts();
        http::Request::from_parts(parts, Body::new(body))
    }
}

#[derive(Clone)]
pub struct GrpcHandler<S> {
    inner: Arc<Mutex<S>>,
}

impl<S> GrpcHandler<S>
where
    S: Service<http::Request<Body>, Response = http::Response<Body>> + NamedService,
    S: Clone + Send + 'static,
    S: Sync + Send + 'static,
    S::Future: Send + 'static,
    S::Error: Into<Box<dyn std::error::Error + Send + Sync>> + Send,
{
    pub fn new(service: S) -> Self {
        Self {
            inner: Arc::new(Mutex::new(service)),
        }
    }
    pub fn path(&self) -> &str {
        S::NAME
    }
}

impl<S> From<S> for GrpcHandler<S>
where
    S: Service<http::Request<Body>, Response = http::Response<Body>> + NamedService,
    S: Clone + Send + 'static,
    S: Sync + Send + 'static,
    S::Future: Send + 'static,
    S::Error: Into<Box<dyn std::error::Error + Send + Sync>> + Send,
{
    fn from(service: S) -> Self {
        Self {
            inner: Arc::new(Mutex::new(service)),
        }
    }
}

#[async_trait]
impl<S> Handler for GrpcHandler<S>
where
    S: Service<http::Request<Body>, Response = http::Response<Body>>,
    S: Clone + Send + 'static,
    S: Sync + Send + 'static,
    S::Future: Send + 'static,
    S::Error: Into<Box<dyn std::error::Error + Send + Sync>> + Send,
{
    async fn call(&self, mut req: crate::Request) -> crate::Result<Response> {
        if let Some(on_upgrade) = req.extensions_mut().remove::<OnUpgrade>() {
            let handler = self.inner.clone();
            async_global_executor::spawn(async move {
                let conn = on_upgrade.await;
                if conn.is_err() {
                    error!("upgrade error: {:?}", conn.err());
                    return;
                }
                let upgraded_io = conn.unwrap();

                let http = hyper::server::conn::http2::Builder::new(TokioExecutor::new());
                match http
                    .serve_connection(upgraded_io, GrpcService::new(handler))
                    .await
                {
                    Ok(_) => info!("finished gracefully"),
                    Err(err) => error!("ERROR: {err}"),
                }
            })
            .detach();
            let mut res = Response::empty();
            res.set_status(StatusCode::SWITCHING_PROTOCOLS);
            res.headers_mut()
                .insert(header::UPGRADE, HeaderValue::from_static("h2c"));
            Ok(res)
        } else {
            let handler = self.inner.clone();
            let mut handler = handler.lock().await;
            let req = req.into_grpc_request();

            let grpc_res = handler.call(req).await.map_err(|e| {
                SilentError::business_error(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("grpc call failed: {}", e.into()),
                )
            })?;
            let mut res = Response::empty();
            merge_grpc_response(&mut res, grpc_res).await;

            Ok(res)
        }
    }
}
