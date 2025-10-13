use crate::Next;
use crate::Request;
use crate::Response as SilentResponse;
use crate::{Handler, MiddleWareHandler};

#[derive(Clone)]
pub(crate) struct AltSvcMiddleware {
    quic_port: u16,
}

impl AltSvcMiddleware {
    pub(crate) fn new(quic_port: u16) -> Self {
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
