use crate::{Handler, MiddleWareHandler, Next, Request, Response, Result, SilentError};
use async_trait::async_trait;
use http::StatusCode;
use std::time::Duration;

/// ExceptionHandler 中间件
/// ```rust
/// use silent::prelude::*;
/// use silent::middlewares::{RequestTimeLogger};
/// // Define a request time logger middleware
/// let _ = RequestTimeLogger::new();
#[derive(Default, Clone)]
pub struct Timeout {
    timeout: Duration,
}

impl Timeout {
    pub fn new(timeout: Duration) -> Self {
        Self { timeout }
    }
}

#[async_trait]
impl MiddleWareHandler for Timeout {
    async fn handle(&self, req: Request, next: &Next) -> Result<Response> {
        match crate::runtime::timeout(self.timeout, next.call(req))
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
