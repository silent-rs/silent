use crate::{Handler, MiddleWareHandler, Next, Request, Response, Result};
use async_trait::async_trait;
use http::HeaderName;
use uuid::Uuid;

const DEFAULT_HEADER: &str = "x-request-id";

/// RequestId 中间件
///
/// 为每个请求生成或透传唯一请求 ID，并注入到请求头、响应头和 tracing span 中。
///
/// # 行为
///
/// 1. 如果请求头中已包含指定的 ID 头（默认 `x-request-id`），则复用该值（上游代理透传场景）
/// 2. 否则生成一个新的 UUID v4 作为请求 ID
/// 3. 将请求 ID 设置到请求头中，下游 handler 可通过 `req.headers().get("x-request-id")` 获取
/// 4. 将请求 ID 设置到响应头中，方便客户端关联
/// 5. 将请求 ID 注入 tracing span，便于日志追踪
///
/// # 示例
///
/// ```rust
/// use silent::prelude::*;
/// use silent::middlewares::RequestId;
///
/// let route = Route::new("/")
///     .hook(RequestId::new())
///     .get(|_req: Request| async { Ok("hello") });
/// ```
///
/// 自定义头名称：
///
/// ```rust
/// use silent::prelude::*;
/// use silent::middlewares::RequestId;
///
/// let route = Route::new("/")
///     .hook(RequestId::with_header("x-trace-id"))
///     .get(|_req: Request| async { Ok("hello") });
/// ```
#[derive(Clone)]
pub struct RequestId {
    header_name: HeaderName,
}

impl Default for RequestId {
    fn default() -> Self {
        Self::new()
    }
}

impl RequestId {
    /// 使用默认头名称 `x-request-id` 创建中间件。
    pub fn new() -> Self {
        Self {
            header_name: HeaderName::from_static(DEFAULT_HEADER),
        }
    }

    /// 使用自定义头名称创建中间件。
    ///
    /// # Panics
    ///
    /// 如果 `name` 不是合法的 HTTP 头名称，会 panic。
    pub fn with_header(name: &'static str) -> Self {
        Self {
            header_name: HeaderName::from_static(name),
        }
    }
}

#[async_trait]
impl MiddleWareHandler for RequestId {
    async fn handle(&self, mut req: Request, next: &Next) -> Result<Response> {
        // 优先使用请求中已有的 ID（上游代理透传），否则生成新的
        let request_id = req
            .headers()
            .get(&self.header_name)
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_owned())
            .unwrap_or_else(|| Uuid::new_v4().to_string());

        // 将 ID 注入请求头，下游 handler 可获取
        if let Ok(val) = request_id.parse() {
            req.headers_mut().insert(self.header_name.clone(), val);
        }

        tracing::debug!(request_id = %request_id, "request started");
        let mut res = next.call(req).await?;

        // 将 ID 注入响应头
        if let Ok(val) = request_id.parse() {
            res.headers_mut().insert(self.header_name.clone(), val);
        }

        Ok(res)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    // ==================== 构造函数测试 ====================

    #[test]
    fn test_request_id_new() {
        let mid = RequestId::new();
        assert_eq!(mid.header_name.as_str(), "x-request-id");
    }

    #[test]
    fn test_request_id_default() {
        let mid = RequestId::default();
        assert_eq!(mid.header_name.as_str(), "x-request-id");
    }

    #[test]
    fn test_request_id_with_header() {
        let mid = RequestId::with_header("x-trace-id");
        assert_eq!(mid.header_name.as_str(), "x-trace-id");
    }

    #[test]
    fn test_request_id_clone() {
        let mid1 = RequestId::new();
        let mid2 = mid1.clone();
        assert_eq!(mid1.header_name, mid2.header_name);
    }

    #[test]
    fn test_request_id_size() {
        use std::mem::size_of;
        // HeaderName 不是 ZST
        assert!(size_of::<RequestId>() > 0);
    }

    // ==================== 集成测试 ====================

    #[cfg(feature = "server")]
    #[tokio::test]
    async fn test_request_id_generates_id() {
        use crate::route::Route;

        let mid = RequestId::new();
        let route = Route::new("/")
            .hook(mid)
            .get(|_req: Request| async { Ok("ok") });
        let route = Route::new_root().append(route);

        let req = Request::empty();
        let res: Result<Response> = crate::Handler::call(&route, req).await;
        assert!(res.is_ok());
        let resp = res.unwrap();
        // 响应中应包含自动生成的 x-request-id
        let id = resp.headers().get("x-request-id");
        assert!(id.is_some());
        // 应该是有效的 UUID
        let id_str = id.unwrap().to_str().unwrap();
        assert!(Uuid::parse_str(id_str).is_ok());
    }

