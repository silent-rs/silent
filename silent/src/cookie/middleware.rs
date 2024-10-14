use crate::{Handler, MiddleWareHandler, Next, Request, Response, Result, SilentError};
use async_trait::async_trait;
use cookie::{Cookie, CookieJar};
use http::{header, StatusCode};

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
        let mut jar = CookieJar::new();
        if let Some(cookies) = req.headers().get(header::COOKIE) {
            for cookie_str in cookies
                .to_str()
                .map_err(|e| {
                    SilentError::business_error(
                        StatusCode::BAD_REQUEST,
                        format!("Failed to parse cookie: {}", e),
                    )
                })?
                .split(';')
                .map(|s| s.trim())
            {
                if let Ok(cookie) = Cookie::parse_encoded(cookie_str).map(|c| c.into_owned()) {
                    jar.add_original(cookie);
                }
            }
        }
        req.extensions_mut().insert(jar.clone());
        let mut res = next.call(req).await?;
        if let Some(cookie_jar) = res.extensions().get::<CookieJar>() {
            for cookie in cookie_jar.delta().cloned() {
                jar.add(cookie)
            }
            res.extensions_mut().insert(jar);
        } else {
            res.extensions_mut().insert(jar);
        };
        Ok(res)
    }
}
