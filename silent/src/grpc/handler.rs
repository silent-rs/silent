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

#[cfg(test)]
mod tests {
    use super::*;
    use std::future::Future;
    use std::pin::Pin;

    // ==================== 基本功能测试 ====================

    #[test]
    fn test_grpc_handler_new() {
        let mock_service = MockGreeterService::new();
        let handler = GrpcHandler::new(mock_service);

        // 验证 handler 创建成功
        assert_eq!(handler.path(), "/mock.greeter.Greeter");
    }

    #[test]
    fn test_grpc_handler_clone() {
        let mock_service = MockGreeterService::new();
        let handler = GrpcHandler::new(mock_service);
        let handler_clone = handler.clone();

        // 验证两个 handler 共享同一个 inner service
        assert!(Arc::ptr_eq(&handler.inner, &handler_clone.inner));
        assert_eq!(Arc::strong_count(&handler.inner), 2);
    }

    // ==================== From Trait 测试 ====================

    #[test]
    fn test_grpc_handler_from_service() {
        let mock_service = MockGreeterService::new();
        let handler: GrpcHandler<MockGreeterService> = GrpcHandler::from(mock_service);

        // 验证 From trait 实现
        assert_eq!(handler.path(), "/mock.greeter.Greeter");
    }

    #[test]
    fn test_grpc_handler_from_consistency() {
        let service1 = MockGreeterService::new();
        let service2 = service1.clone();

        let handler1 = GrpcHandler::new(service1);
        let handler2 = GrpcHandler::from(service2);

        // 验证 new() 和 from() 创建相同的 handler
        assert_eq!(handler1.path(), handler2.path());
    }

    // ==================== Path 方法测试 ====================

    #[test]
    fn test_grpc_handler_path() {
        let greeter = GrpcHandler::new(MockGreeterService::new());
        let user = GrpcHandler::new(MockUserService::new());

        // 验证不同服务有不同的路径
        assert_eq!(greeter.path(), "/mock.greeter.Greeter");
        assert_eq!(user.path(), "/mock.user.UserService");
        assert_ne!(greeter.path(), user.path());
    }

    #[test]
    fn test_grpc_handler_path_static() {
        let handler = GrpcHandler::new(MockGreeterService::new());

        // 验证 path() 返回静态字符串引用
        let path = handler.path();
        assert_eq!(path, MockGreeterService::NAME);
    }

    // ==================== 类型验证测试 ====================

    #[test]
    fn test_grpc_handler_send_sync() {
        // 验证 GrpcHandler 实现 Send 和 Sync
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<GrpcHandler<MockGreeterService>>();
    }

    #[test]
    fn test_grpc_handler_clone_trait() {
        // 验证 GrpcHandler 实现 Clone
        let handler = GrpcHandler::new(MockGreeterService::new());
        let _ = handler.clone();
    }

    #[test]
    fn test_grpc_handler_size() {
        let handler = GrpcHandler::new(MockGreeterService::new());

        // GrpcHandler 只包含 Arc<Mutex<S>>
        assert_eq!(
            std::mem::size_of_val(&handler),
            std::mem::size_of::<Arc<Mutex<MockGreeterService>>>()
        );
    }

    // ==================== GrpcRequestAdapter 测试 ====================

    #[test]
    fn test_grpc_request_adapter() {
        let silent_req = crate::Request::empty();
        let grpc_req = silent_req.into_grpc_request();

        // 验证请求转换成功
        // Request::empty() 的默认方法是 GET
        assert_eq!(grpc_req.method(), http::Method::GET);
        assert_eq!(grpc_req.version(), http::Version::HTTP_11);
    }

    #[test]
    fn test_grpc_request_adapter_with_headers() {
        let mut silent_req = crate::Request::empty();
        silent_req
            .headers_mut()
            .insert("content-type", "application/grpc".parse().unwrap());
        silent_req
            .headers_mut()
            .insert("grpc-acceptance-encoding", "gzip".parse().unwrap());

        let grpc_req = silent_req.into_grpc_request();

        // 验证 headers 被保留
        assert_eq!(
            grpc_req.headers().get("content-type").unwrap(),
            "application/grpc"
        );
        assert_eq!(
            grpc_req.headers().get("grpc-acceptance-encoding").unwrap(),
            "gzip"
        );
    }

    // ==================== Arc 共享测试 ====================

    #[test]
    fn test_grpc_handler_arc_sharing() {
        let service = MockGreeterService::new();
        let handler1 = GrpcHandler::new(service.clone());
        let _handler2 = GrpcHandler::new(service);
        let handler3 = handler1.clone();

        // 验证 Arc 计数正确
        // handler1 和 handler3 共享同一个 Arc（计数为 2）
        // handler2 有独立的 Arc（计数为 1）
        assert_eq!(Arc::strong_count(&handler1.inner), 2);
        assert!(Arc::ptr_eq(&handler1.inner, &handler3.inner));
    }

