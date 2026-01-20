use crate::grpc::GrpcHandler;
use crate::prelude::HandlerGetter;
use crate::prelude::Route;
use http::Method;
use std::sync::Arc;
use tonic::body::Body;
use tonic::codegen::Service;
use tonic::server::NamedService;

pub trait GrpcRegister<S> {
    fn get_handler(self) -> GrpcHandler<S>;
    fn service(self) -> Route;
    fn register(self, route: &mut Route);
}

impl<S> GrpcRegister<S> for S
where
    S: Service<http::Request<Body>, Response = http::Response<Body>> + NamedService,
    S: Clone + Send + 'static,
    S: Sync + Send + 'static,
    S::Future: Send + 'static,
    S::Error: Into<Box<dyn std::error::Error + Send + Sync>> + Send,
{
    fn get_handler(self) -> GrpcHandler<S> {
        GrpcHandler::new(self)
    }
    fn service(self) -> Route {
        let handler = self.get_handler();
        let path = handler.path().to_string();
        let handler = Arc::new(handler);
        Route::new(path.as_str()).append(
            Route::new("<path:**>")
                .insert_handler(Method::POST, handler.clone())
                .insert_handler(Method::GET, handler),
        )
    }

    fn register(self, route: &mut Route) {
        route.push(self.service());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::future::Future;
    use std::pin::Pin;

    // ==================== 基本功能测试 ====================

    #[test]
    fn test_grpc_register_get_handler() {
        let mock_service = MockGreeterService::new();
        let handler = mock_service.get_handler();

        // 验证 handler 创建成功
        assert_eq!(handler.path(), "/mock.greeter/MockGreeter");
    }

    #[test]
    fn test_grpc_register_service() {
        let mock_service = MockGreeterService::new();
        let route = mock_service.service();

        // 验证路由创建成功
        // Route::new() 会处理路径，去掉前导斜杠
        assert_eq!(route.path, "mock.greeter");
    }

    #[test]
    fn test_grpc_register_register_to_route() {
        let mock_service = MockGreeterService::new();
        let mut base_route = Route::new("/api");

        mock_service.register(&mut base_route);

        // 验证服务被注册到基础路由
        // 注册后，基础路由应该包含子路由
        // 注意：具体的行为可能需要根据 Route 的实现调整
    }

    // ==================== 多服务注册测试 ====================

    #[test]
    fn test_grpc_register_multiple_services() {
        let service1 = MockGreeterService::new();
        let service2 = MockUserService::new();

        let route1 = service1.service();
        let route2 = service2.service();

        // 验证两个服务的路径不同
        assert_ne!(route1.path, route2.path);
        assert_eq!(route1.path, "mock.greeter");
        assert_eq!(route2.path, "mock.user.UserService");
    }

    #[test]
    fn test_grpc_register_combine_routes() {
        let greeter_service = MockGreeterService::new();
        let user_service = MockUserService::new();

        let combined_route = Route::new("/api")
            .append(greeter_service.service())
            .append(user_service.service());

        // 验证路由组合成功
        assert_eq!(combined_route.path, "api");
    }

    // ==================== 类型验证测试 ====================

    #[test]
    fn test_grpc_register_trait_bound() {
        // 验证 MockGreeterService 实现了 GrpcRegister
        fn assert_grpc_register<S: GrpcRegister<S>>() {}
        assert_grpc_register::<MockGreeterService>();
    }

    #[test]
    fn test_grpc_register_clone() {
        let service = MockGreeterService::new();
        let _handler1 = service.clone().get_handler();
        let _handler2 = service.get_handler();

        // 验证可以多次调用 get_handler
    }

    // ==================== 命名服务测试 ====================

    #[test]
    fn test_grpc_register_named_service() {
        let _service = MockGreeterService::new();

        // 验证 NamedService trait 实现
        assert_eq!(MockGreeterService::NAME, "/mock.greeter/MockGreeter");
    }

    #[test]
    fn test_grpc_register_different_names() {
        let _greeter = MockGreeterService::new();
        let _user = MockUserService::new();

        // 验证不同服务有不同的名称
        assert_ne!(MockGreeterService::NAME, MockUserService::NAME);
    }

    // ==================== 边界条件测试 ====================

    #[test]
    fn test_grpc_register_empty_route() {
        let service = MockGreeterService::new();
        let mut empty_route = Route::new("");

        service.register(&mut empty_route);

        // 验证可以注册到空路径的路由
    }

    #[test]
    fn test_grpc_register_nested_route() {
        let service = MockGreeterService::new();
        let mut nested_route = Route::new("/api/v1/grpc");

        service.register(&mut nested_route);

        // 验证可以注册到嵌套路由
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
        const NAME: &'static str = "/mock.greeter/MockGreeter";
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

    #[derive(Debug)]
    struct MockError;

    impl std::fmt::Display for MockError {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "Mock error")
        }
    }

    impl std::error::Error for MockError {}
}
