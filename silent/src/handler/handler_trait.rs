use std::collections::HashMap;

use crate::core::res_body::ResBody;
use crate::{Request, Response, Result, SilentError};
use async_trait::async_trait;
use http::{Method, StatusCode};
use std::sync::Arc;

#[async_trait]
pub trait Handler: Send + Sync + 'static {
    async fn call(&self, _req: Request) -> Result<Response>;
}

#[async_trait]
impl Handler for HashMap<Method, Arc<dyn Handler>> {
    async fn call(&self, req: Request) -> Result<Response> {
        let method = req.method().clone();
        // 直接命中匹配的方法
        if let Some(handler) = self.clone().get(&method) {
            let mut pre_res = Response::empty();
            pre_res.configs = req.configs();
            pre_res.copy_from_response(handler.call(req).await?);
            return Ok(pre_res);
        }

        // 特殊处理：HEAD 无显式处理器时回退到 GET，并清空响应体
        if method == http::Method::HEAD
            && let Some(get_handler) = self.clone().get(&http::Method::GET)
        {
            let mut pre_res = Response::empty();
            pre_res.configs = req.configs();
            pre_res.copy_from_response(get_handler.call(req).await?);
            pre_res.set_body(ResBody::None);
            return Ok(pre_res);
        }

        Err(SilentError::business_error(
            StatusCode::METHOD_NOT_ALLOWED,
            "method not allowed".to_string(),
        ))
    }
}