    // ==================== 边界条件测试 ====================

    #[test]
    fn test_grpc_handler_empty_service_name() {
        let handler = GrpcHandler::new(MockEmptyService::new());

        // 验证空服务名称也能正常工作
        assert_eq!(handler.path(), "");
    }

    #[test]
    fn test_grpc_handler_long_service_name() {
        let handler = GrpcHandler::new(MockLongNameService::new());

        // 验证长服务名称能正常工作
        assert_eq!(
            handler.path(),
            "/very.long.service.name.with.many.parts.MockLongNameService"
        );
    }

    // ==================== Mock Service 实现 ====================

    #[derive(Clone)]
    struct MockGreeterService {
        _private: (),
    }

    impl MockGreeterService {
        fn new() -> Self {
            Self { _private: () }
        }
    }

    impl NamedService for MockGreeterService {
        const NAME: &'static str = "/mock.greeter.Greeter";
    }

    impl Service<http::Request<Body>> for MockGreeterService {
        type Response = http::Response<Body>;
        type Error = MockError;
        type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

        fn poll_ready(
            &mut self,
            _cx: &mut std::task::Context<'_>,
        ) -> std::task::Poll<Result<(), Self::Error>> {
            std::task::Poll::Ready(Ok(()))
        }

        fn call(&mut self, _req: http::Request<Body>) -> Self::Future {
            Box::pin(async move {
                Ok(http::Response::builder()
                    .status(http::StatusCode::OK)
                    .body(Body::empty())
                    .unwrap())
            })
        }
    }

    #[derive(Clone)]
    struct MockUserService {
        _private: (),
    }

    impl MockUserService {
        fn new() -> Self {
            Self { _private: () }
        }
    }

    impl NamedService for MockUserService {
        const NAME: &'static str = "/mock.user.UserService";
    }

    impl Service<http::Request<Body>> for MockUserService {
        type Response = http::Response<Body>;
        type Error = MockError;
        type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

        fn poll_ready(
            &mut self,
            _cx: &mut std::task::Context<'_>,
        ) -> std::task::Poll<Result<(), Self::Error>> {
            std::task::Poll::Ready(Ok(()))
        }

        fn call(&mut self, _req: http::Request<Body>) -> Self::Future {
            Box::pin(async move {
                Ok(http::Response::builder()
                    .status(http::StatusCode::OK)
                    .body(Body::empty())
                    .unwrap())
            })
        }
    }

    #[derive(Clone)]
    struct MockEmptyService {
        _private: (),
    }

    impl MockEmptyService {
        fn new() -> Self {
            Self { _private: () }
        }
    }

    impl NamedService for MockEmptyService {
        const NAME: &'static str = "";
    }

    impl Service<http::Request<Body>> for MockEmptyService {
        type Response = http::Response<Body>;
        type Error = MockError;
        type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

        fn poll_ready(
            &mut self,
            _cx: &mut std::task::Context<'_>,
        ) -> std::task::Poll<Result<(), Self::Error>> {
            std::task::Poll::Ready(Ok(()))
        }

        fn call(&mut self, _req: http::Request<Body>) -> Self::Future {
            Box::pin(async move {
                Ok(http::Response::builder()
                    .status(http::StatusCode::OK)
                    .body(Body::empty())
                    .unwrap())
            })
        }
    }

    #[derive(Clone)]
    struct MockLongNameService {
        _private: (),
    }

    impl MockLongNameService {
        fn new() -> Self {
            Self { _private: () }
        }
    }

    impl NamedService for MockLongNameService {
        const NAME: &'static str = "/very.long.service.name.with.many.parts.MockLongNameService";
    }

    impl Service<http::Request<Body>> for MockLongNameService {
        type Response = http::Response<Body>;
        type Error = MockError;
        type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

        fn poll_ready(
            &mut self,
            _cx: &mut std::task::Context<'_>,
        ) -> std::task::Poll<Result<(), Self::Error>> {
            std::task::Poll::Ready(Ok(()))
        }

        fn call(&mut self, _req: http::Request<Body>) -> Self::Future {
            Box::pin(async move {
                Ok(http::Response::builder()
                    .status(http::StatusCode::OK)
                    .body(Body::empty())
                    .unwrap())
            })
        }
    }

    #[derive(Debug)]
    struct MockError;

    impl std::fmt::Display for MockError {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "Mock error")
        }
    }

    impl std::error::Error for MockError {}
}
