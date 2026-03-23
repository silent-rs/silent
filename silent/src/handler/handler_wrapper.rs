use crate::core::into_response::IntoResponse;
use crate::handler::handler_trait::Handler;
use crate::{Request, Response, Result};
use async_trait::async_trait;
use std::future::Future;
use std::sync::Arc;

/// 处理器包装结构体
///
/// 将返回 `Result<T>` 的异步函数包装为 `Handler`，
/// 其中 `T` 需实现 `IntoResponse`。
pub struct HandlerWrapper<F> {
    handler: F,
}

impl<F, T, Fut> HandlerWrapper<F>
where
    Fut: Future<Output = Result<T>> + Send + 'static,
    F: Fn(Request) -> Fut,
    T: IntoResponse,
{
    pub fn new(handler: F) -> Self {
        HandlerWrapper { handler }
    }

    pub async fn handle(&self, req: Request) -> Result<Response> {
        Ok((self.handler)(req).await?.into_response())
    }

    pub fn arc(self) -> Arc<Self> {
        Arc::new(self)
    }
}

/// 为HandlerWrapper实现Handler
#[async_trait]
impl<F, T, Fut> Handler for HandlerWrapper<F>
where
    Fut: Future<Output = Result<T>> + Send + 'static,
    F: Fn(Request) -> Fut + Send + Sync + 'static,
    T: IntoResponse,
{
    async fn call(&self, req: Request) -> Result<Response> {
        self.handle(req).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Request, Result};
    use http_body_util::BodyExt;
    use serde::{Deserialize, Serialize};

    async fn hello_world(_req: Request) -> Result<String> {
        Ok("Hello World".into())
    }

    #[derive(Debug, Serialize, Deserialize)]
    struct HelloHandler {
        name: String,
    }

    async fn hello_world_2(_req: Request) -> Result<HelloHandler> {
        Ok(HelloHandler {
            name: "Hello World".to_string(),
        })
    }

    #[tokio::test]
    async fn handler_wrapper_works() {
        let handler_wrapper = HandlerWrapper::new(hello_world);
        let res_l = handler_wrapper
            .handle(Request::empty())
            .await
            .unwrap()
            .body
            .frame()
            .await
            .unwrap()
            .unwrap()
            .into_data()
            .unwrap();
        assert_eq!(res_l, "Hello World");
    }

    #[tokio::test]
    async fn handler_wrapper_struct_works() {
        let handler_wrapper = HandlerWrapper::new(hello_world_2);
        let hello = HelloHandler {
            name: "Hello World".to_string(),
        };
        let res_l = handler_wrapper
            .handle(Request::empty())
            .await
            .unwrap()
            .body
            .frame()
            .await
            .unwrap()
            .unwrap()
            .into_data()
            .unwrap();
        assert_eq!(res_l, serde_json::to_string(&hello).unwrap());
    }
}
