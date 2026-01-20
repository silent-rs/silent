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

#[cfg(test)]
mod tests {
    use super::*;

    // ==================== 基本功能测试 ====================

    #[test]
    fn test_grpc_service_new() {
        let mock_service = MockService::new();
        let handler = Arc::new(Mutex::new(mock_service));
        let grpc_service = GrpcService::new(handler);

        // 验证服务创建成功
        // Arc 被移动到 GrpcService 中，所以计数为 1
        assert_eq!(Arc::strong_count(&grpc_service.handler), 1);
    }

    #[test]
    fn test_grpc_service_clone() {
        let mock_service = MockService::new();
        let handler = Arc::new(Mutex::new(mock_service));
        let grpc_service = GrpcService::new(handler.clone());
        let grpc_service_clone = grpc_service.clone();

        // 验证两个服务共享同一个 handler
        assert!(Arc::ptr_eq(
            &grpc_service.handler,
            &grpc_service_clone.handler
        ));
        assert_eq!(Arc::strong_count(&handler), 3);
    }

    // ==================== 类型验证测试 ====================

    #[test]
    fn test_grpc_service_send_sync() {
        // 验证 GrpcService 实现 Send 和 Sync
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<GrpcService<MockService>>();
    }

    #[test]
    fn test_grpc_service_clone_trait() {
        // 验证 GrpcService 实现 Clone
        let mock_service = MockService::new();
        let handler = Arc::new(Mutex::new(mock_service));
        let grpc_service = GrpcService::new(handler);

        let _ = grpc_service.clone();
    }

    #[test]
    fn test_grpc_service_size() {
        // 验证 GrpcService 的大小
        let mock_service = MockService::new();
        let handler = Arc::new(Mutex::new(mock_service));
        let grpc_service = GrpcService::new(handler);

        // GrpcService 只包含 Arc<Mutex<S>>
        assert_eq!(
            std::mem::size_of_val(&grpc_service),
            std::mem::size_of::<Arc<Mutex<MockService>>>()
        );
    }

    // ==================== Mock Service 实现 ====================

    #[derive(Clone)]
    struct MockService {
        _private: (),
    }

    impl MockService {
        fn new() -> Self {
            Self { _private: () }
        }
    }

    impl Service<http::Request<Body>> for MockService {
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
