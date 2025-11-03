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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::next::Next;
    use crate::{Handler, Response};
    use std::sync::Arc;

    #[derive(Clone)]
    struct Ep;
    #[async_trait::async_trait]
    impl Handler for Ep {
        async fn call(&self, _req: Request) -> crate::Result<SilentResponse> {
            Ok(Response::empty())
        }
    }

    #[tokio::test]
    async fn test_alt_svc_injected() {
        let mw = AltSvcMiddleware::new(4433);
        // 构造 next 链，仅包含一个空 endpoint
        let next = Next::build_from_slice(Arc::new(Ep), &[]);
        let req = Request::empty();
        let resp = mw.handle(req, &next).await.unwrap();
        assert!(resp.headers().contains_key("alt-svc"));
    }
}
