use async_trait::async_trait;
use std::future::Future;
use std::sync::Arc;

use crate::{Handler, Request, Response, Result};

/// 泛型处理器包装器：让直接传入闭包保持静态分发，不再经过额外的 HandlerWrapper。
pub struct HandlerFn<F> {
    func: F,
}

impl<F> HandlerFn<F> {
    pub fn new(func: F) -> Self {
        Self { func }
    }

    pub fn arc(self) -> Arc<Self> {
        Arc::new(self)
    }
}

#[async_trait]
impl<F, Fut> Handler for HandlerFn<F>
where
    F: Fn(Request) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Response> + Send + 'static,
{
    async fn call(&self, req: Request) -> Result<Response> {
        let resp = (self.func)(req).await;
        Ok(resp)
    }
}
