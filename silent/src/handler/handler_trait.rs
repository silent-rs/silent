use std::collections::HashMap;

use crate::{Request, Response, Result, SilentError};
use async_trait::async_trait;
use http::{Method, StatusCode};
use std::sync::Arc;

#[async_trait]
pub trait Handler: Send + Sync + 'static {
    async fn match_req(&self, _req: &Request) -> bool {
        true
    }

    async fn call(&self, _req: Request) -> Result<Response>;
}

#[async_trait]
impl Handler for HashMap<Method, Arc<dyn Handler>> {
    async fn call(&self, req: Request) -> Result<Response> {
        match self.clone().get(req.method()) {
            None => Err(SilentError::business_error(
                StatusCode::METHOD_NOT_ALLOWED,
                "method not allowed".to_string(),
            )),
            Some(handler) => {
                let mut pre_res = Response::empty();
                pre_res.configs = req.configs();
                pre_res.copy_from_response(handler.call(req).await?);
                Ok(pre_res)
            }
        }
    }
}
