use crate::core::next::Next;
use crate::{Request, Response, Result};
use async_trait::async_trait;

#[async_trait]
pub trait MiddleWareHandler: Send + Sync + 'static {
    async fn handle(&self, _req: Request, _next: &Next) -> Result<Response>;
}

#[cfg(test)]
mod tests {
    use crate::{Handler, HandlerWrapper};
    use std::sync::Arc;
    use tracing::info;

    use super::*;

    struct TestMiddleWare {
        count: u32,
    }

    #[async_trait]
    impl MiddleWareHandler for TestMiddleWare {
        async fn handle(&self, req: Request, next: &Next) -> Result<Response> {
            info!("{}", self.count);
            next.call(req).await
        }
    }

    async fn hello_world(_req: Request) -> Result<String> {
        Ok("Hello World".into())
    }

    #[tokio::test]
    async fn test_middleware() -> Result<()> {
        let handler_wrapper = HandlerWrapper::new(hello_world).arc();
        let middleware1: Arc<dyn MiddleWareHandler> = Arc::new(TestMiddleWare { count: 1 });
        let middleware2: Arc<dyn MiddleWareHandler> = Arc::new(TestMiddleWare { count: 2 });
        let req = Request::empty();
        let middlewares = Next::build(handler_wrapper, &[middleware1, middleware2]);
        let res = middlewares.call(req).await;
        assert!(res.is_ok());
        info!("{:?}", res?);
        Ok(())
    }
}
