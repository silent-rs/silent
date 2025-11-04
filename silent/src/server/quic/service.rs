use tracing::{error, info, warn};

use super::core::{QuicSession, WebTransportHandler, WebTransportStream};
use crate::route::Route;
use crate::server::protocol::Protocol as _;
use crate::server::protocol::hyper_http::HyperHttpProtocol;
use crate::{Handler, Request};
use anyhow::{Context, Result, anyhow};
use bytes::{Buf, Bytes, BytesMut};
use h3::ext::Protocol as H3Protocol;
use h3::server::{RequestResolver, RequestStream};
use h3_quinn::Connection as H3QuinnConnection;
use http::{Method, Request as HttpRequest, Response, StatusCode};
use http_body_util::BodyExt;
use std::{net::SocketAddr, sync::Arc};

pub(crate) async fn handle_quic_connection(
    incoming: quinn::Incoming,
    routes: Arc<Route>,
) -> Result<()> {
    info!("准备建立 QUIC 连接");
    let connection = incoming.await.context("等待 QUIC 连接建立失败")?;
    let remote = connection.remote_address();
    info!(%remote, "客户端连接建立");

    let handler = Arc::new(super::echo::EchoHandler);

    let mut builder = h3::server::builder();
    builder
        .enable_extended_connect(true)
        .enable_datagram(true)
        .enable_webtransport(true)
        .max_webtransport_sessions(32);
    let mut h3_conn = builder
        .build(H3QuinnConnection::new(connection.clone()))
        .await
        .context("构建 HTTP/3 连接失败")?;

    loop {
        match h3_conn.accept().await {
            Ok(Some(resolver)) => {
                let routes = Arc::clone(&routes);
                let handler = Arc::clone(&handler);
                tokio::spawn(async move {
                    if let Err(err) = handle_request(resolver, remote, routes, handler).await {
                        error!(%remote, error = ?err, "处理 HTTP/3 请求失败");
                    }
                });
            }
            Ok(None) => break,
            Err(err) => {
                warn!(%remote, error = ?err, "HTTP/3 连接异常结束");
                break;
            }
        }
    }

    info!(%remote, "客户端连接结束");
    Ok(())
}

/// 内部测试缝隙：HTTP/3 请求-响应通道最小能力
///
/// 仅用于本文件内部，帮助在不依赖真实 h3::RequestStream 的情况下做单测。
/// 保持最小必要方法集合以避免泄露协议细节。
trait H3RequestIo: Send {
    fn recv_data<'a>(
        &'a mut self,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Option<Bytes>>> + Send + 'a>>;
    fn send_response<'a>(
        &'a mut self,
        resp: Response<()>,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<()>> + Send + 'a>>;
    fn send_data<'a>(
        &'a mut self,
        data: Bytes,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<()>> + Send + 'a>>;
    fn finish<'a>(
        &'a mut self,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<()>> + Send + 'a>>;
}

// 真实 H3 RequestStream 到 H3StreamIo 的适配器
struct RealH3Stream(RequestStream<h3_quinn::BidiStream<Bytes>, Bytes>);

impl RealH3Stream {
    fn new(inner: RequestStream<h3_quinn::BidiStream<Bytes>, Bytes>) -> Self {
        Self(inner)
    }
}

impl H3RequestIo for RealH3Stream {
    fn recv_data<'a>(
        &'a mut self,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Option<Bytes>>> + Send + 'a>>
    {
        Box::pin(async move {
            match self.0.recv_data().await {
                Ok(Some(mut chunk)) => Ok(Some(chunk.copy_to_bytes(chunk.remaining()))),
                Ok(None) => Ok(None),
                Err(e) => Err(anyhow!("读取 HTTP/3 请求体失败: {e}")),
            }
        })
    }
    fn send_response<'a>(
        &'a mut self,
        resp: Response<()>,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<()>> + Send + 'a>> {
        Box::pin(async move {
            self.0
                .send_response(resp)
                .await
                .map_err(|e| anyhow!("发送 HTTP/3 响应头失败: {e}"))
        })
    }
    fn send_data<'a>(
        &'a mut self,
        data: Bytes,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<()>> + Send + 'a>> {
        Box::pin(async move {
            self.0
                .send_data(data)
                .await
                .map_err(|e| anyhow!("发送 HTTP/3 响应数据失败: {e}"))
        })
    }
    fn finish<'a>(
        &'a mut self,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<()>> + Send + 'a>> {
        Box::pin(async move {
            self.0
                .finish()
                .await
                .map_err(|e| anyhow!("结束 HTTP/3 响应失败: {e}"))
        })
    }
}

async fn handle_request(
    resolver: RequestResolver<H3QuinnConnection, Bytes>,
    remote: SocketAddr,
    routes: Arc<Route>,
    handler: Arc<dyn WebTransportHandler>,
) -> Result<()> {
    let (request, stream) = resolver
        .resolve_request()
        .await
        .map_err(|err| anyhow!("解析 HTTP/3 请求失败: {err}"))?;
    let protocol = request.extensions().get::<H3Protocol>().cloned();
    if request.method() == Method::CONNECT && matches!(protocol, Some(H3Protocol::WEB_TRANSPORT)) {
        handle_webtransport_request(request, stream, remote, handler).await
    } else {
        handle_http3_request(request, stream, remote, routes).await
    }
}

