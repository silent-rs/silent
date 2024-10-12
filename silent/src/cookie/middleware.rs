use crate::{CookieExt, Handler, MiddleWareHandler, Next, Request, Response, Result};
use async_trait::async_trait;

#[derive(Debug, Default)]
pub struct CookieMiddleware {}

impl CookieMiddleware {
    pub(crate) fn new() -> Self {
        Self::default()
    }
}

#[async_trait]
impl MiddleWareHandler for CookieMiddleware {
    async fn handle(&self, mut req: Request, next: &Next) -> Result<Response> {
        let cookies = req.cookies().clone();
        req.extensions_mut().insert(cookies.clone());
        let mut res = next.call(req).await?;
        res.extensions_mut().insert(cookies);
        Ok(res)
    }
}
