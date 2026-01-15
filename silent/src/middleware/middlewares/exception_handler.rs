use std::future::Future;
use std::sync::Arc;

use async_trait::async_trait;

use crate::{Configs, Handler, MiddleWareHandler, Next, Request, Response, Result};

/// ExceptionHandler 中间件
/// ```rust
/// use silent::prelude::*;
/// use silent::middlewares::{ExceptionHandler};
/// // Define a custom error handler function
/// let _ = ExceptionHandler::new(|res, _configs| async {res});
#[derive(Default, Clone)]
pub struct ExceptionHandler<F> {
    handler: Arc<F>,
}

impl<F, Fut, T> ExceptionHandler<F>
where
    Fut: Future<Output = Result<T>> + Send + 'static,
    F: Fn(Result<Response>, Configs) -> Fut + Send + Sync + 'static,
    T: Into<Response>,
{
    pub fn new(handler: F) -> Self {
        Self {
            handler: Arc::new(handler),
        }
    }
}

#[async_trait]
impl<F, Fut, T> MiddleWareHandler for ExceptionHandler<F>
where
    Fut: Future<Output = Result<T>> + Send + 'static,
    F: Fn(Result<Response>, Configs) -> Fut + Send + Sync + 'static,
    T: Into<Response>,
{
    async fn handle(&self, req: Request, next: &Next) -> Result<Response> {
        let configs = req.configs();
        self.handler.clone()(next.call(req).await, configs)
            .await
            .map(|r| r.into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    // ==================== 构造函数测试 ====================

    #[test]
    fn test_exception_handler_new() {
        let handler = ExceptionHandler::new(|result: Result<Response>, _configs| async {
            match result {
                Ok(resp) => Ok(resp),
                Err(_) => Ok(Response::text("error")),
            }
        });
        let _ = handler;
    }

    #[test]
    fn test_exception_handler_new_identity() {
        // 直接返回结果的处理器
        let handler = ExceptionHandler::new(|result: Result<Response>, _configs| async { result });
        let _ = handler;
    }

    #[test]
    fn test_exception_handler_new_always_success() {
        // 总是返回成功的处理器
        let handler = ExceptionHandler::new(|result: Result<Response>, _configs| async {
            match result {
                Ok(resp) => Ok(resp),
                Err(_) => Ok(Response::text("caught error")),
            }
        });
        let _ = handler;
    }

    // ==================== Clone trait 测试 ====================

    #[test]
    fn test_exception_handler_clone() {
        let handler1 = ExceptionHandler::new(|result: Result<Response>, _configs| async { result });
        let handler2 = handler1.clone();
        let _ = handler1;
        let _ = handler2;
    }

    #[test]
    fn test_exception_handler_clone_independent() {
        let handler1 = ExceptionHandler::new(|result: Result<Response>, _configs| async { result });
        let handler2 = handler1.clone();

        // 两个实例应该独立存在
        let _ = handler1;
        let _ = handler2;
    }

    // ==================== 类型验证测试 ====================

    #[test]
    fn test_exception_handler_type() {
        let handler = ExceptionHandler::new(|result: Result<Response>, _configs| async { result });
        // 验证类型
        let _handler: ExceptionHandler<_> = handler;
    }

    #[test]
    fn test_exception_handler_size() {
        use std::mem::size_of_val;
        let handler = ExceptionHandler::new(|result: Result<Response>, _configs| async { result });
        let size = size_of_val(&handler);
        // Arc<F> 的大小是指针大小
        assert!(size > 0);
    }

    // ==================== 集成测试 ====================

    #[cfg(feature = "server")]
    #[tokio::test]
    async fn test_exception_handler_with_success_response() {
        use crate::route::Route;

        let exception_handler = ExceptionHandler::new(|result: Result<Response>, _configs| async {
            // 成功响应应该被正常返回
            result
        });

        let route = Route::new("/")
            .hook(exception_handler)
            .get(|_req: Request| async { Ok("success") });

        let route = Route::new_root().append(route);
        let req = Request::empty();

        let result: Result<Response> = route.call(req).await;
        assert!(result.is_ok());
    }

    #[cfg(feature = "server")]
    #[tokio::test]
    async fn test_exception_handler_catches_error() {
        use crate::route::Route;

        let exception_handler = ExceptionHandler::new(|result: Result<Response>, _configs| async {
            // 捕获错误并返回自定义响应
            match result {
                Ok(resp) => Ok(resp),
                Err(_) => Ok(Response::text("error was caught")),
            }
        });

        let route = Route::new("/")
            .hook(exception_handler)
            .get(|_req: Request| async {
                Err::<&str, _>(crate::SilentError::business_error(
                    http::StatusCode::INTERNAL_SERVER_ERROR,
                    "test error".to_string(),
                ))
            });

        let route = Route::new_root().append(route);
        let req = Request::empty();

        let result: Result<Response> = route.call(req).await;
        assert!(result.is_ok());
    }

    #[cfg(feature = "server")]
    #[tokio::test]
    async fn test_exception_handler_modifies_error_response() {
        use crate::route::Route;

        let exception_handler =
            ExceptionHandler::new(|result: Result<Response>, _configs| async move {
                match result {
                    Ok(resp) => Ok(resp),
                    Err(e) => {
                        // 修改错误响应
                        let mut resp = Response::text(&format!("Error: {}", e.message()));
                        resp.set_status(http::StatusCode::BAD_GATEWAY);
                        Ok(resp)
                    }
                }
            });

        let route = Route::new("/")
            .hook(exception_handler)
            .get(|_req: Request| async {
                Err::<&str, _>(crate::SilentError::business_error(
                    http::StatusCode::INTERNAL_SERVER_ERROR,
                    "original error".to_string(),
                ))
            });

        let route = Route::new_root().append(route);
        let req = Request::empty();

        let result: Result<Response> = route.call(req).await;
        assert!(result.is_ok());
        let resp = result.unwrap();
        assert_eq!(resp.status, http::StatusCode::BAD_GATEWAY);
    }

    #[cfg(feature = "server")]
    #[tokio::test]
    async fn test_exception_handler_preserves_success() {
        use crate::route::Route;

        let exception_handler = ExceptionHandler::new(|result: Result<Response>, _configs| async {
            // 成功响应应该保持不变
            result
        });

        let route = Route::new("/")
            .hook(exception_handler)
            .get(|_req: Request| async {
                let mut resp = Response::text("success");
                resp.set_status(http::StatusCode::OK);
                Ok(resp)
            });

        let route = Route::new_root().append(route);
        let req = Request::empty();

        let result: Result<Response> = route.call(req).await;
        assert!(result.is_ok());
        let resp = result.unwrap();
        assert_eq!(resp.status, http::StatusCode::OK);
    }

    #[cfg(feature = "server")]
    #[tokio::test]
    async fn test_exception_handler_with_into_response() {
        use crate::route::Route;

        // 测试 Into<Response> trait bound
        let exception_handler =
            ExceptionHandler::new(|result: Result<Response>, _configs| async move {
                match result {
                    Ok(_) => Ok("converted to response"), // &str implements Into<Response>
                    Err(_) => Ok("error converted"),
                }
            });

        let route = Route::new("/")
            .hook(exception_handler)
            .get(|_req: Request| async { Ok("original") });

        let route = Route::new_root().append(route);
        let req = Request::empty();

        let result: Result<Response> = route.call(req).await;
        assert!(result.is_ok());
    }

    // ==================== 并发测试 ====================

    #[cfg(feature = "server")]
    #[tokio::test]
    async fn test_exception_handler_concurrent() {
        use crate::route::Route;

        let exception_handler =
            ExceptionHandler::new(|result: Result<Response>, _configs| async { result });

        let route = Route::new("/")
            .hook(exception_handler)
            .get(|_req: Request| async { Ok("concurrent") });

        let route: Arc<Route> = Arc::new(Route::new_root().append(route));

        // 并发多个请求
        let tasks = (0..5)
            .map(|_| {
                let route = Arc::clone(&route);
                tokio::spawn(async move {
                    let req = Request::empty();
                    let result: Result<Response> = route.call(req).await;
                    result
                })
            })
            .collect::<Vec<_>>();

        for task in tasks {
            let result = task.await.unwrap();
            assert!(result.is_ok());
        }
    }

    // ==================== Arc 共享测试 ====================

    #[test]
    fn test_exception_handler_arc_shared() {
        let handler = ExceptionHandler::new(|result: Result<Response>, _configs| async { result });

        // 内部使用 Arc，可以安全地克隆
        let handler1 = handler.clone();
        let handler2 = handler.clone();

        let _ = handler1;
        let _ = handler2;
    }

    // ==================== 边界条件测试 ====================

    #[cfg(feature = "server")]
    #[tokio::test]
    async fn test_exception_handler_empty_response() {
        use crate::route::Route;

        let exception_handler =
            ExceptionHandler::new(|result: Result<Response>, _configs| async { result });

        let route = Route::new("/")
            .hook(exception_handler)
            .get(|_req: Request| async { Ok(Response::empty()) });

        let route = Route::new_root().append(route);
        let req = Request::empty();

        let result: Result<Response> = route.call(req).await;
        assert!(result.is_ok());
    }

    #[cfg(feature = "server")]
    #[tokio::test]
    async fn test_exception_handler_chain_multiple() {
        use crate::route::Route;

        let handler1 = ExceptionHandler::new(|result: Result<Response>, _configs| async { result });

        let handler2 = ExceptionHandler::new(|result: Result<Response>, _configs| async { result });

        let route = Route::new("/")
            .hook(handler1)
            .hook(handler2)
            .get(|_req: Request| async { Ok("chained") });

        let route = Route::new_root().append(route);
        let req = Request::empty();

        let result: Result<Response> = route.call(req).await;
        assert!(result.is_ok());
    }

    #[cfg(feature = "server")]
    #[tokio::test]
    async fn test_exception_handler_different_http_methods() {
        use crate::route::Route;

        let exception_handler =
            ExceptionHandler::new(|result: Result<Response>, _configs| async { result });

        let route = Route::new("/")
            .hook(exception_handler)
            .get(|_req: Request| async { Ok("GET") })
            .post(|_req: Request| async { Ok("POST") });

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
}
