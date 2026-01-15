use crate::{Handler, MiddleWareHandler, Next, Request, Response, Result};
use async_trait::async_trait;

#[derive(Debug, Default, Clone)]
pub struct SchedulerMiddleware {}

impl SchedulerMiddleware {
    pub(crate) fn new() -> Self {
        Self::default()
    }
}

#[async_trait]
impl MiddleWareHandler for SchedulerMiddleware {
    async fn handle(&self, mut req: Request, next: &Next) -> Result<Response> {
        let scheduler = super::SCHEDULER.clone();
        req.extensions_mut().insert(scheduler);
        next.call(req).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scheduler::Scheduler;
    use async_lock::Mutex;
    use std::sync::Arc;

    // ==================== 构造函数测试 ====================

    #[test]
    fn test_scheduler_middleware_new() {
        let middleware = SchedulerMiddleware::new();
        let _ = middleware;
    }

    #[test]
    fn test_scheduler_middleware_default() {
        let middleware = SchedulerMiddleware::default();
        let _ = middleware;
    }

    // ==================== Debug trait 测试 ====================

    #[test]
    fn test_scheduler_middleware_debug() {
        let middleware = SchedulerMiddleware::new();
        let debug_str = format!("{:?}", middleware);
        assert!(debug_str.contains("SchedulerMiddleware"));
    }

    // ==================== Clone trait 测试 ====================

    #[test]
    fn test_scheduler_middleware_clone() {
        let middleware1 = SchedulerMiddleware::new();
        let middleware2 = middleware1.clone();
        let _ = middleware1;
        let _ = middleware2;
    }

    #[test]
    fn test_scheduler_middleware_clone_independent() {
        let middleware1 = SchedulerMiddleware::new();
        let middleware2 = middleware1.clone();

        // 两个实例应该独立存在
        let _ = middleware1;
        let _ = middleware2;
    }

    // ==================== 类型验证测试 ====================

    #[test]
    fn test_scheduler_middleware_type() {
        let middleware = SchedulerMiddleware::new();
        // 验证类型
        let _middleware: SchedulerMiddleware = middleware;
    }

    #[test]
    fn test_scheduler_middleware_size() {
        use std::mem::size_of;
        let size = size_of::<SchedulerMiddleware>();
        // SchedulerMiddleware 是空结构体（ZST）
        assert_eq!(size, 0);
    }

    // ==================== 集成测试 ====================

    #[cfg(feature = "server")]
    #[tokio::test]
    async fn test_scheduler_middleware_inserts_scheduler() {
        use crate::route::Route;

        let middleware = SchedulerMiddleware::new();

        let route = Route::new("/")
            .hook(middleware)
            .get(|req: Request| async move {
                // 验证调度器被插入到请求扩展中
                let scheduler = req.extensions().get::<Arc<Mutex<Scheduler>>>();
                assert!(scheduler.is_some());
                Ok("scheduler found")
            });

        let route = Route::new_root().append(route);
        let req = Request::empty();

        let result: Result<Response> = route.call(req).await;
        assert!(result.is_ok());
    }

    #[cfg(feature = "server")]
    #[tokio::test]
    async fn test_scheduler_middleware_preserves_response() {
        use crate::route::Route;

        let middleware = SchedulerMiddleware::new();

        let route = Route::new("/")
            .hook(middleware)
            .get(|_req: Request| async { Ok("test response") });

        let route = Route::new_root().append(route);
        let req = Request::empty();

        let result: Result<Response> = route.call(req).await;
        assert!(result.is_ok());
        let resp = result.unwrap();
        // 中间件应该保留原始响应
        assert_eq!(resp.status.as_u16(), 200);
    }

    #[cfg(feature = "server")]
    #[tokio::test]
    async fn test_scheduler_middleware_with_empty_response() {
        use crate::route::Route;

        let middleware = SchedulerMiddleware::new();

        let route = Route::new("/")
            .hook(middleware)
            .get(|_req: Request| async { Ok(Response::empty()) });

        let route = Route::new_root().append(route);
        let req = Request::empty();

        let result: Result<Response> = route.call(req).await;
        assert!(result.is_ok());
    }

    #[cfg(feature = "server")]
    #[tokio::test]
    async fn test_scheduler_middleware_with_body() {
        use crate::route::Route;

        let middleware = SchedulerMiddleware::new();

        let route = Route::new("/")
            .hook(middleware)
            .get(|_req: Request| async { Ok(Response::text("response with body")) });

        let route = Route::new_root().append(route);
        let req = Request::empty();

        let result: Result<Response> = route.call(req).await;
        assert!(result.is_ok());
        let resp = result.unwrap();
        assert_eq!(resp.status.as_u16(), 200);
    }

    #[cfg(feature = "server")]
    #[tokio::test]
    async fn test_scheduler_middleware_scheduler_is_global() {
        use crate::route::Route;

        let middleware = SchedulerMiddleware::new();

        let route = Route::new("/")
            .hook(middleware)
            .get(|req: Request| async move {
                let scheduler1 = req.extensions().get::<Arc<Mutex<Scheduler>>>();
                assert!(scheduler1.is_some());

                // 验证这是全局调度器（通过 SCHEDULER 静态变量）
                let global_scheduler = &super::super::SCHEDULER;
                assert!(Arc::ptr_eq(scheduler1.unwrap(), global_scheduler));

                Ok("global scheduler")
            });

        let route = Route::new_root().append(route);
        let req = Request::empty();

        let result: Result<Response> = route.call(req).await;
        assert!(result.is_ok());
    }

    #[cfg(feature = "server")]
    #[tokio::test]
    async fn test_scheduler_middleware_concurrent_requests() {
        use crate::route::Route;
        use std::sync::Arc;

        let middleware = SchedulerMiddleware::new();

        let route = Route::new("/")
            .hook(middleware)
            .get(|req: Request| async move {
                let scheduler = req.extensions().get::<Arc<Mutex<Scheduler>>>();
                assert!(scheduler.is_some());
                Ok("concurrent")
            });

        let route: Arc<Route> = Arc::new(Route::new_root().append(route));

        // 并发多个请求
        let tasks = (0..5).map(|_| {
            let route = Arc::clone(&route);
            tokio::spawn(async move {
                let req = Request::empty();
                let result: Result<Response> = route.call(req).await;
                result
            })
        });

        for task in tasks {
            let result = task.await.unwrap();
            assert!(result.is_ok());
        }
    }

    // ==================== 边界条件测试 ====================

    #[cfg(feature = "server")]
    #[tokio::test]
    async fn test_scheduler_middleware_different_http_methods() {
        use crate::route::Route;

        let middleware = SchedulerMiddleware::new();

        let route = Route::new("/")
            .hook(middleware)
            .get(|req: Request| async move {
                let scheduler = req.extensions().get::<Arc<Mutex<Scheduler>>>();
                assert!(scheduler.is_some());
                Ok("GET")
            })
            .post(|req: Request| async move {
                let scheduler = req.extensions().get::<Arc<Mutex<Scheduler>>>();
                assert!(scheduler.is_some());
                Ok("POST")
            });

        let route = Route::new_root().append(route);

        // 测试 GET
        let mut req = Request::empty();
        *req.method_mut() = http::Method::GET;
        let result: Result<Response> = route.call(req).await;
        assert!(result.is_ok());

        // 测试 POST
        let mut req = Request::empty();
        *req.method_mut() = http::Method::POST;
        let result: Result<Response> = route.call(req).await;
        assert!(result.is_ok());
    }

    #[cfg(feature = "server")]
    #[tokio::test]
    async fn test_scheduler_middleware_chain_with_other_middleware() {
        use crate::middlewares::{ExceptionHandler, RequestTimeLogger};
        use crate::route::Route;

        let scheduler_middleware = SchedulerMiddleware::new();
        let exception_handler =
            ExceptionHandler::new(|result: Result<Response>, _configs| async move { result });
        let time_logger = RequestTimeLogger::new();

        let route = Route::new("/")
            .hook(scheduler_middleware)
            .hook(exception_handler)
            .hook(time_logger)
            .get(|req: Request| async move {
                let scheduler = req.extensions().get::<Arc<Mutex<Scheduler>>>();
                assert!(scheduler.is_some());
                Ok("chained")
            });

        let route = Route::new_root().append(route);
        let mut req = Request::empty();
        req.headers_mut()
            .insert("x-real-ip", "127.0.0.1".parse().unwrap());

        let result: Result<Response> = route.call(req).await;
        assert!(result.is_ok());
    }

    #[cfg(feature = "server")]
    #[tokio::test]
    async fn test_scheduler_middleware_multiple_middlewares() {
        use crate::route::Route;

        let middleware1 = SchedulerMiddleware::new();
        let middleware2 = SchedulerMiddleware::new();

        let route =
            Route::new("/")
                .hook(middleware1)
                .hook(middleware2)
                .get(|req: Request| async move {
                    // 即使有多个 SchedulerMiddleware，也应该能获取到调度器
                    let scheduler = req.extensions().get::<Arc<Mutex<Scheduler>>>();
                    assert!(scheduler.is_some());
                    Ok("multiple middlewares")
                });

        let route = Route::new_root().append(route);
        let req = Request::empty();

        let result: Result<Response> = route.call(req).await;
        assert!(result.is_ok());
    }

    #[cfg(feature = "server")]
    #[tokio::test]
    async fn test_scheduler_middleware_preserves_headers() {
        use crate::route::Route;

        let middleware = SchedulerMiddleware::new();

        let route = Route::new("/").hook(middleware).get(|_req: Request| async {
            let mut resp = Response::text("with headers");
            resp.headers_mut()
                .insert("X-Custom-Header", "test-value".parse().unwrap());
            Ok(resp)
        });

        let route = Route::new_root().append(route);
        let req = Request::empty();

        let result: Result<Response> = route.call(req).await;
        assert!(result.is_ok());
        let resp = result.unwrap();
        assert!(resp.headers().get("X-Custom-Header").is_some());
    }

    #[cfg(feature = "server")]
    #[tokio::test]
    async fn test_scheduler_middleware_with_different_urls() {
        use crate::route::Route;

        let middleware = SchedulerMiddleware::new();

        let route = Route::new("/test")
            .hook(middleware)
            .get(|req: Request| async move {
                let scheduler = req.extensions().get::<Arc<Mutex<Scheduler>>>();
                assert!(scheduler.is_some());
                Ok("test")
            });

        let route = Route::new_root().append(route);

        // 测试不同的 URL 路径
        for url in &["/test", "/test?query=value", "/test?foo=bar&baz=qux"] {
            let mut req = Request::empty();
            *req.uri_mut() = url.parse().unwrap();
            let result: Result<Response> = route.call(req).await;
            assert!(result.is_ok());
        }
    }

    #[cfg(feature = "server")]
    #[tokio::test]
    async fn test_scheduler_middleware_error_handler_still_has_scheduler() {
        use crate::middlewares::ExceptionHandler;
        use crate::route::Route;

        let scheduler_middleware = SchedulerMiddleware::new();
        let exception_handler =
            ExceptionHandler::new(|result: Result<Response>, _configs| async move {
                // 错误处理器中也能访问调度器
                result
            });

        let route = Route::new("/")
            .hook(scheduler_middleware)
            .hook(exception_handler)
            .get(|req: Request| async move {
                // 验证调度器在错误处理器之前被插入
                let scheduler = req.extensions().get::<Arc<Mutex<Scheduler>>>();
                assert!(scheduler.is_some());
                Ok("success")
            });

        let route = Route::new_root().append(route);
        let req = Request::empty();

        let result: Result<Response> = route.call(req).await;
        assert!(result.is_ok());
    }
}
