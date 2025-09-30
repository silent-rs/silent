use crate::{Handler, MiddleWareHandler, Request, Response};
use async_trait::async_trait;
use std::sync::Arc;

/// The `Next` struct is used to chain multiple middlewares and endpoints together.
#[derive(Clone)]
pub struct Next {
    inner: NextInstance,
    next: Option<Arc<Next>>,
}

#[derive(Clone)]
pub(crate) enum NextInstance {
    Middleware(Arc<dyn MiddleWareHandler>),
    EndPoint(Arc<dyn Handler>),
}

impl Next {
    pub(crate) fn build_from_slice(
        endpoint: Arc<dyn Handler>,
        middlewares: &[Arc<dyn MiddleWareHandler>],
    ) -> Self {
        let mut next = Next {
            inner: NextInstance::EndPoint(endpoint),
            next: None,
        };
        if middlewares.is_empty() {
            return next;
        }
        for mw in middlewares.iter().rev() {
            next = Next {
                inner: NextInstance::Middleware(Arc::clone(mw)),
                next: Some(Arc::new(next)),
            };
        }
        next
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn build(
        endpoint: Arc<dyn Handler>,
        middlewares: Vec<Arc<dyn MiddleWareHandler>>,
    ) -> Self {
        Self::build_from_slice(endpoint, middlewares.as_slice())
    }
}

#[async_trait]
impl Handler for Next {
    async fn call(&self, req: Request) -> crate::Result<Response> {
        match &self.inner {
            NextInstance::Middleware(mw) => {
                mw.handle(req, self.next.clone().unwrap().as_ref()).await
            }
            NextInstance::EndPoint(ep) => ep.call(req).await,
        }
    }
}

#[async_trait]
impl MiddleWareHandler for Next {
    async fn handle(&self, req: Request, next: &Next) -> crate::Result<Response> {
        next.call(req).await
    }
}
