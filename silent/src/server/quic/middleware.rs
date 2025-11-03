use crate::Next;
use crate::Request;
use crate::Response as SilentResponse;
use crate::{Handler, MiddleWareHandler};

/// Alt-Svc 中间件，用于通知客户端可以使用 HTTP/3
#[derive(Clone)]
pub struct AltSvcMiddleware {
    quic_port: u16,
}

impl AltSvcMiddleware {
    pub fn new(quic_port: u16) -> Self {
        Self { quic_port }
    }
}

#[async_trait::async_trait]
impl MiddleWareHandler for AltSvcMiddleware {
    async fn handle(&self, req: Request, next: &Next) -> crate::Result<SilentResponse> {
        let mut response = next.call(req).await?;
        let port = self.quic_port;
        if port != 0 {
            let val = format!("h3=\":{}\"; ma=86400", port);
            if let Ok(h) = http::HeaderValue::from_str(&val) {
                response.headers_mut().insert("alt-svc", h);
            }
        }
        Ok(response)
    }
}
