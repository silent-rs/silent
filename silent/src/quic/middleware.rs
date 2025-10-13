use crate::Next;
use crate::Request;
use crate::Response as SilentResponse;
use crate::{Handler, MiddleWareHandler};

/// Alt-Svc 中间件，用于通知客户端可以使用 HTTP/3
///
/// 该中间件会在 HTTP/1.1 或 HTTP/2 响应中添加 Alt-Svc 头，
/// 告知客户端可以通过指定端口使用 HTTP/3 (QUIC) 协议。
#[derive(Clone)]
pub struct AltSvcMiddleware {
    quic_port: u16,
}

impl AltSvcMiddleware {
    /// 创建新的 Alt-Svc 中间件
    ///
    /// # 参数
    /// - `quic_port`: QUIC 服务监听的端口号
    ///
    /// # 示例
    /// ```no_run
    /// use silent::quic::AltSvcMiddleware;
    /// use silent::prelude::*;
    ///
    /// let routes = Route::new("")
    ///     .hook(AltSvcMiddleware::new(4433));
    /// ```
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
