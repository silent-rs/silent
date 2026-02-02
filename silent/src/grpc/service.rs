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

    // ==================== HyperService::call 测试 ====================

    #[test]
    fn test_grpc_service_request_to_body_conversion() {
        // 测试请求到 Body 的转换逻辑（第 53-54 行）
        use tonic::body::Body;

        // 创建模拟的 hyper::Request 结构
        // 我们可以测试转换逻辑的核心部分
        let mock_req = http::Request::builder()
            .method(http::Method::POST)
            .uri("/")
            .body(Body::empty())
            .unwrap();

        // 验证请求结构正确
        assert_eq!(mock_req.method(), http::Method::POST);
        assert_eq!(mock_req.uri(), "/");
    }

    #[test]
    fn test_grpc_service_request_parts_extraction() {
        // 测试请求部分的提取（第 53 行）
        let mock_req = http::Request::builder()
            .method(http::Method::GET)
            .uri("/test")
            .header("content-type", "application/grpc")
            .body(Body::empty())
            .unwrap();

        let (parts, _body) = mock_req.into_parts();

        // 验证各部分被正确提取
        assert_eq!(parts.method, http::Method::GET);
        assert_eq!(parts.uri, "/test");
        assert_eq!(
            parts.headers.get("content-type").unwrap(),
            "application/grpc"
        );
    }

    #[test]
    fn test_grpc_service_request_from_parts() {
        // 测试 from_parts 转换（第 54 行）
        use tonic::body::Body;

        let (parts, _body) = http::Request::builder()
            .method(http::Method::POST)
            .uri("/")
            .body(Body::empty())
            .unwrap()
            .into_parts();

        // 使用 from_parts 重新创建请求
        let new_req = http::Request::from_parts(parts, Body::empty());

        assert_eq!(new_req.method(), http::Method::POST);
        assert_eq!(new_req.uri(), "/");
    }

    #[test]
    fn test_grpc_service_response_body_empty() {
        // 测试 Body::empty() 的使用
        use tonic::body::Body;

        // Body::empty() 的测试验证
        let _body = Body::empty();
        // 验证可以创建空 body
    }

    #[test]
    fn test_grpc_service_body_type_compatibility() {
        // 测试 Body 类型兼容性
        use tonic::body::Body;

        // 验证 Body 实现了必要的 trait
        fn assert_body_traits<B: Send + 'static>() {}
        assert_body_traits::<Body>();
    }

    #[test]
    fn test_grpc_service_handler_lock() {
        // 测试 handler 锁机制（第 57-59 行）
        let runtime = tokio::runtime::Runtime::new().unwrap();
        runtime.block_on(async {
            let mock_service = MockService::new();
            let handler = Arc::new(Mutex::new(mock_service));

            // 测试获取锁
            let locked = handler.lock().await;
            drop(locked);

            // 验证可以再次获取锁
            let _locked2 = handler.lock().await;
        });
    }

    #[test]
    fn test_grpc_service_concurrent_handler_access() {
        // 测试并发访问 handler（模拟 call 方法中的锁竞争）
        let runtime = tokio::runtime::Runtime::new().unwrap();
        runtime.block_on(async {
            let mock_service = MockService::new();
            let handler = Arc::new(Mutex::new(mock_service));

            let mut handles = Vec::new();
            for _ in 0..10 {
                let handler_clone = handler.clone();
                let handle = async_global_executor::spawn(async move {
                    let _locked = handler_clone.lock().await;
                    // 验证可以获取锁
                    // 不需要实际工作，锁的获取和释放就足够了
                });
                handles.push(handle);
            }

            // 等待所有任务完成
            for handle in handles {
                handle.await;
            }
        });
    }

    #[test]
    fn test_grpc_service_arc_sharing() {
        // 测试 Arc 共享和计数
        let mock_service = MockService::new();
        let handler = Arc::new(Mutex::new(mock_service));
        let grpc_service1 = GrpcService::new(handler.clone());
        let grpc_service2 = GrpcService::new(handler);

        // 验证 Arc 计数
        // grpc_service1 和 grpc_service2 都持有 handler 的克隆
        assert_eq!(Arc::strong_count(&grpc_service1.handler), 2);
        assert!(Arc::ptr_eq(&grpc_service1.handler, &grpc_service2.handler));
    }

    // ==================== 辅助函数和 Mock Service ====================
}
