use crate::{Handler, MiddleWareHandler, Next, Request, Response, Result};
use async_trait::async_trait;
use chrono::Utc;

/// ExceptionHandler 中间件
/// ```rust
/// use silent::prelude::*;
/// use silent::middlewares::{RequestTimeLogger};
/// // Define a request time logger middleware
/// let _ = RequestTimeLogger::new();
#[derive(Default, Clone)]
pub struct RequestTimeLogger;

impl RequestTimeLogger {
    pub fn new() -> Self {
        Self {}
    }
}

#[async_trait]
impl MiddleWareHandler for RequestTimeLogger {
    async fn handle(&self, req: Request, next: &Next) -> Result<Response> {
        let method = req.method().clone();
        let url = req.uri().to_string().clone();
        let http_version = req.version();
        let peer_addr = req.remote();
        let start_time = Utc::now().time();
        let res = next.call(req).await;
        let end_time = Utc::now().time();
        let req_time = end_time - start_time;
        Ok(match res {
            Ok(res) => {
                if res.status.as_u16() >= 400 {
                    tracing::warn!(
                        "{} {} {} {:?} {} {:?} {}",
                        peer_addr,
                        method,
                        url,
                        http_version,
                        res.status.as_u16(),
                        res.content_length().lower(),
                        req_time.num_nanoseconds().unwrap_or(0) as f64 / 1000000.0
                    );
                } else {
                    tracing::info!(
                        "{} {} {} {:?} {} {:?} {}",
                        peer_addr,
                        method,
                        url,
                        http_version,
                        res.status.as_u16(),
                        res.content_length().lower(),
                        req_time.num_nanoseconds().unwrap_or(0) as f64 / 1000000.0
                    );
                }
                res
            }
            Err(e) => {
                tracing::error!(
                    "{} {} {} {:?} {} {:?} {} {}",
                    peer_addr,
                    method,
                    url,
                    http_version,
                    e.status().as_u16(),
                    0,
                    req_time.num_nanoseconds().unwrap_or(0) as f64 / 1000000.0,
                    e.to_string()
                );
                e.into()
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    // ==================== 构造函数测试 ====================

    #[test]
    fn test_request_time_logger_new() {
        let logger = RequestTimeLogger::new();
        let _ = logger;
    }

    #[test]
    fn test_request_time_logger_default() {
        let logger = RequestTimeLogger::new();
        let _ = logger;
    }

    // ==================== Clone trait 测试 ====================

    #[test]
    fn test_request_time_logger_clone() {
        let logger1 = RequestTimeLogger::new();
        let logger2 = logger1.clone();
        let _ = logger1;
        let _ = logger2;
    }

    #[test]
    fn test_request_time_logger_clone_independent() {
        let logger1 = RequestTimeLogger::new();
        let logger2 = logger1.clone();

        // 两个实例应该独立存在
        let _ = logger1;
        let _ = logger2;
    }

    // ==================== 类型验证测试 ====================

    #[test]
    fn test_request_time_logger_type() {
        let logger = RequestTimeLogger::new();
        // 验证类型
        let _logger: RequestTimeLogger = logger;
    }

    #[test]
    fn test_request_time_logger_size() {
        use std::mem::size_of;
        let size = size_of::<RequestTimeLogger>();
        // RequestTimeLogger 是空结构体（ZST）
        assert_eq!(size, 0);
    }

    // ==================== 集成测试 ====================

    #[cfg(feature = "server")]
    #[tokio::test]
    async fn test_request_time_logger_success_response() {
        use crate::route::Route;

        let logger = RequestTimeLogger::new();

        let route = Route::new("/")
            .hook(logger)
            .get(|_req: Request| async { Ok("success") });

        let route = Route::new_root().append(route);
        let mut req = Request::empty();
        req.headers_mut()
            .insert("x-real-ip", "127.0.0.1".parse().unwrap());

        let result: Result<Response> = route.call(req).await;
        assert!(result.is_ok());
        let resp = result.unwrap();
        // 成功响应状态码应该是 200
        assert_eq!(resp.status.as_u16(), 200);
    }

    #[cfg(feature = "server")]
    #[tokio::test]
    async fn test_request_time_logger_client_error() {
        use crate::route::Route;

        let logger = RequestTimeLogger::new();

        let route = Route::new("/").hook(logger).get(|_req: Request| async {
            let mut resp = Response::text("not found");
            resp.set_status(http::StatusCode::NOT_FOUND);
            Ok(resp)
        });

        let route = Route::new_root().append(route);
        let mut req = Request::empty();
        req.headers_mut()
            .insert("x-real-ip", "127.0.0.1".parse().unwrap());

        let result: Result<Response> = route.call(req).await;
        assert!(result.is_ok());
        let resp = result.unwrap();
        // 404 状态码应该触发 warn 日志
        assert_eq!(resp.status.as_u16(), 404);
    }

    #[cfg(feature = "server")]
    #[tokio::test]
    async fn test_request_time_logger_server_error() {
        use crate::route::Route;

        let logger = RequestTimeLogger::new();

        let route = Route::new("/").hook(logger).get(|_req: Request| async {
            let mut resp = Response::text("internal error");
            resp.set_status(http::StatusCode::INTERNAL_SERVER_ERROR);
            Ok(resp)
        });

        let route = Route::new_root().append(route);
        let mut req = Request::empty();
        req.headers_mut()
            .insert("x-real-ip", "127.0.0.1".parse().unwrap());

        let result: Result<Response> = route.call(req).await;
        assert!(result.is_ok());
        let resp = result.unwrap();
        // 500 状态码应该触发 warn 日志
        assert_eq!(resp.status.as_u16(), 500);
    }

    #[cfg(feature = "server")]
    #[tokio::test]
    async fn test_request_time_logger_handler_error() {
        use crate::route::Route;

        let logger = RequestTimeLogger::new();

        let route = Route::new("/").hook(logger).get(|_req: Request| async {
            Err::<&str, _>(crate::SilentError::business_error(
                http::StatusCode::BAD_REQUEST,
                "bad request".to_string(),
            ))
        });

        let route = Route::new_root().append(route);
        let mut req = Request::empty();
        req.headers_mut()
            .insert("x-real-ip", "127.0.0.1".parse().unwrap());

        let result: Result<Response> = route.call(req).await;
        // RequestTimeLogger 会记录错误并转换为响应，所以返回成功
        assert!(result.is_ok());
        let resp = result.unwrap();
        // 错误状态码应该被保留
        assert_eq!(resp.status.as_u16(), 400);
    }

    #[cfg(feature = "server")]
    #[tokio::test]
    async fn test_request_time_logger_empty_response() {
        use crate::route::Route;

        let logger = RequestTimeLogger::new();

        let route = Route::new("/")
            .hook(logger)
            .get(|_req: Request| async { Ok(Response::empty()) });

        let route = Route::new_root().append(route);
        let mut req = Request::empty();
        req.headers_mut()
            .insert("x-real-ip", "127.0.0.1".parse().unwrap());

        let result: Result<Response> = route.call(req).await;
        assert!(result.is_ok());
    }

    #[cfg(feature = "server")]
    #[tokio::test]
    async fn test_request_time_logger_with_body() {
        use crate::route::Route;

        let logger = RequestTimeLogger::new();

        let route = Route::new("/")
            .hook(logger)
            .get(|_req: Request| async { Ok(Response::text("response with body")) });

        let route = Route::new_root().append(route);
        let mut req = Request::empty();
        req.headers_mut()
            .insert("x-real-ip", "127.0.0.1".parse().unwrap());

        let result: Result<Response> = route.call(req).await;
        assert!(result.is_ok());
        let resp = result.unwrap();
        assert_eq!(resp.status.as_u16(), 200);
    }

    #[cfg(feature = "server")]
    #[tokio::test]
    async fn test_request_time_logger_preserves_response() {
        use crate::route::Route;

        let logger = RequestTimeLogger::new();

        let route = Route::new("/").hook(logger).get(|_req: Request| async {
            let mut resp = Response::text("test response");
            resp.set_status(http::StatusCode::ACCEPTED);
            Ok(resp)
        });

        let route = Route::new_root().append(route);
        let mut req = Request::empty();
        req.headers_mut()
            .insert("x-real-ip", "127.0.0.1".parse().unwrap());

        let result: Result<Response> = route.call(req).await;
        assert!(result.is_ok());
        let resp = result.unwrap();
        // 中间件应该保留原始响应
        assert_eq!(resp.status.as_u16(), 202);
    }

    #[cfg(feature = "server")]
    #[tokio::test]
    async fn test_request_time_logger_concurrent_requests() {
        use crate::route::Route;

        let logger = RequestTimeLogger::new();

        let route = Route::new("/")
            .hook(logger)
            .get(|_req: Request| async { Ok("concurrent") });

        let route: Arc<Route> = Arc::new(Route::new_root().append(route));

        // 并发多个请求
        let tasks = (0..5).map(|_| {
            let route = Arc::clone(&route);
            tokio::spawn(async move {
                let mut req = Request::empty();
                req.headers_mut()
                    .insert("x-real-ip", "127.0.0.1".parse().unwrap());
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
    async fn test_request_time_logger_different_http_methods() {
        use crate::route::Route;

        let logger = RequestTimeLogger::new();

        let route = Route::new("/")
            .hook(logger)
            .get(|_req: Request| async { Ok("GET") })
            .post(|_req: Request| async { Ok("POST") })
            .put(|_req: Request| async { Ok("PUT") });

        let route = Route::new_root().append(route);

        // 测试 GET
        let mut req = Request::empty();
        req.headers_mut()
            .insert("x-real-ip", "127.0.0.1".parse().unwrap());
        *req.method_mut() = http::Method::GET;
        let result: Result<Response> = route.call(req).await;
        assert!(result.is_ok());

        // 测试 POST
        let mut req = Request::empty();
        req.headers_mut()
            .insert("x-real-ip", "127.0.0.1".parse().unwrap());
        *req.method_mut() = http::Method::POST;
        let result: Result<Response> = route.call(req).await;
        assert!(result.is_ok());

        // 测试 PUT
        let mut req = Request::empty();
        req.headers_mut()
            .insert("x-real-ip", "127.0.0.1".parse().unwrap());
        *req.method_mut() = http::Method::PUT;
        let result: Result<Response> = route.call(req).await;
        assert!(result.is_ok());
    }

    #[cfg(feature = "server")]
    #[tokio::test]
    async fn test_request_time_logger_different_status_codes() {
        use crate::route::Route;

        let logger = RequestTimeLogger::new();

        // 测试 2xx 成功状态
        let route1 = Route::new("/")
            .hook(logger.clone())
            .get(|_req: Request| async {
                let mut resp = Response::text("OK");
                resp.set_status(http::StatusCode::OK);
                Ok(resp)
            });

        // 测试 3xx 重定向状态
        let route2 = Route::new("/")
            .hook(logger.clone())
            .get(|_req: Request| async {
                let mut resp = Response::empty();
                resp.set_status(http::StatusCode::FOUND);
                Ok(resp)
            });

        // 测试 4xx 客户端错误
        let route3 = Route::new("/")
            .hook(logger.clone())
            .get(|_req: Request| async {
                let mut resp = Response::text("Bad Request");
                resp.set_status(http::StatusCode::BAD_REQUEST);
                Ok(resp)
            });

        // 测试 5xx 服务器错误
        let route4 = Route::new("/").hook(logger).get(|_req: Request| async {
            let mut resp = Response::text("Internal Server Error");
            resp.set_status(http::StatusCode::INTERNAL_SERVER_ERROR);
            Ok(resp)
        });

        for route in &[route1, route2, route3, route4] {
            let route = Route::new_root().append(route.clone());
            let mut req = Request::empty();
            req.headers_mut()
                .insert("x-real-ip", "127.0.0.1".parse().unwrap());
            let result: Result<Response> = route.call(req).await;
            assert!(result.is_ok());
        }
    }

    #[cfg(feature = "server")]
    #[tokio::test]
    async fn test_request_time_logger_chain_with_other_middleware() {
        use crate::middlewares::ExceptionHandler;
        use crate::route::Route;

        let time_logger = RequestTimeLogger::new();
        let exception_handler =
            ExceptionHandler::new(|result: Result<Response>, _configs| async move { result });

        let route = Route::new("/")
            .hook(time_logger)
            .hook(exception_handler)
            .get(|_req: Request| async { Ok("chained") });

        let route = Route::new_root().append(route);
        let mut req = Request::empty();
        req.headers_mut()
            .insert("x-real-ip", "127.0.0.1".parse().unwrap());

        let result: Result<Response> = route.call(req).await;
        assert!(result.is_ok());
    }

    #[cfg(feature = "server")]
    #[tokio::test]
    async fn test_request_time_logger_multiple_loggers() {
        use crate::route::Route;

        let logger1 = RequestTimeLogger::new();
        let logger2 = RequestTimeLogger::new();

        let route = Route::new("/")
            .hook(logger1)
            .hook(logger2)
            .get(|_req: Request| async { Ok("multiple loggers") });

        let route = Route::new_root().append(route);
        let mut req = Request::empty();
        req.headers_mut()
            .insert("x-real-ip", "127.0.0.1".parse().unwrap());

        let result: Result<Response> = route.call(req).await;
        assert!(result.is_ok());
    }

    #[cfg(feature = "server")]
    #[tokio::test]
    async fn test_request_time_logger_preserves_headers() {
        use crate::route::Route;

        let logger = RequestTimeLogger::new();

        let route = Route::new("/").hook(logger).get(|_req: Request| async {
            let mut resp = Response::text("with headers");
            resp.headers_mut()
                .insert("X-Custom-Header", "test-value".parse().unwrap());
            Ok(resp)
        });

        let route = Route::new_root().append(route);
        let mut req = Request::empty();
        req.headers_mut()
            .insert("x-real-ip", "127.0.0.1".parse().unwrap());

        let result: Result<Response> = route.call(req).await;
        assert!(result.is_ok());
        let resp = result.unwrap();
        assert!(resp.headers().get("X-Custom-Header").is_some());
    }

    #[cfg(feature = "server")]
    #[tokio::test]
    async fn test_request_time_logger_with_different_urls() {
        use crate::route::Route;

        let logger = RequestTimeLogger::new();

        let route = Route::new("/test")
            .hook(logger)
            .get(|_req: Request| async { Ok("test") });

        let route = Route::new_root().append(route);

        // 测试不同的 URL 路径（只测试会匹配的 URL）
        for url in &["/test", "/test?query=value", "/test?foo=bar&baz=qux"] {
            let mut req = Request::empty();
            req.headers_mut()
                .insert("x-real-ip", "127.0.0.1".parse().unwrap());
            *req.uri_mut() = url.parse().unwrap();
            let result: Result<Response> = route.call(req).await;
            assert!(result.is_ok());
        }
    }
}