async fn handle_http3_request(
    request: HttpRequest<()>,
    stream: RequestStream<h3_quinn::BidiStream<Bytes>, Bytes>,
    remote: SocketAddr,
    routes: Arc<Route>,
) -> Result<()> {
    let mut stream = RealH3Stream::new(stream);
    handle_http3_request_impl(request, &mut stream, remote, routes).await
}

// 提取后的实现，便于在测试中注入自定义流
async fn handle_http3_request_impl(
    request: HttpRequest<()>,
    stream: &mut dyn H3RequestIo,
    remote: SocketAddr,
    routes: Arc<Route>,
) -> Result<()> {
    let mut body_buf = BytesMut::new();
    while let Some(bytes) = stream.recv_data().await? {
        if !bytes.is_empty() {
            body_buf.extend_from_slice(&bytes);
        }
    }
    let (parts, _) = request.into_parts();
    let body = if body_buf.is_empty() {
        crate::prelude::ReqBody::Empty
    } else {
        crate::prelude::ReqBody::Once(body_buf.freeze())
    };
    let mut silent_req = Request::from_parts(parts, body);
    silent_req.set_remote(remote.into());
    let response = Handler::call(&*routes, silent_req)
        .await
        .unwrap_or_else(Into::into);
    let hyper_response = HyperHttpProtocol::from_internal(response);
    let (parts, mut body) = hyper_response.into_parts();
    stream
        .send_response(Response::from_parts(parts, ()))
        .await?;
    while let Some(frame) = body.frame().await {
        let frame = frame.map_err(|err| anyhow!("读取响应体失败: {err}"))?;
        if let Ok(data) = frame.into_data() {
            if data.is_empty() {
                continue;
            }
            stream.send_data(data).await?;
        }
    }
    stream.finish().await?;
    Ok(())
}

async fn handle_webtransport_request(
    request: HttpRequest<()>,
    mut stream: RequestStream<h3_quinn::BidiStream<Bytes>, Bytes>,
    remote: SocketAddr,
    handler: Arc<dyn WebTransportHandler>,
) -> Result<()> {
    let handshake = build_webtransport_handshake_response(&request);
    stream
        .send_response(handshake)
        .await
        .map_err(|err| anyhow!("发送 WebTransport 握手响应失败: {err}"))?;
    info!(%remote, "WebTransport 会话建立");
    let session = Arc::new(QuicSession::new(remote));
    let mut channel = WebTransportStream::new(stream);
    handler.handle(session, &mut channel).await
}

fn build_webtransport_handshake_response(request: &HttpRequest<()>) -> Response<()> {
    let draft_header = request
        .headers()
        .get("sec-webtransport-http3-draft")
        .cloned();
    let mut response_builder = Response::builder().status(StatusCode::OK);
    if let Some(value) = draft_header {
        response_builder = response_builder.header("sec-webtransport-http3-draft", value);
    }
    response_builder.body(()).unwrap()
}

#[cfg(all(test, feature = "quic"))]
mod tests {
    use super::{H3RequestIo, build_webtransport_handshake_response, handle_http3_request_impl};
    use crate::prelude::{ReqBody, Request as SilentRequest, ResBody};
    use crate::route::Route;
    use crate::{Method, Response as SilentResponse};
    use anyhow::anyhow;
    use bytes::Bytes;
    use http::Request as HttpRequest;
    use http::Response as HttpResponse;
    use http::StatusCode;
    use std::collections::VecDeque;
    use std::net::SocketAddr;
    use std::sync::Arc;

    // 伪造 H3 流，用于在不依赖真实 h3/quinn 的情况下测试 HTTP/3 处理路径
    struct FakeH3Stream {
        incoming: VecDeque<Bytes>,
        pub sent_head: Option<HttpResponse<()>>,
        pub sent_data: Vec<Bytes>,
        pub finished: bool,
        fail_on_send_head: bool,
    }

    impl FakeH3Stream {
        fn new(frames: Vec<Bytes>) -> Self {
            Self {
                incoming: frames.into(),
                sent_head: None,
                sent_data: Vec::new(),
                finished: false,
                fail_on_send_head: false,
            }
        }
    }

