use crate::{Handler, MiddleWareHandler, Next, Request, Response, Result, SilentError};
use async_trait::async_trait;
use http::StatusCode;
use std::time::Duration;

#[cfg(feature = "server")]
/// Timeout 中间件 - 在server模式下提供请求超时控制
/// ```rust
/// use silent::prelude::*;
/// use silent::middlewares::Timeout;
/// use std::time::Duration;
/// // Define a timeout middleware
/// let _ = Timeout::new(Duration::from_secs(30));
#[derive(Default, Clone)]
pub struct Timeout {
    timeout: Duration,
}

#[cfg(feature = "server")]
impl Timeout {
    pub fn new(timeout: Duration) -> Self {
        Self { timeout }
    }
}

#[cfg(feature = "server")]
#[async_trait]
impl MiddleWareHandler for Timeout {
    async fn handle(&self, req: Request, next: &Next) -> Result<Response> {
        match tokio::time::timeout(self.timeout, next.call(req))
            .await
            .map_err(|_| {
                SilentError::business_error(
                    StatusCode::REQUEST_TIMEOUT,
                    "Request timed out".to_string(),
                )
            }) {
            Ok(res) => res,
            Err(err) => Err(err),
        }
    }
}

#[cfg(not(feature = "server"))]
/// Timeout 中间件 - 非server模式下不可用
#[derive(Debug, Clone)]
pub struct Timeout {
    _timeout: Duration,
}

#[cfg(not(feature = "server"))]
impl Timeout {
    pub fn new(_timeout: Duration) -> Self {
        Self { _timeout }
    }
}

#[cfg(not(feature = "server"))]
impl MiddleWareHandler for Timeout {
    fn name(&self) -> &'static str {
        "timeout"
    }

    fn is_available(&self) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::time::Duration;

    // ==================== 构造函数测试 ====================

    #[test]
    fn test_timeout_new() {
        let timeout = Timeout::new(Duration::from_secs(30));
        let _ = timeout;
    }

    #[test]
    fn test_timeout_new_zero() {
        let timeout = Timeout::new(Duration::ZERO);
        let _ = timeout;
    }

    #[test]
    fn test_timeout_new_millis() {
        let timeout = Timeout::new(Duration::from_millis(100));
        let _ = timeout;
    }

    // ==================== Clone trait 测试 ====================

    #[test]
    fn test_timeout_clone() {
        let timeout1 = Timeout::new(Duration::from_secs(10));
        let timeout2 = timeout1.clone();
        let _ = timeout1;
        let _ = timeout2;
    }

    #[test]
    fn test_timeout_clone_independent() {
        let timeout1 = Timeout::new(Duration::from_secs(5));
        let timeout2 = timeout1.clone();

        // 两个实例应该独立存在
        let _ = timeout1;
        let _ = timeout2;
    }

    // ==================== Default trait 测试 ====================

    #[test]
    fn test_timeout_default() {
        #[cfg(feature = "server")]
        let timeout = Timeout::default();
        #[cfg(not(feature = "server"))]
        let timeout = <Timeout as Default>::default();

        let _ = timeout;
    }

    // ==================== 边界条件测试 ====================

    #[test]
    fn test_timeout_very_short() {
        let timeout = Timeout::new(Duration::from_nanos(1));
        let _ = timeout;
    }

    #[test]
    fn test_timeout_very_long() {
        let timeout = Timeout::new(Duration::from_secs(3600 * 24 * 365)); // 1年
        let _ = timeout;
    }

    #[test]
    fn test_timeout_max_duration() {
        let timeout = Timeout::new(Duration::MAX);
        let _ = timeout;
    }

    // ==================== 类型验证测试 ====================

    #[test]
    fn test_timeout_type() {
        let timeout = Timeout::new(Duration::from_secs(10));
        // 验证类型
        let _timeout: Timeout = timeout;
    }

    #[test]
    fn test_timeout_size() {
        use std::mem::size_of;
        let size = size_of::<Timeout>();
        assert!(size > 0);
    }

    // ==================== server feature 集成测试 ====================

    #[cfg(feature = "server")]
    #[tokio::test]
    async fn test_timeout_with_route() {
        use crate::route::Route;

        let route = Route::new("/")
            .hook(Timeout::new(Duration::from_secs(5)))
            .get(|_req: Request| async {
                tokio::time::sleep(Duration::from_millis(100)).await;
                Ok("success")
            });

        let route = Route::new_root().append(route);
        let req = Request::empty();

        let result: Result<Response> = route.call(req).await;
        assert!(result.is_ok());
    }

    #[cfg(feature = "server")]
    #[tokio::test]
    async fn test_timeout_expires_with_route() {
        use crate::route::Route;

        let route = Route::new("/")
            .hook(Timeout::new(Duration::from_millis(50)))
            .get(|_req: Request| async {
                tokio::time::sleep(Duration::from_secs(1)).await;
                Ok("will timeout")
            });

        let route = Route::new_root().append(route);
        let req = Request::empty();

        let result: Result<Response> = route.call(req).await;
        assert!(result.is_err());
        if let Err(e) = result {
            assert_eq!(e.status(), StatusCode::REQUEST_TIMEOUT);
        }
    }

    #[cfg(feature = "server")]
    #[tokio::test]
    async fn test_timeout_just_in_time_with_route() {
        use crate::route::Route;

        let route = Route::new("/")
            .hook(Timeout::new(Duration::from_millis(200)))
            .get(|_req: Request| async {
                tokio::time::sleep(Duration::from_millis(150)).await;
                Ok("just in time")
            });

        let route = Route::new_root().append(route);
        let req = Request::empty();

        let result: Result<Response> = route.call(req).await;
        assert!(result.is_ok());
    }

    #[cfg(feature = "server")]
    #[tokio::test]
    async fn test_timeout_concurrent_with_route() {
        use crate::route::Route;

        let route = Route::new("/")
            .hook(Timeout::new(Duration::from_secs(2)))
            .get(|_req: Request| async {
                tokio::time::sleep(Duration::from_millis(100)).await;
                Ok("concurrent")
            });

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

    // ==================== 非server模式测试 ====================

    #[cfg(not(feature = "server"))]
    #[test]
    fn test_timeout_not_server_mode() {
        let timeout = Timeout::new(Duration::from_secs(30));
        // 非server模式下，is_available 应该返回 false
        assert!(!timeout.is_available());
    }
}
