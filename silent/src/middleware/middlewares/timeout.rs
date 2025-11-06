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