    impl H3RequestIo for FakeH3Stream {
        fn recv_data<'a>(
            &'a mut self,
        ) -> std::pin::Pin<
            Box<dyn std::future::Future<Output = anyhow::Result<Option<Bytes>>> + Send + 'a>,
        > {
            Box::pin(async move { Ok(self.incoming.pop_front()) })
        }
        fn send_response<'a>(
            &'a mut self,
            resp: HttpResponse<()>,
        ) -> std::pin::Pin<Box<dyn std::future::Future<Output = anyhow::Result<()>> + Send + 'a>>
        {
            Box::pin(async move {
                if self.fail_on_send_head {
                    return Err(anyhow!("send_head_failed"));
                }
                self.sent_head = Some(resp);
                Ok(())
            })
        }
        fn send_data<'a>(
            &'a mut self,
            data: Bytes,
        ) -> std::pin::Pin<Box<dyn std::future::Future<Output = anyhow::Result<()>> + Send + 'a>>
        {
            Box::pin(async move {
                self.sent_data.push(data);
                Ok(())
            })
        }
        fn finish<'a>(
            &'a mut self,
        ) -> std::pin::Pin<Box<dyn std::future::Future<Output = anyhow::Result<()>> + Send + 'a>>
        {
            Box::pin(async move {
                self.finished = true;
                Ok(())
            })
        }
    }

    fn make_routes_echo_body() -> Arc<Route> {
        let route = Route::new_root().post(|mut req: SilentRequest| async move {
            // 直接把 silent 聚合的 body 原样返回
            match req.take_body() {
                ReqBody::Once(b) => {
                    let mut resp = SilentResponse::empty();
                    resp.set_body(ResBody::from(b));
                    Ok(resp)
                }
                ReqBody::Empty => Ok(SilentResponse::from("")),
                other => {
                    // 其余分支在当前实现不会出现，防御性处理
                    let bytes = http_body_util::BodyExt::collect(other).await?.to_bytes();
                    let mut resp = SilentResponse::empty();
                    resp.set_body(ResBody::from(bytes));
                    Ok(resp)
                }
            }
        });
        Arc::new(route)
    }

    fn make_request(path: &str) -> HttpRequest<()> {
        HttpRequest::builder()
            .method(Method::POST)
            .uri(path)
            .body(())
            .unwrap()
    }

    #[tokio::test]
    async fn test_http3_impl_basic_body_roundtrip() {
        let mut stream = FakeH3Stream::new(vec![
            Bytes::from_static(b"hello "),
            Bytes::from_static(b"world"),
        ]);
        let routes = make_routes_echo_body();
        let req = make_request("/");
        let remote: SocketAddr = "127.0.0.1:34567".parse().unwrap();

        handle_http3_request_impl(req, &mut stream, remote, routes)
            .await
            .expect("http3 impl should succeed");

        // 校验响应头已发送且结束标记被设置
        assert!(stream.sent_head.is_some());
        assert!(stream.finished);
        // 回显的响应体应当与请求体一致（一次或多次 data 帧）
        let body = stream.sent_data.iter().fold(Vec::new(), |mut acc, b| {
            acc.extend_from_slice(b);
            acc
        });
        assert_eq!(body, b"hello world".to_vec());
    }

    #[tokio::test]
    async fn test_http3_impl_empty_body() {
        let mut stream = FakeH3Stream::new(vec![]);
        let routes = make_routes_echo_body();
        let req = make_request("/");
        let remote: SocketAddr = "127.0.0.1:34568".parse().unwrap();

        handle_http3_request_impl(req, &mut stream, remote, routes)
            .await
            .expect("http3 impl should succeed on empty body");
        assert!(stream.sent_head.is_some());
        assert!(stream.finished);
        // 空请求体应当产生空响应体
        assert!(stream.sent_data.is_empty());
    }

    #[tokio::test]
    async fn test_http3_impl_head_send_error_propagates() {
        let mut stream = FakeH3Stream::new(vec![Bytes::from_static(b"abc")]);
        stream.fail_on_send_head = true;
        let routes = make_routes_echo_body();
        let req = make_request("/err");
        let remote: SocketAddr = "127.0.0.1:34569".parse().unwrap();

        let err = handle_http3_request_impl(req, &mut stream, remote, routes)
            .await
            .expect_err("should bubble up head send error");
        let msg = format!("{err:#}");
        assert!(msg.contains("send_head_failed"));
    }

    #[test]
    fn test_webtransport_handshake_header_propagation() {
        use http::HeaderValue;
        // 带 sec-webtransport-http3-draft 头
        let req = HttpRequest::builder()
            .method(Method::CONNECT)
            .uri("/")
            .header(
                "sec-webtransport-http3-draft",
                HeaderValue::from_static("draft02"),
            )
            .body(())
            .unwrap();
        let resp = build_webtransport_handshake_response(&req);
        assert_eq!(resp.status(), StatusCode::OK);
        assert_eq!(
            resp.headers()
                .get("sec-webtransport-http3-draft")
                .unwrap()
                .to_str()
                .unwrap(),
            "draft02"
        );

        // 不带草案头也应返回 200，且无该响应头
        let req2 = HttpRequest::builder()
            .method(Method::CONNECT)
            .uri("/")
            .body(())
            .unwrap();
        let resp2 = build_webtransport_handshake_response(&req2);
        assert_eq!(resp2.status(), StatusCode::OK);
        assert!(
            resp2
                .headers()
                .get("sec-webtransport-http3-draft")
                .is_none()
        );
    }
}
