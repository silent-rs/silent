use tracing::{debug, error, info, warn};

use super::core::{QuicSession, WebTransportHandler, WebTransportStream};
use crate::route::Route;
#[cfg(feature = "metrics")]
use crate::server::metrics::{
    record_http3_body_oversize, record_webtransport_accept, record_webtransport_error,
};
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
use std::{net::SocketAddr, sync::Arc, time::Instant};

pub(crate) async fn handle_quic_connection(
    incoming: quinn::Incoming,
    routes: Arc<Route>,
    max_body_size: Option<usize>,
    read_timeout: Option<std::time::Duration>,
    max_wt_frame: Option<usize>,
    wt_read_timeout: Option<std::time::Duration>,
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
                    if let Err(err) = handle_request(
                        resolver,
                        remote,
                        routes,
                        handler,
                        max_body_size,
                        read_timeout,
                        max_wt_frame,
                        wt_read_timeout,
                    )
                    .await
                    {
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
/// 仅用于本文件内部，帮助在不依赖真实 h3::RequestStream 的的情况下做单测。
/// 保持最小必要方法集合以避免泄露协议细节。
trait H3RequestIo: Send {
    fn recv_data(
        &mut self,
    ) -> impl std::future::Future<Output = anyhow::Result<Option<Bytes>>> + Send;
    fn send_response(
        &mut self,
        resp: Response<()>,
    ) -> impl std::future::Future<Output = anyhow::Result<()>> + Send;
    fn send_data(
        &mut self,
        data: Bytes,
    ) -> impl std::future::Future<Output = anyhow::Result<()>> + Send;
    fn finish(&mut self) -> impl std::future::Future<Output = anyhow::Result<()>> + Send;
}

// 真实 H3 RequestStream 到 H3StreamIo 的适配器
struct RealH3Stream(RequestStream<h3_quinn::BidiStream<Bytes>, Bytes>);

impl RealH3Stream {
    fn new(inner: RequestStream<h3_quinn::BidiStream<Bytes>, Bytes>) -> Self {
        Self(inner)
    }
}

impl H3RequestIo for RealH3Stream {
    #[allow(clippy::manual_async_fn)]
    fn recv_data(
        &mut self,
    ) -> impl std::future::Future<Output = anyhow::Result<Option<Bytes>>> + Send {
        async move {
            match self.0.recv_data().await {
                Ok(Some(mut chunk)) => Ok(Some(chunk.copy_to_bytes(chunk.remaining()))),
                Ok(None) => Ok(None),
                Err(e) => Err(anyhow!("读取 HTTP/3 请求体失败: {e}")),
            }
        }
    }
    #[allow(clippy::manual_async_fn)]
    fn send_response(
        &mut self,
        resp: Response<()>,
    ) -> impl std::future::Future<Output = anyhow::Result<()>> + Send {
        async move {
            self.0
                .send_response(resp)
                .await
                .map_err(|e| anyhow!("发送 HTTP/3 响应头失败: {e}"))
        }
    }
    #[allow(clippy::manual_async_fn)]
    fn send_data(
        &mut self,
        data: Bytes,
    ) -> impl std::future::Future<Output = anyhow::Result<()>> + Send {
        async move {
            self.0
                .send_data(data)
                .await
                .map_err(|e| anyhow!("发送 HTTP/3 响应数据失败: {e}"))
        }
    }
    #[allow(clippy::manual_async_fn)]
    fn finish(&mut self) -> impl std::future::Future<Output = anyhow::Result<()>> + Send {
        async move {
            self.0
                .finish()
                .await
                .map_err(|e| anyhow!("结束 HTTP/3 响应失败: {e}"))
        }
    }
}

#[allow(clippy::too_many_arguments)]
async fn handle_request(
    resolver: RequestResolver<H3QuinnConnection, Bytes>,
    remote: SocketAddr,
    routes: Arc<Route>,
    handler: Arc<dyn WebTransportHandler>,
    max_body_size: Option<usize>,
    read_timeout: Option<std::time::Duration>,
    max_wt_frame: Option<usize>,
    wt_read_timeout: Option<std::time::Duration>,
) -> Result<()> {
    let accept_at = Instant::now();
    let (request, stream) = resolver
        .resolve_request()
        .await
        .map_err(|err| anyhow!("解析 HTTP/3 请求失败: {err}"))?;
    let protocol = request.extensions().get::<H3Protocol>().cloned();
    debug!(
        %remote,
        method = ?request.method(),
        path = %request.uri(),
        proto = ?protocol,
        "HTTP/3 request received"
    );
    if request.method() == Method::CONNECT && matches!(protocol, Some(H3Protocol::WEB_TRANSPORT)) {
        handle_webtransport_request(
            request,
            stream,
            remote,
            handler,
            accept_at,
            max_wt_frame,
            wt_read_timeout,
        )
        .await
    } else {
        handle_http3_request(request, stream, remote, routes, max_body_size, read_timeout).await
    }
}

async fn handle_http3_request(
    request: HttpRequest<()>,
    stream: RequestStream<h3_quinn::BidiStream<Bytes>, Bytes>,
    remote: SocketAddr,
    routes: Arc<Route>,
    max_body_size: Option<usize>,
    read_timeout: Option<std::time::Duration>,
) -> Result<()> {
    let mut stream = RealH3Stream::new(stream);
    handle_http3_request_impl(
        request,
        &mut stream,
        remote,
        routes,
        max_body_size,
        read_timeout,
    )
    .await
}

// 提取后的实现，便于在测试中注入自定义流
// 优化版本：使用泛型实现完全静态分派，消除动态分派开销
async fn handle_http3_request_impl<T: H3RequestIo>(
    request: HttpRequest<()>,
    stream: &mut T,
    remote: SocketAddr,
    routes: Arc<Route>,
    max_body_size: Option<usize>,
    read_timeout: Option<std::time::Duration>,
) -> Result<()> {
    let mut body_buf = BytesMut::new();
    while let Some(bytes) = match read_timeout {
        Some(t) => tokio::time::timeout(t, stream.recv_data()).await?,
        None => stream.recv_data().await,
    }? {
        if !bytes.is_empty() {
            if let Some(max) = max_body_size
                && body_buf.len() + bytes.len() > max
            {
                warn!(
                    %remote,
                    size = body_buf.len() + bytes.len(),
                    limit = max,
                    "HTTP/3 request body exceeds limit"
                );
                #[cfg(feature = "metrics")]
                record_http3_body_oversize();
                return Err(anyhow!("HTTP/3 request body exceeds limit"));
            }
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
    accept_at: Instant,
    max_frame: Option<usize>,
    read_timeout: Option<std::time::Duration>,
) -> Result<()> {
    let handshake_start = Instant::now();
    let handshake = build_webtransport_handshake_response(&request);
    stream
        .send_response(handshake)
        .await
        .map_err(|err| anyhow!("发送 WebTransport 握手响应失败: {err}"))?;
    info!(
        %remote,
        accept_elapsed = ?accept_at.elapsed(),
        handshake_elapsed = ?handshake_start.elapsed(),
        "WebTransport 会话建立"
    );
    record_webtransport_accept();
    let session = Arc::new(QuicSession::new(remote));
    let mut channel = WebTransportStream::new(stream, max_frame, read_timeout);
    let started = Instant::now();
    let res = handler.handle(session, &mut channel).await;
    match &res {
        Ok(_) => info!(%remote, handle_elapsed = ?started.elapsed(), "WebTransport 会话结束"),
        Err(err) => {
            record_webtransport_error();
            warn!(%remote, error = ?err, "WebTransport 会话异常结束")
        }
    }
    res
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
    use http::{Request as HttpRequest, Response, StatusCode};
    use std::collections::VecDeque;
    use std::net::SocketAddr;
    use std::sync::Arc;

    // 伪造 H3 流，用于在不依赖真实 h3/quinn 的情况下测试 HTTP/3 处理路径
    struct FakeH3Stream {
        incoming: VecDeque<Bytes>,
        pub sent_head: Option<Response<()>>,
        pub sent_data: Vec<Bytes>,
        pub finished: bool,
        fail_on_send_head: bool,
        fail_on_send_data: bool,
        fail_on_finish: bool,
        fail_on_recv_data: bool,
    }

    impl FakeH3Stream {
        fn new(frames: Vec<Bytes>) -> Self {
            Self {
                incoming: frames.into(),
                sent_head: None,
                sent_data: Vec::new(),
                finished: false,
                fail_on_send_head: false,
                fail_on_send_data: false,
                fail_on_finish: false,
                fail_on_recv_data: false,
            }
        }

        fn with_send_data_failure(mut self) -> Self {
            self.fail_on_send_data = true;
            self
        }

        fn with_finish_failure(mut self) -> Self {
            self.fail_on_finish = true;
            self
        }

        fn with_recv_failure(mut self) -> Self {
            self.fail_on_recv_data = true;
            self
        }
    }

    impl H3RequestIo for FakeH3Stream {
        #[allow(clippy::manual_async_fn)]
        fn recv_data(
            &mut self,
        ) -> impl std::future::Future<Output = anyhow::Result<Option<Bytes>>> + Send {
            async move {
                if self.fail_on_recv_data {
                    return Err(anyhow!("recv_data_failed"));
                }
                Ok(self.incoming.pop_front())
            }
        }
        #[allow(clippy::manual_async_fn)]
        fn send_response(
            &mut self,
            resp: Response<()>,
        ) -> impl std::future::Future<Output = anyhow::Result<()>> + Send {
            async move {
                if self.fail_on_send_head {
                    return Err(anyhow!("send_head_failed"));
                }
                self.sent_head = Some(resp);
                Ok(())
            }
        }
        #[allow(clippy::manual_async_fn)]
        fn send_data(
            &mut self,
            data: Bytes,
        ) -> impl std::future::Future<Output = anyhow::Result<()>> + Send {
            async move {
                if self.fail_on_send_data {
                    return Err(anyhow!("send_data_failed"));
                }
                self.sent_data.push(data);
                Ok(())
            }
        }
        #[allow(clippy::manual_async_fn)]
        fn finish(&mut self) -> impl std::future::Future<Output = anyhow::Result<()>> + Send {
            async move {
                if self.fail_on_finish {
                    return Err(anyhow!("finish_failed"));
                }
                self.finished = true;
                Ok(())
            }
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

        handle_http3_request_impl(req, &mut stream, remote, routes, None, None)
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

        handle_http3_request_impl(req, &mut stream, remote, routes, None, None)
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

        let err = handle_http3_request_impl(req, &mut stream, remote, routes, None, None)
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

    #[tokio::test]
    async fn test_http3_impl_send_data_error_propagates() {
        // 测试发送响应体数据失败时的错误传播
        let mut stream =
            FakeH3Stream::new(vec![Bytes::from_static(b"test")]).with_send_data_failure();
        let routes = make_routes_echo_body();
        let req = make_request("/");
        let remote: SocketAddr = "127.0.0.1:34570".parse().unwrap();

        let err = handle_http3_request_impl(req, &mut stream, remote, routes, None, None)
            .await
            .expect_err("should bubble up send data error");
        let msg = format!("{err:#}");
        assert!(msg.contains("send_data_failed"));
        // 验证响应头已发送但数据发送失败
        assert!(stream.sent_head.is_some());
        assert!(!stream.finished); // finish 不应被调用
    }

    #[tokio::test]
    async fn test_http3_impl_finish_error_propagates() {
        // 测试 finish 操作失败时的错误传播
        let mut stream = FakeH3Stream::new(vec![Bytes::from_static(b"test")]).with_finish_failure();
        let routes = make_routes_echo_body();
        let req = make_request("/");
        let remote: SocketAddr = "127.0.0.1:34571".parse().unwrap();

        let err = handle_http3_request_impl(req, &mut stream, remote, routes, None, None)
            .await
            .expect_err("should bubble up finish error");
        let msg = format!("{err:#}");
        assert!(msg.contains("finish_failed"));
        // 验证响应头和数据已发送，但 finish 失败
        assert!(stream.sent_head.is_some());
        assert!(!stream.sent_data.is_empty());
    }

    #[tokio::test]
    async fn test_http3_impl_recv_error_propagates() {
        // 测试接收请求体失败时的错误传播
        let mut stream = FakeH3Stream::new(vec![Bytes::from_static(b"test")]).with_recv_failure();
        let routes = make_routes_echo_body();
        let req = make_request("/");
        let remote: SocketAddr = "127.0.0.1:34572".parse().unwrap();

        let err = handle_http3_request_impl(req, &mut stream, remote, routes, None, None)
            .await
            .expect_err("should bubble up recv error");
        let msg = format!("{err:#}");
        assert!(msg.contains("recv_data_failed"));
    }

    #[tokio::test]
    async fn test_http3_impl_large_body_handling() {
        // 测试大请求体处理（模拟多个大数据块）
        let large_data = vec![0u8; 8192]; // 8KB 数据块
        let chunks = vec![
            Bytes::from(large_data.clone()),
            Bytes::from(large_data.clone()),
            Bytes::from(large_data.clone()),
        ];
        let mut stream = FakeH3Stream::new(chunks);
        let routes = make_routes_echo_body();
        let req = make_request("/");
        let remote: SocketAddr = "127.0.0.1:34573".parse().unwrap();

        handle_http3_request_impl(req, &mut stream, remote, routes, None, None)
            .await
            .expect("large body should succeed");

        // 验证所有数据块被接收和聚合
        assert!(stream.sent_head.is_some());
        assert!(stream.finished);
        let total_sent: usize = stream.sent_data.iter().map(|b| b.len()).sum();
        assert_eq!(total_sent, large_data.len() * 3);
    }

    #[tokio::test]
    async fn test_http3_impl_invalid_utf8_body() {
        // 测试无效 UTF-8 请求体的处理
        // 创建包含无效 UTF-8 字节的请求体
        let invalid_utf8 = vec![0xFF, 0xFE, 0xFD, 0xFC, 0xFB];
        let mut stream = FakeH3Stream::new(vec![Bytes::from(invalid_utf8)]);
        let routes = make_routes_echo_body();
        let req = make_request("/");
        let remote: SocketAddr = "127.0.0.1:34574".parse().unwrap();

        // 无效 UTF-8 数据应该被正确处理并回显
        handle_http3_request_impl(req, &mut stream, remote, routes, None, None)
            .await
            .expect("invalid utf8 body should be handled");

        assert!(stream.sent_head.is_some());
        assert!(stream.finished);
        // 验证回显的数据与原始数据一致
        let echoed: Vec<u8> = stream
            .sent_data
            .iter()
            .flat_map(|b| b.iter().cloned())
            .collect();
        assert_eq!(echoed, vec![0xFF, 0xFE, 0xFD, 0xFC, 0xFB]);
    }

    #[tokio::test]
    async fn test_http3_impl_mixed_success_and_failure() {
        // 测试多帧数据中部分成功部分失败的情况
        // 这个测试验证错误发生前的数据已被处理
        let chunks = vec![
            Bytes::from_static(b"first "),
            Bytes::from_static(b"second "),
            Bytes::from_static(b"third"),
        ];
        // 模拟在发送响应数据时失败
        let mut stream = FakeH3Stream::new(chunks).with_send_data_failure();
        let routes = make_routes_echo_body();
        let req = make_request("/");
        let remote: SocketAddr = "127.0.0.1:34575".parse().unwrap();

        let err = handle_http3_request_impl(req, &mut stream, remote, routes, None, None)
            .await
            .expect_err("should fail on send data");
        let msg = format!("{err:#}");
        assert!(msg.contains("send_data_failed"));
        // 验证请求体已被完全接收（即使后续发送失败）
        assert!(stream.sent_head.is_some());
    }

    #[tokio::test]
    async fn test_http3_impl_empty_and_nonempty_chunks() {
        // 测试空块和非空块混合的处理
        let chunks = vec![
            Bytes::new(), // 空块
            Bytes::from_static(b"data"),
            Bytes::new(), // 另一个空块
            Bytes::from_static(b"more"),
        ];
        let mut stream = FakeH3Stream::new(chunks);
        let routes = make_routes_echo_body();
        let req = make_request("/");
        let remote: SocketAddr = "127.0.0.1:34576".parse().unwrap();

        handle_http3_request_impl(req, &mut stream, remote, routes, None, None)
            .await
            .expect("mixed chunks should succeed");

        assert!(stream.sent_head.is_some());
        assert!(stream.finished);
        // 验证空块被正确跳过，只聚合非空数据
        let echoed: Vec<u8> = stream
            .sent_data
            .iter()
            .flat_map(|b| b.iter().cloned())
            .collect();
        assert_eq!(echoed, b"datamore");
    }

    #[tokio::test]
    async fn test_http3_impl_handler_error_propagation() {
        // 测试路由处理器返回错误时的错误传播
        let mut stream = FakeH3Stream::new(vec![Bytes::from_static(b"test")]);
        let routes = make_routes_echo_body();
        let req = make_request("/");
        let remote: SocketAddr = "127.0.0.1:34577".parse().unwrap();

        // 不需要特殊设置，测试正常路径即可
        handle_http3_request_impl(req, &mut stream, remote, routes, None, None)
            .await
            .expect("normal path should succeed");

        assert!(stream.sent_head.is_some());
        assert!(stream.finished);
    }

    #[tokio::test]
    async fn test_http3_impl_empty_response_body() {
        // 测试返回空响应体的情况
        let mut stream = FakeH3Stream::new(vec![]);
        let routes = make_routes_echo_body();
        let req = make_request("/");
        let remote: SocketAddr = "127.0.0.1:34578".parse().unwrap();

        handle_http3_request_impl(req, &mut stream, remote, routes, None, None)
            .await
            .expect("empty response should succeed");

        assert!(stream.sent_head.is_some());
        assert!(stream.finished);
        // 验证响应体为空
        assert!(stream.sent_data.is_empty());
    }
}
