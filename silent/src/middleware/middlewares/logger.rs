use crate::{Handler, MiddleWareHandler, Next, Request, Response, Result};
use async_trait::async_trait;
use std::time::Instant;

/// Logger 中间件
///
/// 记录每个请求的结构化日志，包含客户端 IP、方法、路径、HTTP 版本、
/// 响应状态码、响应体大小和处理耗时。
///
/// 日志级别根据响应状态码自动选择：
/// - 2xx/3xx: `INFO`
/// - 4xx: `WARN`
/// - 5xx 或处理器返回错误: `ERROR`
///
/// # 示例
///
/// ```rust
/// use silent::prelude::*;
/// use silent::middlewares::Logger;
///
/// let route = Route::new("api")
///     .hook(Logger::new())
///     .get(|_req: Request| async { Ok("hello") });
/// ```
#[derive(Default, Clone)]
pub struct Logger;

impl Logger {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl MiddleWareHandler for Logger {
    async fn handle(&self, req: Request, next: &Next) -> Result<Response> {
        let method = req.method().clone();
        let path = req.uri().path().to_string();
        let query = req.uri().query().map(|q| q.to_string());
        let version = format!("{:?}", req.version());
        let peer_addr = req
            .headers()
            .get("x-real-ip")
            .and_then(|h| h.to_str().ok())
            .unwrap_or("-")
            .to_string();

        let start = Instant::now();
        let res = next.call(req).await;
        let elapsed = start.elapsed();
        let elapsed_ms = elapsed.as_secs_f64() * 1000.0;

        match res {
            Ok(res) => {
                let status = res.status.as_u16();
                let size = res.content_length().lower();

                if status >= 500 {
                    tracing::error!(
                        peer = %peer_addr,
                        %method,
                        %path,
                        query = query.as_deref().unwrap_or(""),
                        %version,
                        %status,
                        size,
                        elapsed_ms,
                        "request completed"
                    );
                } else if status >= 400 {
                    tracing::warn!(
                        peer = %peer_addr,
                        %method,
                        %path,
                        query = query.as_deref().unwrap_or(""),
                        %version,
                        %status,
                        size,
                        elapsed_ms,
                        "request completed"
                    );
                } else {
                    tracing::info!(
                        peer = %peer_addr,
                        %method,
                        %path,
                        query = query.as_deref().unwrap_or(""),
                        %version,
                        %status,
                        size,
                        elapsed_ms,
                        "request completed"
                    );
                }
                Ok(res)
            }
            Err(e) => {
                let status = e.status().as_u16();
                tracing::error!(
                    peer = %peer_addr,
                    %method,
                    %path,
                    query = query.as_deref().unwrap_or(""),
                    %version,
                    %status,
                    elapsed_ms,
                    error = %e,
                    "request failed"
                );
                Err(e)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[test]
    fn test_logger_new() {
        let _logger = Logger::new();
    }

    #[test]
    fn test_logger_default() {
        let _logger = Logger;
    }

    #[test]
    fn test_logger_clone() {
        let logger1 = Logger::new();
        let _logger2 = logger1.clone();
    }

    #[test]
    fn test_logger_size() {
        assert_eq!(std::mem::size_of::<Logger>(), 0);
    }

    #[cfg(feature = "server")]
    #[tokio::test]
    async fn test_logger_success_response() {
        use crate::route::Route;

        let route = Route::new("/")
            .hook(Logger::new())
            .get(|_req: Request| async { Ok("success") });

        let route = Route::new_root().append(route);
        let mut req = Request::empty();
        req.headers_mut()
            .insert("x-real-ip", "127.0.0.1".parse().unwrap());

        let result: Result<Response> = route.call(req).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap().status.as_u16(), 200);
    }

    #[cfg(feature = "server")]
    #[tokio::test]
    async fn test_logger_client_error() {
        use crate::route::Route;

        let route = Route::new("/")
            .hook(Logger::new())
            .get(|_req: Request| async {
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
        assert_eq!(result.unwrap().status.as_u16(), 404);
    }

    #[cfg(feature = "server")]
    #[tokio::test]
    async fn test_logger_server_error() {
        use crate::route::Route;

        let route = Route::new("/")
            .hook(Logger::new())
            .get(|_req: Request| async {
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
        assert_eq!(result.unwrap().status.as_u16(), 500);
    }

    #[cfg(feature = "server")]
    #[tokio::test]
    async fn test_logger_handler_error() {
        use crate::route::Route;

        let route = Route::new("/")
            .hook(Logger::new())
            .get(|_req: Request| async {
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
        // Logger 不吞错误，传播给上层
        assert!(result.is_err());
    }

    #[cfg(feature = "server")]
    #[tokio::test]
    async fn test_logger_without_peer_addr() {
        use crate::route::Route;

        let route = Route::new("/")
            .hook(Logger::new())
            .get(|_req: Request| async { Ok("no peer") });

        let route = Route::new_root().append(route);
        let req = Request::empty();

        let result: Result<Response> = route.call(req).await;
        assert!(result.is_ok());
    }

    #[cfg(feature = "server")]
    #[tokio::test]
    async fn test_logger_with_query_string() {
        use crate::route::Route;

        let route = Route::new("/search")
            .hook(Logger::new())
            .get(|_req: Request| async { Ok("results") });

        let route = Route::new_root().append(route);
        let mut req = Request::empty();
        req.headers_mut()
            .insert("x-real-ip", "127.0.0.1".parse().unwrap());
        *req.uri_mut() = "/search?q=test&page=1".parse().unwrap();

        let result: Result<Response> = route.call(req).await;
        assert!(result.is_ok());
    }

    #[cfg(feature = "server")]
    #[tokio::test]
    async fn test_logger_preserves_response() {
        use crate::route::Route;

        let route = Route::new("/")
            .hook(Logger::new())
            .get(|_req: Request| async {
                let mut resp = Response::text("custom");
                resp.set_status(http::StatusCode::ACCEPTED);
                resp.headers_mut()
                    .insert("X-Custom", "value".parse().unwrap());
                Ok(resp)
            });

        let route = Route::new_root().append(route);
        let mut req = Request::empty();
        req.headers_mut()
            .insert("x-real-ip", "127.0.0.1".parse().unwrap());

        let result: Result<Response> = route.call(req).await;
        assert!(result.is_ok());
        let resp = result.unwrap();
        assert_eq!(resp.status.as_u16(), 202);
        assert!(resp.headers().get("X-Custom").is_some());
    }

    #[cfg(feature = "server")]
    #[tokio::test]
    async fn test_logger_concurrent() {
        use crate::route::Route;

        let route = Route::new("/")
            .hook(Logger::new())
            .get(|_req: Request| async { Ok("concurrent") });

        let route: Arc<Route> = Arc::new(Route::new_root().append(route));

        let tasks: Vec<_> = (0..5)
            .map(|_| {
                let route = Arc::clone(&route);
                tokio::spawn(async move {
                    let mut req = Request::empty();
                    req.headers_mut()
                        .insert("x-real-ip", "127.0.0.1".parse().unwrap());
                    let result: Result<Response> = route.call(req).await;
                    result
                })
            })
            .collect();

        for task in tasks {
            assert!(task.await.unwrap().is_ok());
        }
    }
}
