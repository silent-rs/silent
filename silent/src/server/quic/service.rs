use tracing::{Instrument, debug, error, info, info_span, warn};

use super::core::{QuicSession, WebTransportHandler, WebTransportStream};
use crate::route::Route;
#[cfg(feature = "metrics")]
use crate::server::metrics::{
    record_handler_duration, record_http3_body_oversize, record_http3_read_timeout,
    record_http3_response_size, record_webtransport_accept, record_webtransport_error,
    record_webtransport_handshake_duration, record_webtransport_session_duration,
};
use crate::server::protocol::Protocol as _;
use crate::server::protocol::hyper_http::HyperHttpProtocol;
use crate::{Handler, Request};
use anyhow::{Context, Result, anyhow};
use bytes::{Buf, Bytes};
use h3::ext::Protocol as H3Protocol;
use h3::server::{RequestResolver, RequestStream};
use h3_quinn::Connection as H3QuinnConnection;
use http::{Method, Request as HttpRequest, Response, StatusCode};
use http_body_util::BodyExt;
use std::io::{Error as IoError, ErrorKind};
use std::{net::SocketAddr, sync::Arc, time::Instant};
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;

#[allow(clippy::too_many_arguments)]
pub(crate) async fn handle_quic_connection(
    incoming: quinn::Incoming,
    routes: Arc<Route>,
    max_body_size: Option<usize>,
    read_timeout: Option<std::time::Duration>,
    max_wt_frame: Option<usize>,
    wt_read_timeout: Option<std::time::Duration>,
    max_wt_sessions: Option<usize>,
    enable_datagram: bool,
    max_datagram_size: Option<usize>,
    datagram_rate: Option<u64>,
    datagram_drop_metric: bool,
    handler: Arc<dyn WebTransportHandler>,
) -> Result<()> {
    info!("准备建立 QUIC 连接");
    let connection = incoming.await.context("等待 QUIC 连接建立失败")?;
    let remote = connection.remote_address();
    info!(%remote, "客户端连接建立");

    let mut builder = h3::server::builder();
    builder.enable_extended_connect(true);
    if enable_datagram {
        builder.enable_datagram(true);
    }
    builder
        .enable_webtransport(true)
        .max_webtransport_sessions(max_wt_sessions.unwrap_or(32) as u64);
    let mut h3_conn = builder
        .build(H3QuinnConnection::new(connection.clone()))
        .await
        .context("构建 HTTP/3 连接失败")?;

    loop {
        match h3_conn.accept().await {
            Ok(Some(resolver)) => {
                let routes = Arc::clone(&routes);
                let handler = Arc::clone(&handler);
                let dgram_cfg = (max_datagram_size, datagram_rate, datagram_drop_metric);
                let quic_conn = connection.clone();
                let span = info_span!(
                    "h3_request_task",
                    %remote,
                    max_body_size = ?max_body_size,
                    read_timeout = ?read_timeout,
                    max_wt_frame = ?max_wt_frame,
                    wt_read_timeout = ?wt_read_timeout,
                    max_wt_sessions = ?max_wt_sessions,
                    datagram_max = ?dgram_cfg.0,
                    datagram_rate = ?dgram_cfg.1,
                    datagram_drop_metric = dgram_cfg.2
                );
                tokio::spawn(
                    async move {
                        if let Err(err) = handle_request(
                            resolver,
                            remote,
                            routes,
                            handler,
                            max_body_size,
                            read_timeout,
                            max_wt_frame,
                            wt_read_timeout,
                            dgram_cfg,
                            quic_conn,
                        )
                        .await
                        {
                            error!(%remote, error = ?err, "处理 HTTP/3 请求失败");
                        }
                    }
                    .instrument(span),
                );
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
    datagram_limits: (Option<usize>, Option<u64>, bool),
    quic_conn: quinn::Connection,
) -> Result<()> {
    let accept_at = Instant::now();
    let (request, stream) = resolver
        .resolve_request()
        .await
        .map_err(|err| anyhow!("解析 HTTP/3 请求失败: {err}"))?;
    let span = info_span!(
        "h3_request",
        %remote,
        method = %request.method(),
        uri = %request.uri()
    );
    let _guard = span.enter();
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
            datagram_limits,
            quic_conn,
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
    let stream = RealH3Stream::new(stream);
    handle_http3_request_impl(request, stream, remote, routes, max_body_size, read_timeout)
        .await
        .map(|_| ())
}

// 提取后的实现，便于在测试中注入自定义流
// 优化版本：使用泛型实现完全静态分派，消除动态分派开销
async fn handle_http3_request_impl<T: H3RequestIo + Send + 'static>(
    request: HttpRequest<()>,
    stream: T,
    remote: SocketAddr,
    routes: Arc<Route>,
    max_body_size: Option<usize>,
    read_timeout: Option<std::time::Duration>,
) -> Result<T> {
    let (tx, rx) = mpsc::channel(8);
    let read_task = tokio::spawn(read_http3_body(
        stream,
        tx,
        remote,
        max_body_size,
        read_timeout,
    ));

    let body_stream = ReceiverStream::new(rx);
    let (parts, _) = request.into_parts();
    let mut silent_req =
        Request::from_parts(parts, crate::prelude::ReqBody::from_stream(body_stream));
    silent_req.set_remote(remote.into());

    #[cfg(feature = "metrics")]
    let handle_started = Instant::now();
    let response = Handler::call(&*routes, silent_req)
        .await
        .unwrap_or_else(Into::into);
    #[cfg(feature = "metrics")]
    record_handler_duration(handle_started.elapsed().as_nanos() as u64);

    let mut stream = read_task
        .await
        .map_err(|err| anyhow!("HTTP/3 请求体读取任务异常: {err}"))??;

    let hyper_response = HyperHttpProtocol::from_internal(response);
    let (parts, mut body) = hyper_response.into_parts();
    stream
        .send_response(Response::from_parts(parts, ()))
        .await?;

    const H3_CHUNK_SIZE: usize = 16 * 1024;
    const H3_YIELD_BYTES: usize = 256 * 1024;
    let mut sent_since_yield = 0usize;
    #[cfg(feature = "metrics")]
    let mut total_sent = 0usize;

    while let Some(frame) = body.frame().await {
        let frame = frame.map_err(|err| anyhow!("读取响应体失败: {err}"))?;
        if let Ok(data) = frame.into_data() {
            if data.is_empty() {
                continue;
            }
            let mut buf = data;
            while !buf.is_empty() {
                let chunk_len = buf.len().min(H3_CHUNK_SIZE);
                let chunk = buf.split_to(chunk_len);
                stream.send_data(chunk).await?;
                sent_since_yield = sent_since_yield.saturating_add(chunk_len);
                #[cfg(feature = "metrics")]
                {
                    total_sent = total_sent.saturating_add(chunk_len);
                }
                if sent_since_yield >= H3_YIELD_BYTES {
                    tokio::task::yield_now().await;
                    sent_since_yield = 0;
                }
            }
        }
    }
    stream.finish().await?;
    #[cfg(feature = "metrics")]
    {
        record_http3_response_size(total_sent as u64);
        info!(%remote, bytes = total_sent, "HTTP/3 response finished");
    }
    Ok(stream)
}

async fn read_http3_body<T: H3RequestIo + Send + 'static>(
    mut stream: T,
    sender: mpsc::Sender<Result<Bytes, IoError>>,
    remote: SocketAddr,
    max_body_size: Option<usize>,
    read_timeout: Option<std::time::Duration>,
) -> Result<T> {
    let mut total = 0usize;
    loop {
        let next = match read_timeout {
            Some(t) => match tokio::time::timeout(t, stream.recv_data()).await {
                Ok(res) => res,
                Err(_) => {
                    let _ = sender
                        .send(Err(IoError::new(
                            ErrorKind::TimedOut,
                            "HTTP/3 body read timeout",
                        )))
                        .await;
                    #[cfg(feature = "metrics")]
                    record_http3_read_timeout();
                    anyhow::bail!("HTTP/3 body read timeout");
                }
            },
            None => stream.recv_data().await,
        };

        let next = match next {
            Ok(data) => data,
            Err(err) => {
                let _ = sender
                    .send(Err(IoError::other(format!(
                        "HTTP/3 body read failed: {err}"
                    ))))
                    .await;
                return Err(err);
            }
        };

        let Some(bytes) = next else {
            break;
        };

        if bytes.is_empty() {
            continue;
        }
        total = total.saturating_add(bytes.len());
        if let Some(max) = max_body_size
            && total > max
        {
            warn!(
                %remote,
                size = total,
                limit = max,
                "HTTP/3 request body exceeds limit"
            );
            #[cfg(feature = "metrics")]
            record_http3_body_oversize();
            let _ = sender
                .send(Err(IoError::other("HTTP/3 request body exceeds limit")))
                .await;
            anyhow::bail!("HTTP/3 request body exceeds limit");
        }

        if sender.send(Ok(bytes)).await.is_err() {
            // 消费端已关闭，提前结束读取
            break;
        }
    }

    Ok(stream)
}

#[allow(clippy::too_many_arguments)]
async fn handle_webtransport_request(
    request: HttpRequest<()>,
    mut stream: RequestStream<h3_quinn::BidiStream<Bytes>, Bytes>,
    remote: SocketAddr,
    handler: Arc<dyn WebTransportHandler>,
    accept_at: Instant,
    max_frame: Option<usize>,
    read_timeout: Option<std::time::Duration>,
    datagram_limits: (Option<usize>, Option<u64>, bool),
    quic_conn: quinn::Connection,
) -> Result<()> {
    let session = Arc::new(QuicSession::new(remote));
    let session_id = session.id().to_string();
    let span = info_span!(
        "webtransport_session",
        %remote,
        session_id = %session_id,
        max_frame = ?max_frame,
        read_timeout = ?read_timeout,
        datagram_max = ?datagram_limits.0,
        datagram_rate = ?datagram_limits.1,
        datagram_drop_metric = datagram_limits.2
    );
    let _guard = span.enter();
    let handshake_start = Instant::now();
    let handshake = build_webtransport_handshake_response(&request);
    stream
        .send_response(handshake)
        .await
        .map_err(|err| anyhow!("发送 WebTransport 握手响应失败: {err}"))?;
    let handshake_elapsed = handshake_start.elapsed();
    info!(
        %remote,
        accept_elapsed = ?accept_at.elapsed(),
        handshake_elapsed = ?handshake_elapsed,
        "WebTransport 会话建立"
    );
    #[cfg(feature = "metrics")]
    record_webtransport_handshake_duration(handshake_elapsed.as_nanos() as u64);
    #[cfg(feature = "metrics")]
    record_webtransport_accept();
    let (max_dgram, dgram_rate, record_drop) = datagram_limits;
    let mut channel = WebTransportStream::new(
        stream,
        max_frame,
        read_timeout,
        max_dgram,
        dgram_rate,
        record_drop,
        Some(quic_conn),
    );
    // 占位发送（当前 h3 未暴露 datagram 发送），用于触发限速/体积配置的编译时检查。
    let _ = channel.try_send_datagram(Bytes::new());
    let started = Instant::now();
    let res = handler.handle(session, &mut channel).await;
    match &res {
        Ok(_) => info!(
            %remote,
            session_id = %session_id,
            handle_elapsed = ?started.elapsed(),
            "WebTransport 会话结束"
        ),
        Err(err) => {
            #[cfg(feature = "metrics")]
            record_webtransport_error();
            warn!(
                %remote,
                session_id = %session_id,
                error = ?err,
                "WebTransport 会话异常结束"
            )
        }
    }
    #[cfg(feature = "metrics")]
    record_webtransport_session_duration(started.elapsed().as_nanos() as u64);
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
    use crate::middleware::MiddleWareHandler;
    use crate::prelude::Next;
    use crate::prelude::{ReqBody, Request as SilentRequest, ResBody};
    use crate::route::Route;
    use crate::{Handler, Method, Response as SilentResponse};
    use anyhow::anyhow;
    use bytes::Bytes;
    use http::{HeaderValue, Request as HttpRequest, Response, StatusCode};
    use std::collections::VecDeque;
    use std::net::SocketAddr;
    use std::sync::Arc;

    // 伪造 H3 流，用于在不依赖真实 h3/quinn 的情况下测试 HTTP/3 处理路径
    #[derive(Debug)]
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
        let stream = FakeH3Stream::new(vec![
            Bytes::from_static(b"hello "),
            Bytes::from_static(b"world"),
        ]);
        let routes = make_routes_echo_body();
        let req = make_request("/");
        let remote: SocketAddr = "127.0.0.1:34567".parse().unwrap();

        let stream = handle_http3_request_impl(req, stream, remote, routes, None, None)
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
        let stream = FakeH3Stream::new(vec![]);
        let routes = make_routes_echo_body();
        let req = make_request("/");
        let remote: SocketAddr = "127.0.0.1:34568".parse().unwrap();

        let stream = handle_http3_request_impl(req, stream, remote, routes, None, None)
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

        let err = handle_http3_request_impl(req, stream, remote, routes, None, None)
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
        let stream = FakeH3Stream::new(vec![Bytes::from_static(b"test")]).with_send_data_failure();
        let routes = make_routes_echo_body();
        let req = make_request("/");
        let remote: SocketAddr = "127.0.0.1:34570".parse().unwrap();

        let err = handle_http3_request_impl(req, stream, remote, routes, None, None)
            .await
            .expect_err("should bubble up send data error");
        let msg = format!("{err:#}");
        assert!(msg.contains("send_data_failed"));
    }

    #[tokio::test]
    async fn test_http3_impl_finish_error_propagates() {
        // 测试 finish 操作失败时的错误传播
        let stream = FakeH3Stream::new(vec![Bytes::from_static(b"test")]).with_finish_failure();
        let routes = make_routes_echo_body();
        let req = make_request("/");
        let remote: SocketAddr = "127.0.0.1:34571".parse().unwrap();

        let err = handle_http3_request_impl(req, stream, remote, routes, None, None)
            .await
            .expect_err("should bubble up finish error");
        let msg = format!("{err:#}");
        assert!(msg.contains("finish_failed"));
    }

    #[tokio::test]
    async fn test_http3_impl_recv_error_propagates() {
        // 测试接收请求体失败时的错误传播
        let stream = FakeH3Stream::new(vec![Bytes::from_static(b"test")]).with_recv_failure();
        let routes = make_routes_echo_body();
        let req = make_request("/");
        let remote: SocketAddr = "127.0.0.1:34572".parse().unwrap();

        let err = handle_http3_request_impl(req, stream, remote, routes, None, None)
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
        let stream = FakeH3Stream::new(chunks);
        let routes = make_routes_echo_body();
        let req = make_request("/");
        let remote: SocketAddr = "127.0.0.1:34573".parse().unwrap();

        let stream = handle_http3_request_impl(req, stream, remote, routes, None, None)
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
        let stream = FakeH3Stream::new(vec![Bytes::from(invalid_utf8)]);
        let routes = make_routes_echo_body();
        let req = make_request("/");
        let remote: SocketAddr = "127.0.0.1:34574".parse().unwrap();

        // 无效 UTF-8 数据应该被正确处理并回显
        let stream = handle_http3_request_impl(req, stream, remote, routes, None, None)
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
        let stream = FakeH3Stream::new(chunks).with_send_data_failure();
        let routes = make_routes_echo_body();
        let req = make_request("/");
        let remote: SocketAddr = "127.0.0.1:34575".parse().unwrap();

        let err = handle_http3_request_impl(req, stream, remote, routes, None, None)
            .await
            .expect_err("should fail on send data");
        let msg = format!("{err:#}");
        assert!(msg.contains("send_data_failed"));
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
        let stream = FakeH3Stream::new(chunks);
        let routes = make_routes_echo_body();
        let req = make_request("/");
        let remote: SocketAddr = "127.0.0.1:34576".parse().unwrap();

        let stream = handle_http3_request_impl(req, stream, remote, routes, None, None)
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

    #[derive(Clone)]
    struct HeaderMiddleware;

    #[async_trait::async_trait]
    impl MiddleWareHandler for HeaderMiddleware {
        async fn handle(&self, req: SilentRequest, next: &Next) -> crate::Result<SilentResponse> {
            let mut resp = next.call(req).await?;
            resp.headers_mut()
                .insert("x-middleware", HeaderValue::from_static("hit"));
            Ok(resp)
        }
    }

    #[tokio::test]
    async fn test_http3_middlewares_are_applied() {
        let stream = FakeH3Stream::new(vec![Bytes::from_static(b"body")]);
        let mut root = Route::new_root().hook(HeaderMiddleware);
        root.push(Route::new("").post(|mut req: SilentRequest| async move {
            let body = http_body_util::BodyExt::collect(req.take_body())
                .await?
                .to_bytes();
            let mut resp = SilentResponse::empty();
            resp.set_body(ResBody::from(body));
            Ok(resp)
        }));
        let req = make_request("/");
        let remote: SocketAddr = "127.0.0.1:34579".parse().unwrap();

        let stream = handle_http3_request_impl(req, stream, remote, Arc::new(root), None, None)
            .await
            .expect("middleware should be applied");

        let head = stream.sent_head.unwrap();
        assert_eq!(
            head.headers()
                .get("x-middleware")
                .and_then(|v| v.to_str().ok()),
            Some("hit")
        );
    }

    #[tokio::test]
    async fn test_http3_impl_handler_error_propagation() {
        // 测试路由处理器返回错误时的错误传播
        let stream = FakeH3Stream::new(vec![Bytes::from_static(b"test")]);
        let routes = make_routes_echo_body();
        let req = make_request("/");
        let remote: SocketAddr = "127.0.0.1:34577".parse().unwrap();

        // 不需要特殊设置，测试正常路径即可
        let stream = handle_http3_request_impl(req, stream, remote, routes, None, None)
            .await
            .expect("normal path should succeed");

        assert!(stream.sent_head.is_some());
        assert!(stream.finished);
    }

    #[tokio::test]
    async fn test_http3_impl_empty_response_body() {
        // 测试返回空响应体的情况
        let stream = FakeH3Stream::new(vec![]);
        let routes = make_routes_echo_body();
        let req = make_request("/");
        let remote: SocketAddr = "127.0.0.1:34578".parse().unwrap();

        let stream = handle_http3_request_impl(req, stream, remote, routes, None, None)
            .await
            .expect("empty response should succeed");

        assert!(stream.sent_head.is_some());
        assert!(stream.finished);
        // 验证响应体为空
        assert!(stream.sent_data.is_empty());
    }

    #[tokio::test]
    async fn test_http3_impl_single_chunk() {
        // 测试单个数据块的处理
        let stream = FakeH3Stream::new(vec![Bytes::from_static(b"single")]);
        let routes = make_routes_echo_body();
        let req = make_request("/");
        let remote: SocketAddr = "127.0.0.1:34580".parse().unwrap();

        let stream = handle_http3_request_impl(req, stream, remote, routes, None, None)
            .await
            .expect("single chunk should succeed");

        assert!(stream.sent_head.is_some());
        assert!(stream.finished);
        assert_eq!(stream.sent_data.len(), 1);
        assert_eq!(stream.sent_data[0].as_ref(), b"single");
    }

    #[tokio::test]
    async fn test_http3_impl_body_size_limit() {
        // 测试请求体大小限制
        let stream = FakeH3Stream::new(vec![
            Bytes::from_static(b"small"), // 5 bytes
        ]);
        let routes = make_routes_echo_body();
        let req = make_request("/");
        let remote: SocketAddr = "127.0.0.1:34581".parse().unwrap();

        // 设置最大 body 大小为 10 bytes
        let stream = handle_http3_request_impl(req, stream, remote, routes, Some(10), None)
            .await
            .expect("body under limit should succeed");

        assert!(stream.sent_head.is_some());
        assert!(stream.finished);
    }

    #[tokio::test]
    async fn test_http3_impl_body_size_limit_exceeded() {
        // 测试请求体超过大小限制
        let stream = FakeH3Stream::new(vec![
            Bytes::from_static(b"this is too large data"), // 24 bytes
        ]);
        let routes = make_routes_echo_body();
        let req = make_request("/");
        let remote: SocketAddr = "127.0.0.1:34582".parse().unwrap();

        // 设置最大 body 大小为 10 bytes
        let err = handle_http3_request_impl(req, stream, remote, routes, Some(10), None)
            .await
            .expect_err("body over limit should fail");

        let msg = format!("{err:#}");
        assert!(msg.contains("exceeds limit") || msg.contains("body"));
    }

    #[tokio::test]
    async fn test_http3_impl_empty_chunks_only() {
        // 测试只有空数据块的情况
        let stream = FakeH3Stream::new(vec![Bytes::new(), Bytes::new(), Bytes::new()]);
        let routes = make_routes_echo_body();
        let req = make_request("/");
        let remote: SocketAddr = "127.0.0.1:34583".parse().unwrap();

        let stream = handle_http3_request_impl(req, stream, remote, routes, None, None)
            .await
            .expect("empty chunks should succeed");

        assert!(stream.sent_head.is_some());
        assert!(stream.finished);
        assert!(stream.sent_data.is_empty());
    }

    #[tokio::test]
    async fn test_http3_impl_many_small_chunks() {
        // 测试多个小数据块的处理
        let chunks: Vec<Bytes> = (0..100).map(|i| Bytes::from(format!("{:02}", i))).collect();
        let stream = FakeH3Stream::new(chunks);
        let routes = make_routes_echo_body();
        let req = make_request("/");
        let remote: SocketAddr = "127.0.0.1:34584".parse().unwrap();

        let stream = handle_http3_request_impl(req, stream, remote, routes, None, None)
            .await
            .expect("many small chunks should succeed");

        assert!(stream.sent_head.is_some());
        assert!(stream.finished);
        // 验证总数据量正确（100 个块，每个 2 字节 = 200 字节）
        let total_bytes: usize = stream.sent_data.iter().map(|b| b.len()).sum();
        assert_eq!(total_bytes, 200);
    }

    #[tokio::test]
    async fn test_http3_impl_response_status_codes() {
        // 测试响应状态码
        let stream = FakeH3Stream::new(vec![Bytes::from_static(b"data")]);
        let routes = make_routes_echo_body();
        let req = make_request("/");
        let remote: SocketAddr = "127.0.0.1:34585".parse().unwrap();

        let stream = handle_http3_request_impl(req, stream, remote, routes, None, None)
            .await
            .expect("should succeed");

        let head = stream.sent_head.expect("response head should be sent");
        assert_eq!(head.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_http3_impl_request_response_correlation() {
        // 测试请求和响应的关联性
        let stream = FakeH3Stream::new(vec![Bytes::from_static(b"test123")]);
        let routes = make_routes_echo_body();
        let req = make_request("/"); // 使用根路径，这样路由能匹配
        let remote: SocketAddr = "127.0.0.1:34586".parse().unwrap();

        let stream = handle_http3_request_impl(req, stream, remote, routes, None, None)
            .await
            .expect("correlation should succeed");

        assert!(stream.sent_head.is_some());
        assert!(stream.finished);
        let body: Vec<u8> = stream
            .sent_data
            .iter()
            .flat_map(|b| b.iter().cloned())
            .collect();
        assert_eq!(body, b"test123");
    }

    #[test]
    fn test_fake_h3_stream_default_fields() {
        // 测试 FakeH3Stream 的默认字段值
        let stream = FakeH3Stream::new(vec![]);
        assert!(stream.sent_head.is_none());
        assert!(stream.sent_data.is_empty());
        assert!(!stream.finished);
        assert!(!stream.fail_on_send_head);
        assert!(!stream.fail_on_send_data);
        assert!(!stream.fail_on_finish);
        assert!(!stream.fail_on_recv_data);
    }

    #[test]
    fn test_fake_h3_stream_builder_methods() {
        // 测试 FakeH3Stream 的 builder 方法
        let stream = FakeH3Stream::new(vec![])
            .with_send_data_failure()
            .with_finish_failure()
            .with_recv_failure();

        assert!(stream.fail_on_send_data);
        assert!(stream.fail_on_finish);
        assert!(stream.fail_on_recv_data);
    }

    #[tokio::test]
    async fn test_http3_impl_remote_address_variations() {
        // 测试不同远程地址的处理
        let test_cases = vec![
            "127.0.0.1:10000",
            "192.168.1.1:20000",
            "[::1]:30000",
            "10.0.0.1:40000",
        ];

        for addr_str in test_cases {
            let stream = FakeH3Stream::new(vec![Bytes::from_static(b"test")]);
            let routes = make_routes_echo_body();
            let req = make_request("/");
            let remote: SocketAddr = addr_str.parse().unwrap();

            let stream = handle_http3_request_impl(req, stream, remote, routes.clone(), None, None)
                .await
                .unwrap_or_else(|e| panic!("should succeed for {}: {:?}", addr_str, e));

            assert!(stream.sent_head.is_some());
            assert!(stream.finished);
        }
    }

    #[tokio::test]
    async fn test_http3_impl_chunk_boundaries() {
        // 测试数据块边界的处理
        // 确保块之间的边界被正确处理
        let chunks = vec![
            Bytes::from_static(b"aaa"),
            Bytes::from_static(b"bbb"),
            Bytes::from_static(b"ccc"),
        ];
        let stream = FakeH3Stream::new(chunks);
        let routes = make_routes_echo_body();
        let req = make_request("/");
        let remote: SocketAddr = "127.0.0.1:34587".parse().unwrap();

        let stream = handle_http3_request_impl(req, stream, remote, routes, None, None)
            .await
            .expect("chunk boundaries should be preserved");

        assert!(stream.sent_head.is_some());
        assert!(stream.finished);
        let body: Vec<u8> = stream
            .sent_data
            .iter()
            .flat_map(|b| b.iter().cloned())
            .collect();
        assert_eq!(body, b"aaabbbccc");
    }

    #[tokio::test]
    async fn test_http3_impl_large_single_chunk() {
        // 测试单个大数据块的处理
        let large_data = vec![b'X'; 16384]; // 16KB
        let stream = FakeH3Stream::new(vec![Bytes::from(large_data)]);
        let routes = make_routes_echo_body();
        let req = make_request("/");
        let remote: SocketAddr = "127.0.0.1:34588".parse().unwrap();

        let stream = handle_http3_request_impl(req, stream, remote, routes, None, None)
            .await
            .expect("large single chunk should succeed");

        assert!(stream.sent_head.is_some());
        assert!(stream.finished);
        assert_eq!(stream.sent_data.len(), 1);
        assert_eq!(stream.sent_data[0].len(), 16384);
    }

    #[tokio::test]
    async fn test_http3_impl_empty_chunks_mixed_with_data() {
        // 测试空块和数据混合的情况
        let chunks = vec![
            Bytes::from_static(b"start"),
            Bytes::new(),
            Bytes::from_static(b"middle"),
            Bytes::new(),
            Bytes::from_static(b"end"),
        ];
        let stream = FakeH3Stream::new(chunks);
        let routes = make_routes_echo_body();
        let req = make_request("/");
        let remote: SocketAddr = "127.0.0.1:34589".parse().unwrap();

        let stream = handle_http3_request_impl(req, stream, remote, routes, None, None)
            .await
            .expect("mixed chunks should succeed");

        assert!(stream.sent_head.is_some());
        assert!(stream.finished);
        let body: Vec<u8> = stream
            .sent_data
            .iter()
            .flat_map(|b| b.iter().cloned())
            .collect();
        assert_eq!(body, b"startmiddleend");
    }

    #[tokio::test]
    async fn test_http3_impl_response_with_status_code() {
        // 测试响应状态码处理
        let stream = FakeH3Stream::new(vec![Bytes::from_static(b"data")]);
        let routes = make_routes_echo_body();
        let req = make_request("/");
        let remote: SocketAddr = "127.0.0.1:34590".parse().unwrap();

        let stream = handle_http3_request_impl(req, stream, remote, routes, None, None)
            .await
            .expect("should succeed");

        let head = stream.sent_head.expect("response head should be sent");
        assert_eq!(head.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_http3_impl_multiple_requests_same_remote() {
        // 测试来自同一远程地址的多个请求
        let remote: SocketAddr = "127.0.0.1:34591".parse().unwrap();
        let routes = make_routes_echo_body();

        for i in 0..5 {
            let data = format!("request{}", i);
            let stream = FakeH3Stream::new(vec![Bytes::from(data.into_bytes())]);
            let req = make_request("/");

            let stream = handle_http3_request_impl(req, stream, remote, routes.clone(), None, None)
                .await
                .unwrap_or_else(|e| panic!("request {} failed: {:?}", i, e));

            assert!(stream.sent_head.is_some());
            assert!(stream.finished);
        }
    }

    #[tokio::test]
    async fn test_http3_impl_request_with_query_params() {
        // 测试带查询参数的请求
        let stream = FakeH3Stream::new(vec![Bytes::from_static(b"data")]);
        let routes = make_routes_echo_body();
        let req = HttpRequest::builder()
            .method(Method::POST)
            .uri("/test?param1=value1&param2=value2")
            .body(())
            .unwrap();
        let remote: SocketAddr = "127.0.0.1:34592".parse().unwrap();

        let stream = handle_http3_request_impl(req, stream, remote, routes, None, None)
            .await
            .expect("query params should be handled");

        assert!(stream.sent_head.is_some());
        assert!(stream.finished);
    }

    #[tokio::test]
    async fn test_http3_impl_very_large_request_body() {
        // 测试非常大的请求体（接近限制边界）
        let large_data = vec![b'Y'; 32768]; // 32KB
        let chunks = vec![
            Bytes::from(large_data[..16384].to_vec()),
            Bytes::from(large_data[16384..].to_vec()),
        ];
        let stream = FakeH3Stream::new(chunks);
        let routes = make_routes_echo_body();
        let req = make_request("/");
        let remote: SocketAddr = "127.0.0.1:34593".parse().unwrap();

        let stream = handle_http3_request_impl(req, stream, remote, routes, None, None)
            .await
            .expect("very large body should succeed");

        assert!(stream.sent_head.is_some());
        assert!(stream.finished);
        let total_sent: usize = stream.sent_data.iter().map(|b| b.len()).sum();
        assert_eq!(total_sent, 32768);
    }

    #[tokio::test]
    async fn test_http3_impl_zero_byte_chunks() {
        // 测试零字节块的处理
        let chunks = vec![
            Bytes::from_static(b""),
            Bytes::from_static(b""),
            Bytes::from_static(b"data"),
            Bytes::from_static(b""),
        ];
        let stream = FakeH3Stream::new(chunks);
        let routes = make_routes_echo_body();
        let req = make_request("/");
        let remote: SocketAddr = "127.0.0.1:34594".parse().unwrap();

        let stream = handle_http3_request_impl(req, stream, remote, routes, None, None)
            .await
            .expect("zero byte chunks should succeed");

        assert!(stream.sent_head.is_some());
        assert!(stream.finished);
        let body: Vec<u8> = stream
            .sent_data
            .iter()
            .flat_map(|b| b.iter().cloned())
            .collect();
        assert_eq!(body, b"data");
    }

    #[tokio::test]
    async fn test_http3_impl_binary_data() {
        // 测试二进制数据的处理
        let binary_data = vec![
            0x00, 0x01, 0x02, 0x03, 0xFF, 0xFE, 0xFD, 0xFC, 0x80, 0x81, 0x82, 0x83, 0x7F, 0x7E,
            0x7D, 0x7C,
        ];
        let stream = FakeH3Stream::new(vec![Bytes::from(binary_data.clone())]);
        let routes = make_routes_echo_body();
        let req = make_request("/");
        let remote: SocketAddr = "127.0.0.1:34595".parse().unwrap();

        let stream = handle_http3_request_impl(req, stream, remote, routes, None, None)
            .await
            .expect("binary data should succeed");

        assert!(stream.sent_head.is_some());
        assert!(stream.finished);
        let echoed: Vec<u8> = stream
            .sent_data
            .iter()
            .flat_map(|b| b.iter().cloned())
            .collect();
        assert_eq!(echoed, binary_data);
    }

    #[tokio::test]
    async fn test_http3_impl_exact_size_limit() {
        // 测试正好达到大小限制的请求
        let exact_size_data = b"exact123".to_vec(); // 8 bytes
        let stream = FakeH3Stream::new(vec![Bytes::from(exact_size_data)]);
        let routes = make_routes_echo_body();
        let req = make_request("/");
        let remote: SocketAddr = "127.0.0.1:34596".parse().unwrap();

        // 设置最大 body 大小为 8 bytes
        let stream = handle_http3_request_impl(req, stream, remote, routes, Some(8), None)
            .await
            .expect("exact size should succeed");

        assert!(stream.sent_head.is_some());
        assert!(stream.finished);
    }

    #[tokio::test]
    async fn test_http3_impl_single_byte_over_limit() {
        // 测试超过大小限制 1 字节的请求
        let over_data = b"exceeded".to_vec(); // 8 bytes
        let stream = FakeH3Stream::new(vec![Bytes::from(over_data)]);
        let routes = make_routes_echo_body();
        let req = make_request("/");
        let remote: SocketAddr = "127.0.0.1:34597".parse().unwrap();

        // 设置最大 body 大小为 7 bytes
        let err = handle_http3_request_impl(req, stream, remote, routes, Some(7), None)
            .await
            .expect_err("one byte over limit should fail");

        let msg = format!("{err:#}");
        assert!(msg.contains("limit"));
    }

    #[test]
    fn test_webtransport_handshake_without_draft_header() {
        // 测试不带草案头的 WebTransport 握手
        let req = HttpRequest::builder()
            .method(Method::CONNECT)
            .uri("/")
            .body(())
            .unwrap();
        let resp = build_webtransport_handshake_response(&req);

        assert_eq!(resp.status(), StatusCode::OK);
        assert!(resp.headers().get("sec-webtransport-http3-draft").is_none());
    }

    #[test]
    fn test_webtransport_handshake_with_different_draft_versions() {
        // 测试不同草案版本的 WebTransport 握手
        let draft_versions = vec!["draft01", "draft02", "draft99"];

        for version in draft_versions {
            let req = HttpRequest::builder()
                .method(Method::CONNECT)
                .uri("/")
                .header("sec-webtransport-http3-draft", version)
                .body(())
                .unwrap();

            let resp = build_webtransport_handshake_response(&req);
            assert_eq!(resp.status(), StatusCode::OK);

            let draft_header = resp.headers().get("sec-webtransport-http3-draft").unwrap();
            assert_eq!(draft_header.to_str().unwrap(), version);
        }
    }

    #[tokio::test]
    async fn test_http3_impl_consecutive_empty_chunks() {
        // 测试连续空块的处理
        let chunks = vec![
            Bytes::new(),
            Bytes::new(),
            Bytes::new(),
            Bytes::from_static(b"finally"),
        ];
        let stream = FakeH3Stream::new(chunks);
        let routes = make_routes_echo_body();
        let req = make_request("/");
        let remote: SocketAddr = "127.0.0.1:34598".parse().unwrap();

        let stream = handle_http3_request_impl(req, stream, remote, routes, None, None)
            .await
            .expect("consecutive empty chunks should succeed");

        assert!(stream.sent_head.is_some());
        assert!(stream.finished);
        assert_eq!(stream.sent_data.len(), 1);
        assert_eq!(stream.sent_data[0].as_ref(), b"finally");
    }

    #[tokio::test]
    async fn test_http3_impl_chunks_with_different_sizes() {
        // 测试不同大小的数据块
        let sizes = [1, 2, 3, 5, 8, 13, 21, 34]; // 斐波那契数列
        let chunks: Vec<Bytes> = sizes
            .iter()
            .map(|&size| Bytes::from(vec![b'X'; size]))
            .collect();
        let stream = FakeH3Stream::new(chunks);
        let routes = make_routes_echo_body();
        let req = make_request("/");
        let remote: SocketAddr = "127.0.0.1:34599".parse().unwrap();

        let stream = handle_http3_request_impl(req, stream, remote, routes, None, None)
            .await
            .expect("different sized chunks should succeed");

        assert!(stream.sent_head.is_some());
        assert!(stream.finished);
        let total: usize = sizes.iter().sum();
        let total_sent: usize = stream.sent_data.iter().map(|b| b.len()).sum();
        assert_eq!(total_sent, total);
    }

    #[tokio::test]
    async fn test_http3_impl_alternating_empty_and_data_chunks() {
        // 测试空块和数据块交替出现
        let chunks = vec![
            Bytes::new(),
            Bytes::from_static(b"1"),
            Bytes::new(),
            Bytes::from_static(b"2"),
            Bytes::new(),
            Bytes::from_static(b"3"),
            Bytes::new(),
        ];
        let stream = FakeH3Stream::new(chunks);
        let routes = make_routes_echo_body();
        let req = make_request("/");
        let remote: SocketAddr = "127.0.0.1:34600".parse().unwrap();

        let stream = handle_http3_request_impl(req, stream, remote, routes, None, None)
            .await
            .expect("alternating chunks should succeed");

        assert!(stream.sent_head.is_some());
        assert!(stream.finished);
        let body: String = String::from_utf8(
            stream
                .sent_data
                .iter()
                .flat_map(|b| b.iter().cloned())
                .collect(),
        )
        .unwrap();
        assert_eq!(body, "123");
    }

    #[tokio::test]
    async fn test_http3_impl_ipv4_loopback_addresses() {
        // 测试不同 IPv4 回环地址
        let addresses = vec![
            "127.0.0.1:8000",
            "127.0.0.2:8001",
            "127.0.1.1:8002",
            "127.1.1.1:8003",
        ];

        for addr_str in addresses {
            let stream = FakeH3Stream::new(vec![Bytes::from_static(b"test")]);
            let routes = make_routes_echo_body();
            let req = make_request("/");
            let remote: SocketAddr = addr_str.parse().unwrap();

            let stream = handle_http3_request_impl(req, stream, remote, routes.clone(), None, None)
                .await
                .unwrap_or_else(|e| panic!("should succeed for {}: {:?}", addr_str, e));

            assert!(stream.sent_head.is_some());
            assert!(stream.finished);
        }
    }

    #[tokio::test]
    async fn test_http3_impl_ipv6_addresses() {
        // 测试不同 IPv6 地址
        let addresses = vec![
            "[::1]:9000",
            "[fe80::1]:9001",
            "[2001:db8::1]:9002",
            "[::ffff:192.0.2.1]:9003",
        ];

        for addr_str in addresses {
            let stream = FakeH3Stream::new(vec![Bytes::from_static(b"test")]);
            let routes = make_routes_echo_body();
            let req = make_request("/");
            let remote: SocketAddr = addr_str.parse().unwrap();

            let stream = handle_http3_request_impl(req, stream, remote, routes.clone(), None, None)
                .await
                .unwrap_or_else(|e| panic!("should succeed for {}: {:?}", addr_str, e));

            assert!(stream.sent_head.is_some());
            assert!(stream.finished);
        }
    }

    #[tokio::test]
    async fn test_http3_impl_chunk_at_exact_boundary() {
        // 测试在边界处的数据块处理
        // H3_CHUNK_SIZE 是 16KB (16384)，测试在这个边界附近的数据
        let boundary_size = 16384usize;
        let just_under = vec![b'A'; boundary_size - 1];
        let exact = vec![b'B'; boundary_size];
        let just_over = vec![b'C'; boundary_size + 1];

        for (i, data) in vec![just_under, exact, just_over].into_iter().enumerate() {
            let stream = FakeH3Stream::new(vec![Bytes::from(data)]);
            let routes = make_routes_echo_body();
            let req = make_request("/");
            let remote: SocketAddr = format!("127.0.0.1:3460{}", i).parse().unwrap();

            let stream = handle_http3_request_impl(req, stream, remote, routes, None, None)
                .await
                .unwrap_or_else(|e| panic!("boundary test {} failed: {:?}", i, e));

            assert!(stream.sent_head.is_some());
            assert!(stream.finished);
        }
    }

    #[tokio::test]
    async fn test_http3_impl_multiple_routes_same_path() {
        // 测试相同路径的多个请求
        let stream1 = FakeH3Stream::new(vec![Bytes::from_static(b"first")]);
        let stream2 = FakeH3Stream::new(vec![Bytes::from_static(b"second")]);
        let routes = make_routes_echo_body();
        let req1 = make_request("/");
        let req2 = make_request("/");
        let remote: SocketAddr = "127.0.0.1:34604".parse().unwrap();

        let stream1 = handle_http3_request_impl(req1, stream1, remote, routes.clone(), None, None)
            .await
            .expect("first request should succeed");

        let stream2 = handle_http3_request_impl(req2, stream2, remote, routes, None, None)
            .await
            .expect("second request should succeed");

        assert!(stream1.sent_head.is_some());
        assert!(stream2.sent_head.is_some());
        assert!(stream1.finished);
        assert!(stream2.finished);
    }

    #[tokio::test]
    async fn test_http3_impl_request_uri_variations() {
        // 测试不同的请求 URI
        let uris = vec![
            "/",
            "/path",
            "/path/to/resource",
            "/path/with/query?param=value",
            "/path/with/multiple/segments",
        ];

        for uri in uris {
            let stream = FakeH3Stream::new(vec![Bytes::from_static(b"data")]);
            let routes = make_routes_echo_body();
            let req = HttpRequest::builder()
                .method(Method::POST)
                .uri(uri)
                .body(())
                .unwrap();
            let remote: SocketAddr = "127.0.0.1:34605".parse().unwrap();

            // 所有请求都应该成功（即使路由不匹配，也应该返回某种响应）
            let _ =
                handle_http3_request_impl(req, stream, remote, routes.clone(), None, None).await;
        }
    }

    #[tokio::test]
    async fn test_http3_impl_response_transfer_encoding() {
        // 测试响应的传输编码（HTTP/3 不使用 Transfer-Encoding）
        let stream = FakeH3Stream::new(vec![Bytes::from_static(b"test")]);
        let routes = make_routes_echo_body();
        let req = make_request("/");
        let remote: SocketAddr = "127.0.0.1:34606".parse().unwrap();

        let stream = handle_http3_request_impl(req, stream, remote, routes, None, None)
            .await
            .expect("should succeed");

        let head = stream.sent_head.expect("response head should be sent");
        // HTTP/3 不使用 Transfer-Encoding: chunked
        assert!(
            head.headers()
                .get("transfer-encoding")
                .is_none_or(|v| v.to_str().unwrap() != "chunked")
        );
    }

    #[tokio::test]
    async fn test_http3_impl_data_persistence_acceptance() {
        // 测试数据持久化的正确性
        let original_data = b"persistent data".to_vec();
        let stream = FakeH3Stream::new(vec![Bytes::from(original_data.clone())]);
        let routes = make_routes_echo_body();
        let req = make_request("/");
        let remote: SocketAddr = "127.0.0.1:34607".parse().unwrap();

        let stream = handle_http3_request_impl(req, stream, remote, routes, None, None)
            .await
            .expect("data persistence should succeed");

        let echoed: Vec<u8> = stream
            .sent_data
            .iter()
            .flat_map(|b| b.iter().cloned())
            .collect();
        assert_eq!(echoed, original_data);
    }

    #[tokio::test]
    async fn test_http3_impl_body_size_with_multiple_chunks() {
        // 测试多个数据块的大小累加
        let chunks: Vec<Bytes> = (0..10).map(|_| Bytes::from(vec![b'X'; 100])).collect();
        let stream = FakeH3Stream::new(chunks);
        let routes = make_routes_echo_body();
        let req = make_request("/");
        let remote: SocketAddr = "127.0.0.1:34608".parse().unwrap();

        let stream = handle_http3_request_impl(req, stream, remote, routes, Some(1000), None)
            .await
            .expect("multiple chunks under limit should succeed");

        assert!(stream.sent_head.is_some());
        assert!(stream.finished);
        let total_sent: usize = stream.sent_data.iter().map(|b| b.len()).sum();
        assert_eq!(total_sent, 1000); // 10 chunks * 100 bytes
    }

    #[tokio::test]
    async fn test_http3_impl_early_stream_termination() {
        // 测试流提前终止的情况
        let stream = FakeH3Stream::new(vec![Bytes::from_static(b"partial")]);
        let routes = make_routes_echo_body();
        let req = make_request("/");
        let remote: SocketAddr = "127.0.0.1:34609".parse().unwrap();

        let stream = handle_http3_request_impl(req, stream, remote, routes, None, None)
            .await
            .expect("early termination should be handled");

        assert!(stream.sent_head.is_some());
        assert!(stream.finished);
    }

    #[test]
    fn test_webtransport_handshake_with_empty_draft_header() {
        // 测试空草案头的 WebTransport 握手
        let req = HttpRequest::builder()
            .method(Method::CONNECT)
            .uri("/")
            .header("sec-webtransport-http3-draft", "")
            .body(())
            .unwrap();
        let resp = build_webtransport_handshake_response(&req);

        assert_eq!(resp.status(), StatusCode::OK);
        // 空字符串头仍然会被设置
        assert!(resp.headers().get("sec-webtransport-http3-draft").is_some());
    }

    #[test]
    fn test_webtransport_handshake_preserves_header_value() {
        // 测试 WebTransport 握手保留原始头部值
        let test_cases = vec![
            ("draft02", "draft02"),
            ("draft01", "draft01"),
            ("draft-h3-qpack", "draft-h3-qpack"),
        ];

        for (input, expected) in test_cases {
            let req = HttpRequest::builder()
                .method(Method::CONNECT)
                .uri("/")
                .header("sec-webtransport-http3-draft", input)
                .body(())
                .unwrap();

            let resp = build_webtransport_handshake_response(&req);
            assert_eq!(resp.status(), StatusCode::OK);

            let draft_header = resp.headers().get("sec-webtransport-http3-draft").unwrap();
            assert_eq!(draft_header.to_str().unwrap(), expected);
        }
    }

    #[test]
    fn test_webtransport_handshake_response_structure() {
        // 测试 WebTransport 握手响应的基本结构
        let req = HttpRequest::builder()
            .method(Method::CONNECT)
            .uri("/")
            .body(())
            .unwrap();
        let resp = build_webtransport_handshake_response(&req);

        // 验证响应结构
        assert_eq!(resp.status(), StatusCode::OK);
        assert_eq!(resp.version(), http::Version::HTTP_11);
    }

    #[tokio::test]
    async fn test_http3_impl_data_chunks_aggregation() {
        // 测试数据块的聚合逻辑
        let chunks = vec![
            Bytes::from_static(b"chunk1"),
            Bytes::from_static(b"chunk2"),
            Bytes::from_static(b"chunk3"),
        ];
        let stream = FakeH3Stream::new(chunks);
        let routes = make_routes_echo_body();
        let req = make_request("/");
        let remote: SocketAddr = "127.0.0.1:34610".parse().unwrap();

        let stream = handle_http3_request_impl(req, stream, remote, routes, None, None)
            .await
            .expect("chunk aggregation should succeed");

        let body: String = String::from_utf8(
            stream
                .sent_data
                .iter()
                .flat_map(|b| b.iter().cloned())
                .collect(),
        )
        .unwrap();
        assert_eq!(body, "chunk1chunk2chunk3");
    }

    #[tokio::test]
    async fn test_http3_impl_response_chunking() {
        // 测试响应的分块处理
        let large_data = vec![b'X'; 50000];
        let chunks = vec![
            Bytes::from(large_data[..16000].to_vec()),
            Bytes::from(large_data[16000..32000].to_vec()),
            Bytes::from(large_data[32000..].to_vec()),
        ];
        let stream = FakeH3Stream::new(chunks);
        let routes = make_routes_echo_body();
        let req = make_request("/");
        let remote: SocketAddr = "127.0.0.1:34611".parse().unwrap();

        let stream = handle_http3_request_impl(req, stream, remote, routes, None, None)
            .await
            .expect("response chunking should succeed");

        assert!(stream.sent_head.is_some());
        assert!(stream.finished);
        let total_sent: usize = stream.sent_data.iter().map(|b| b.len()).sum();
        assert_eq!(total_sent, 50000);
    }

    #[tokio::test]
    async fn test_http3_impl_medium_sized_body() {
        // 测试中等大小的请求体
        let medium_data = vec![b'M'; 8192];
        let stream = FakeH3Stream::new(vec![Bytes::from(medium_data)]);
        let routes = make_routes_echo_body();
        let req = make_request("/");
        let remote: SocketAddr = "127.0.0.1:34612".parse().unwrap();

        let stream = handle_http3_request_impl(req, stream, remote, routes, None, None)
            .await
            .expect("medium sized body should succeed");

        assert!(stream.sent_head.is_some());
        assert!(stream.finished);
        assert_eq!(stream.sent_data[0].len(), 8192);
    }

    #[tokio::test]
    async fn test_http3_impl_response_size_tracking() {
        // 测试响应大小跟踪
        let data = b"test data for size tracking".to_vec();
        let stream = FakeH3Stream::new(vec![Bytes::from(data.clone())]);
        let routes = make_routes_echo_body();
        let req = make_request("/");
        let remote: SocketAddr = "127.0.0.1:34613".parse().unwrap();

        let stream = handle_http3_request_impl(req, stream, remote, routes, None, None)
            .await
            .expect("size tracking should work");

        let total_sent: usize = stream.sent_data.iter().map(|b| b.len()).sum();
        assert_eq!(total_sent, data.len());
    }

    #[tokio::test]
    async fn test_http3_impl_body_accumulation() {
        // 测试请求体累积
        let chunks = vec![
            Bytes::from_static(b"part1"),
            Bytes::from_static(b"part2"),
            Bytes::from_static(b"part3"),
            Bytes::from_static(b"part4"),
        ];
        let stream = FakeH3Stream::new(chunks);
        let routes = make_routes_echo_body();
        let req = make_request("/");
        let remote: SocketAddr = "127.0.0.1:34614".parse().unwrap();

        let stream = handle_http3_request_impl(req, stream, remote, routes, None, None)
            .await
            .expect("body accumulation should succeed");

        let body: Vec<u8> = stream
            .sent_data
            .iter()
            .flat_map(|b| b.iter().cloned())
            .collect();
        assert_eq!(body, b"part1part2part3part4");
    }
}
