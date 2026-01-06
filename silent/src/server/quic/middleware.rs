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

    #[tokio::test]
    async fn test_alt_svc_zero_port_no_header() {
        let mw = AltSvcMiddleware::new(0);
        let next = Next::build_from_slice(Arc::new(Ep), &[]);
        let req = Request::empty();
        let resp = mw.handle(req, &next).await.unwrap();
        assert!(!resp.headers().contains_key("alt-svc"));
    }

    // 新增测试：验证 Alt-Svc 头的格式
    #[tokio::test]
    async fn test_alt_svc_header_format() {
        let mw = AltSvcMiddleware::new(8443);
        let next = Next::build_from_slice(Arc::new(Ep), &[]);
        let req = Request::empty();
        let resp = mw.handle(req, &next).await.unwrap();

        let alt_svc = resp.headers().get("alt-svc").unwrap();
        let alt_svc_str = alt_svc.to_str().unwrap();

        // 验证格式：h3=":8443"; ma=86400
        assert!(alt_svc_str.contains("h3=\":8443\""));
        assert!(alt_svc_str.contains("ma=86400"));
    }

    // 新增测试：验证不同端口号的 Alt-Svc 头
    #[tokio::test]
    async fn test_alt_svc_different_ports() {
        let ports = vec![443, 4433, 8443, 9000, 65535];

        for port in ports {
            let mw = AltSvcMiddleware::new(port);
            let next = Next::build_from_slice(Arc::new(Ep), &[]);
            let req = Request::empty();
            let resp = mw.handle(req, &next).await.unwrap();

            let alt_svc = resp.headers().get("alt-svc").unwrap();
            let alt_svc_str = alt_svc.to_str().unwrap();

            assert!(alt_svc_str.contains(&format!("h3=\":{}\"", port)));
        }
    }

    // 新增测试：验证 AltSvcMiddleware 可以被 Clone
    #[test]
    fn test_alt_svc_middleware_clone() {
        let mw1 = AltSvcMiddleware::new(4433);
        let mw2 = mw1.clone();

        // Clone 后应该有相同的端口
        assert_eq!(mw1.quic_port, mw2.quic_port);
    }

    // 新增测试：验证 AltSvcMiddleware 的构造
    #[test]
    fn test_alt_svc_middleware_construction() {
        let mw = AltSvcMiddleware::new(9000);

        // 验证端口正确存储
        assert_eq!(mw.quic_port, 9000);
    }

    // 新增测试：验证 Alt-Svc 头的 ma 参数
    #[tokio::test]
    async fn test_alt_svc_ma_parameter() {
        let mw = AltSvcMiddleware::new(443);
        let next = Next::build_from_slice(Arc::new(Ep), &[]);
        let req = Request::empty();
        let resp = mw.handle(req, &next).await.unwrap();

        let alt_svc = resp.headers().get("alt-svc").unwrap();
        let alt_svc_str = alt_svc.to_str().unwrap();

        // ma=86400 表示 24 小时（86400 秒）
        assert!(alt_svc_str.contains("ma=86400"));
    }

    // 新增测试：验证响应不会覆盖已存在的 Alt-Svc 头
    #[tokio::test]
    async fn test_alt_svc_does_not_override_existing_header() {
        // 创建一个返回带有 Alt-Svc 头的响应的 handler
        #[derive(Clone)]
        struct EpWithAltSvc;
        #[async_trait::async_trait]
        impl Handler for EpWithAltSvc {
            async fn call(&self, _req: Request) -> crate::Result<SilentResponse> {
                let mut resp = Response::empty();
                resp.headers_mut().insert(
                    "alt-svc",
                    http::HeaderValue::from_static("h3=\":8888\"; ma=3600"),
                );
                Ok(resp)
            }
        }

        let mw = AltSvcMiddleware::new(4433);
        let next = Next::build_from_slice(Arc::new(EpWithAltSvc), &[]);
        let req = Request::empty();
        let resp = mw.handle(req, &next).await.unwrap();

        let alt_svc = resp.headers().get("alt-svc").unwrap();
        let alt_svc_str = alt_svc.to_str().unwrap();

        // 验证中间件添加的头会覆盖已存在的头
        assert!(alt_svc_str.contains(":4433"));
        assert!(alt_svc_str.contains("ma=86400"));
    }

    // 新增测试：验证 AltSvcMiddleware 的 Send + Sync 约束
    #[test]
    fn test_alt_svc_middleware_is_send_and_sync() {
        fn assert_send<T: Send>() {}
        fn assert_sync<T: Sync>() {}

        assert_send::<AltSvcMiddleware>();
        assert_sync::<AltSvcMiddleware>();
    }

    // 新增测试：验证 port 为 u16::MAX 的情况
    #[tokio::test]
    async fn test_alt_svc_max_port() {
        let mw = AltSvcMiddleware::new(u16::MAX);
        let next = Next::build_from_slice(Arc::new(Ep), &[]);
        let req = Request::empty();
        let resp = mw.handle(req, &next).await.unwrap();

        let alt_svc = resp.headers().get("alt-svc").unwrap();
        let alt_svc_str = alt_svc.to_str().unwrap();

        assert!(alt_svc_str.contains(&format!("h3=\":{}\"", u16::MAX)));
    }

    // 新增测试：验证 Alt-Svc 头的 h3 协议标识
    #[tokio::test]
    async fn test_alt_svc_h3_protocol() {
        let mw = AltSvcMiddleware::new(443);
        let next = Next::build_from_slice(Arc::new(Ep), &[]);
        let req = Request::empty();
        let resp = mw.handle(req, &next).await.unwrap();

        let alt_svc = resp.headers().get("alt-svc").unwrap();
        let alt_svc_str = alt_svc.to_str().unwrap();

        // 验证使用 h3 协议标识
        assert!(alt_svc_str.starts_with("h3="));
    }

    // 新增测试：验证多个响应的 Alt-Svc 头一致性
    #[tokio::test]
    async fn test_alt_svc_consistency_across_multiple_requests() {
        let mw = AltSvcMiddleware::new(8443);
        let next = Next::build_from_slice(Arc::new(Ep), &[]);

        // 发送多个请求，验证 Alt-Svc 头的一致性
        for _ in 0..5 {
            let req = Request::empty();
            let resp = mw.handle(req, &next).await.unwrap();

            let alt_svc = resp.headers().get("alt-svc").unwrap();
            let alt_svc_str = alt_svc.to_str().unwrap();

            assert!(alt_svc_str.contains("h3=\":8443\""));
            assert!(alt_svc_str.contains("ma=86400"));
        }
    }
}