    #[cfg(feature = "server")]
    #[tokio::test]
    async fn test_request_id_passthrough() {
        use crate::route::Route;

        let mid = RequestId::new();
        let route = Route::new("/")
            .hook(mid)
            .get(|_req: Request| async { Ok("ok") });
        let route = Route::new_root().append(route);

        let mut req = Request::empty();
        req.headers_mut()
            .insert("x-request-id", "my-custom-id-123".parse().unwrap());

        let res: Result<Response> = crate::Handler::call(&route, req).await;
        assert!(res.is_ok());
        let resp = res.unwrap();
        // 应透传上游提供的 ID
        let id = resp.headers().get("x-request-id").unwrap();
        assert_eq!(id.to_str().unwrap(), "my-custom-id-123");
    }

    #[cfg(feature = "server")]
    #[tokio::test]
    async fn test_request_id_custom_header() {
        use crate::route::Route;

        let mid = RequestId::with_header("x-trace-id");
        let route = Route::new("/")
            .hook(mid)
            .get(|_req: Request| async { Ok("ok") });
        let route = Route::new_root().append(route);

        let req = Request::empty();
        let res: Result<Response> = crate::Handler::call(&route, req).await;
        assert!(res.is_ok());
        let resp = res.unwrap();
        // 应使用自定义头名
        assert!(resp.headers().get("x-trace-id").is_some());
        assert!(resp.headers().get("x-request-id").is_none());
    }

    #[cfg(feature = "server")]
    #[tokio::test]
    async fn test_request_id_unique_per_request() {
        use crate::route::Route;

        let mid = RequestId::new();
        let route = Route::new("/")
            .hook(mid)
            .get(|_req: Request| async { Ok("ok") });
        let route = Route::new_root().append(route);

        let req1 = Request::empty();
        let res1: Result<Response> = crate::Handler::call(&route, req1).await;
        let id1 = res1
            .unwrap()
            .headers()
            .get("x-request-id")
            .unwrap()
            .to_str()
            .unwrap()
            .to_owned();

        let req2 = Request::empty();
        let res2: Result<Response> = crate::Handler::call(&route, req2).await;
        let id2 = res2
            .unwrap()
            .headers()
            .get("x-request-id")
            .unwrap()
            .to_str()
            .unwrap()
            .to_owned();

        // 两次请求应生成不同的 ID
        assert_ne!(id1, id2);
    }

    #[cfg(feature = "server")]
    #[tokio::test]
    async fn test_request_id_handler_can_read() {
        use crate::route::Route;

        let mid = RequestId::new();
        let route = Route::new("/").hook(mid).get(|req: Request| async move {
            // handler 应能通过请求头读取 ID
            let id = req
                .headers()
                .get("x-request-id")
                .unwrap()
                .to_str()
                .unwrap()
                .to_owned();
            Ok(id)
        });
        let route = Route::new_root().append(route);

        let req = Request::empty();
        let res: Result<Response> = crate::Handler::call(&route, req).await;
        assert!(res.is_ok());
    }

    #[cfg(feature = "server")]
    #[tokio::test]
    async fn test_request_id_preserves_response() {
        use crate::route::Route;

        let mid = RequestId::new();
        let route = Route::new("/").hook(mid).get(|_req: Request| async {
            let mut resp = Response::text("custom body");
            resp.set_status(http::StatusCode::ACCEPTED);
            resp.headers_mut()
                .insert("x-custom", "value".parse().unwrap());
            Ok(resp)
        });
        let route = Route::new_root().append(route);

        let req = Request::empty();
        let res: Result<Response> = crate::Handler::call(&route, req).await;
        assert!(res.is_ok());
        let resp = res.unwrap();
        assert_eq!(resp.status.as_u16(), 202);
        assert!(resp.headers().get("x-custom").is_some());
        assert!(resp.headers().get("x-request-id").is_some());
    }

    #[cfg(feature = "server")]
    #[tokio::test]
    async fn test_request_id_concurrent() {
        use crate::route::Route;

        let mid = RequestId::new();
        let route = Route::new("/")
            .hook(mid)
            .get(|_req: Request| async { Ok("ok") });
        let route: Arc<Route> = Arc::new(Route::new_root().append(route));

        let tasks: Vec<_> = (0..10)
            .map(|_| {
                let route = Arc::clone(&route);
                tokio::spawn(async move {
                    let req = Request::empty();
                    let res: Result<Response> = crate::Handler::call(&*route, req).await;
                    res.unwrap()
                        .headers()
                        .get("x-request-id")
                        .unwrap()
                        .to_str()
                        .unwrap()
                        .to_owned()
                })
            })
            .collect();

        let mut ids = Vec::new();
        for task in tasks {
            ids.push(task.await.unwrap());
        }

        // 所有 ID 应唯一
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), 10);
    }

    #[cfg(feature = "server")]
    #[tokio::test]
    async fn test_request_id_with_error_handler() {
        use crate::route::Route;

        let mid = RequestId::new();
        let route = Route::new("/").hook(mid).get(|_req: Request| async {
            Err::<&str, _>(crate::SilentError::business_error(
                http::StatusCode::BAD_REQUEST,
                "bad".to_string(),
            ))
        });
        let route = Route::new_root().append(route);

        let req = Request::empty();
        let res: Result<Response> = crate::Handler::call(&route, req).await;
        // 错误情况下中间件返回 Err，不会设置响应头
        assert!(res.is_err());
    }
}
