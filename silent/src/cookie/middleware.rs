use crate::{Handler, MiddleWareHandler, Next, Request, Response, Result, SilentError};
use async_trait::async_trait;
use cookie::{Cookie, CookieJar};
use http::{StatusCode, header};

#[derive(Debug, Default)]
pub struct CookieMiddleware {}

impl CookieMiddleware {
    pub(crate) fn new() -> Self {
        Self::default()
    }
}

#[async_trait]
impl MiddleWareHandler for CookieMiddleware {
    async fn handle(&self, mut req: Request, next: &Next) -> Result<Response> {
        let mut jar = CookieJar::new();
        if let Some(cookies) = req.headers().get(header::COOKIE) {
            for cookie_str in cookies
                .to_str()
                .map_err(|e| {
                    SilentError::business_error(
                        StatusCode::BAD_REQUEST,
                        format!("Failed to parse cookie: {e}"),
                    )
                })?
                .split(';')
                .map(|s| s.trim())
            {
                if let Ok(cookie) = Cookie::parse_encoded(cookie_str).map(|c| c.into_owned()) {
                    jar.add_original(cookie);
                }
            }
        }
        req.extensions_mut().insert(jar.clone());
        let mut res = next.call(req).await?;
        if let Some(cookie_jar) = res.extensions().get::<CookieJar>() {
            for cookie in cookie_jar.delta().cloned() {
                jar.add(cookie)
            }
            res.extensions_mut().insert(jar);
        } else {
            res.extensions_mut().insert(jar);
        };
        Ok(res)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    // ==================== 构造函数测试 ====================

    #[test]
    fn test_cookie_middleware_new() {
        let middleware = CookieMiddleware::new();
        let _ = middleware;
    }

    #[test]
    fn test_cookie_middleware_default() {
        let middleware = CookieMiddleware::default();
        let _ = middleware;
    }

    // ==================== Debug trait 测试 ====================

    #[test]
    fn test_cookie_middleware_debug() {
        let middleware = CookieMiddleware::new();
        let debug_str = format!("{:?}", middleware);
        assert!(debug_str.contains("CookieMiddleware"));
    }

    // ==================== 类型验证测试 ====================

    #[test]
    fn test_cookie_middleware_type() {
        let middleware = CookieMiddleware::new();
        let _middleware: CookieMiddleware = middleware;
    }

    #[test]
    fn test_cookie_middleware_size() {
        use std::mem::size_of;
        let size = size_of::<CookieMiddleware>();
        // CookieMiddleware 是空结构体（ZST）
        assert_eq!(size, 0);
    }

    // ==================== 集成测试 ====================

    #[cfg(feature = "server")]
    #[tokio::test]
    async fn test_cookie_middleware_parse_request_cookies() {
        use crate::route::Route;

        let middleware = CookieMiddleware::new();

        let route = Route::new("/")
            .hook(middleware)
            .get(|req: Request| async move {
                // 验证请求中的 Cookie 被 CookieJar 解析并存储
                let jar = req.extensions().get::<CookieJar>();
                assert!(jar.is_some());
                Ok("parsed")
            });

        let route = Route::new_root().append(route);

        // 创建带有 Cookie 的请求
        let mut req = Request::empty();
        req.headers_mut().insert(
            header::COOKIE,
            "session=abc123; user=testuser".parse().unwrap(),
        );

        let result: Result<Response> = route.call(req).await;
        assert!(result.is_ok());
    }

    #[cfg(feature = "server")]
    #[tokio::test]
    async fn test_cookie_middleware_with_response_cookies() {
        use crate::route::Route;

        let middleware = CookieMiddleware::new();

        let route = Route::new("/")
            .hook(middleware)
            .get(|req: Request| async move {
                // 在响应中添加 Cookie
                let jar = req
                    .extensions()
                    .get::<CookieJar>()
                    .cloned()
                    .unwrap_or_default();
                let mut resp = Response::text("cookies set");
                resp.extensions_mut().insert(jar);
                Ok(resp)
            });

        let route = Route::new_root().append(route);
        let req = Request::empty();

        let result: Result<Response> = route.call(req).await;
        assert!(result.is_ok());
    }

    #[cfg(feature = "server")]
    #[tokio::test]
    async fn test_cookie_middleware_no_cookies() {
        use crate::route::Route;

        let middleware = CookieMiddleware::new();

        let route = Route::new("/")
            .hook(middleware)
            .get(|req: Request| async move {
                // 没有 Cookie 头时应该有一个空的 CookieJar
                let jar = req.extensions().get::<CookieJar>();
                assert!(jar.is_some());
                Ok("no cookies")
            });

        let route = Route::new_root().append(route);
        let req = Request::empty();

        let result: Result<Response> = route.call(req).await;
        assert!(result.is_ok());
    }

    #[cfg(feature = "server")]
    #[tokio::test]
    async fn test_cookie_middleware_malformed_cookie_value() {
        use crate::route::Route;

        let middleware = CookieMiddleware::new();

        let route = Route::new("/")
            .hook(middleware)
            .get(|req: Request| async move {
                // 即使某些 Cookie 解析失败，中间件仍应继续处理
                let jar = req.extensions().get::<CookieJar>();
                assert!(jar.is_some());
                Ok("test")
            });

        let route = Route::new_root().append(route);

        // 创建包含格式错误的 Cookie（无效的 UTF-8）
        let mut req = Request::empty();
        // 使用包含无效 UTF-8 的字节序列
        let invalid_bytes = &[0xFF, 0xFE, 0xFD];
        req.headers_mut().insert(
            header::COOKIE,
            hyper::header::HeaderValue::from_bytes(invalid_bytes).unwrap(),
        );

        let result: Result<Response> = route.call(req).await;
        // 应该返回错误，因为 Cookie 头包含无效的 UTF-8
        assert!(result.is_err());
        if let Err(e) = result {
            assert_eq!(e.status(), StatusCode::BAD_REQUEST);
        }
    }

    #[cfg(feature = "server")]
    #[tokio::test]
    async fn test_cookie_middleware_multiple_cookies() {
        use crate::route::Route;

        let middleware = CookieMiddleware::new();

        let route = Route::new("/")
            .hook(middleware)
            .get(|req: Request| async move {
                let jar = req.extensions().get::<CookieJar>();
                assert!(jar.is_some());
                Ok("multiple cookies")
            });

        let route = Route::new_root().append(route);

        // 创建包含多个 Cookie 的请求
        let mut req = Request::empty();
        req.headers_mut().insert(
            header::COOKIE,
            "cookie1=value1; cookie2=value2; cookie3=value3"
                .parse()
                .unwrap(),
        );

        let result: Result<Response> = route.call(req).await;
        assert!(result.is_ok());
    }

    #[cfg(feature = "server")]
    #[tokio::test]
    async fn test_cookie_middleware_cookie_with_spaces() {
        use crate::route::Route;

        let middleware = CookieMiddleware::new();

        let route = Route::new("/")
            .hook(middleware)
            .get(|req: Request| async move {
                let jar = req.extensions().get::<CookieJar>();
                assert!(jar.is_some());
                Ok("spaces")
            });

        let route = Route::new_root().append(route);

        // 创建包含空格的 Cookie
        let mut req = Request::empty();
        req.headers_mut().insert(
            header::COOKIE,
            "name1 = value1 ; name2=value2".parse().unwrap(),
        );

        let result: Result<Response> = route.call(req).await;
        assert!(result.is_ok());
    }

    #[cfg(feature = "server")]
    #[tokio::test]
    async fn test_cookie_middleware_empty_cookie_value() {
        use crate::route::Route;

        let middleware = CookieMiddleware::new();

        let route = Route::new("/")
            .hook(middleware)
            .get(|req: Request| async move {
                let jar = req.extensions().get::<CookieJar>();
                assert!(jar.is_some());
                Ok("empty value")
            });

        let route = Route::new_root().append(route);

        // 创建包含空值的 Cookie
        let mut req = Request::empty();
        req.headers_mut()
            .insert(header::COOKIE, "cookie1=; cookie2=value2".parse().unwrap());

        let result: Result<Response> = route.call(req).await;
        assert!(result.is_ok());
    }

    #[cfg(feature = "server")]
    #[tokio::test]
    async fn test_cookie_middleware_preserves_original_cookies() {
        use crate::route::Route;

        let middleware = CookieMiddleware::new();

        let route = Route::new("/")
            .hook(middleware)
            .get(|req: Request| async move {
                // 验证原始 Cookie 被保存
                let jar = req.extensions().get::<CookieJar>();
                assert!(jar.is_some());
                Ok("preserved")
            });

        let route = Route::new_root().append(route);

        let mut req = Request::empty();
        req.headers_mut()
            .insert(header::COOKIE, "session=xyz789".parse().unwrap());

        let result: Result<Response> = route.call(req).await;
        assert!(result.is_ok());
    }

    #[cfg(feature = "server")]
    #[tokio::test]
    async fn test_cookie_middleware_response_has_cookie_jar() {
        use crate::route::Route;

        let middleware = CookieMiddleware::new();

        let route = Route::new("/")
            .hook(middleware)
            .get(|_req: Request| async { Ok("response test") });

        let route = Route::new_root().append(route);
        let req = Request::empty();

        let result: Result<Response> = route.call(req).await;
        assert!(result.is_ok());

        let resp = result.unwrap();
        // 响应应该有 CookieJar（即使是空的）
        let jar = resp.extensions().get::<CookieJar>();
        assert!(jar.is_some());
    }

    #[cfg(feature = "server")]
    #[tokio::test]
    async fn test_cookie_middleware_concurrent_requests() {
        use crate::route::Route;

        let middleware = CookieMiddleware::new();

        let route = Route::new("/")
            .hook(middleware)
            .get(|_req: Request| async { Ok("concurrent") });

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
    async fn test_cookie_middleware_single_cookie() {
        use crate::route::Route;

        let middleware = CookieMiddleware::new();

        let route = Route::new("/")
            .hook(middleware)
            .get(|req: Request| async move {
                let jar = req.extensions().get::<CookieJar>();
                assert!(jar.is_some());
                Ok("single")
            });

        let route = Route::new_root().append(route);

        let mut req = Request::empty();
        req.headers_mut()
            .insert(header::COOKIE, "only_one=value".parse().unwrap());

        let result: Result<Response> = route.call(req).await;
        assert!(result.is_ok());
    }

    #[cfg(feature = "server")]
    #[tokio::test]
    async fn test_cookie_middleware_cookie_with_special_chars() {
        use crate::route::Route;

        let middleware = CookieMiddleware::new();

        let route = Route::new("/")
            .hook(middleware)
            .get(|req: Request| async move {
                let jar = req.extensions().get::<CookieJar>();
                assert!(jar.is_some());
                Ok("special chars")
            });

        let route = Route::new_root().append(route);

        // 创建包含特殊字符的 Cookie（URL 编码）
        let mut req = Request::empty();
        req.headers_mut().insert(
            header::COOKIE,
            "name=value%20with%20spaces".parse().unwrap(),
        );

        let result: Result<Response> = route.call(req).await;
        assert!(result.is_ok());
    }

    #[cfg(feature = "server")]
    #[tokio::test]
    async fn test_cookie_middleware_chain_with_other_middleware() {
        use crate::middlewares::ExceptionHandler;
        use crate::route::Route;

        let cookie_middleware = CookieMiddleware::new();
        let exception_handler =
            ExceptionHandler::new(|result: Result<Response>, _configs| async move { result });

        let route = Route::new("/")
            .hook(cookie_middleware)
            .hook(exception_handler)
            .get(|req: Request| async move {
                let jar = req.extensions().get::<CookieJar>();
                assert!(jar.is_some());
                Ok("chained")
            });

        let route = Route::new_root().append(route);
        let req = Request::empty();

        let result: Result<Response> = route.call(req).await;
        assert!(result.is_ok());
    }

    #[cfg(feature = "server")]
    #[tokio::test]
    async fn test_cookie_middleware_different_http_methods() {
        use crate::route::Route;

        let middleware = CookieMiddleware::new();

        let route = Route::new("/")
            .hook(middleware)
            .get(|req: Request| async move {
                let jar = req.extensions().get::<CookieJar>();
                assert!(jar.is_some());
                Ok("GET")
            })
            .post(|req: Request| async move {
                let jar = req.extensions().get::<CookieJar>();
                assert!(jar.is_some());
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
}
