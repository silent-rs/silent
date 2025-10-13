use tracing::{error, info, warn};

use super::core::{QuicSession, WebTransportHandler, WebTransportStream};
use crate::protocol::Protocol as _;
use crate::protocol::hyper_http::HyperHttpProtocol;
use crate::route::Route;
use crate::{Handler, Request};
use anyhow::{Context, Result, anyhow};
use bytes::{Buf, Bytes, BytesMut};
use h3::ext::Protocol as H3Protocol;
use h3::server::{RequestResolver, RequestStream};
use h3_quinn::Connection as H3QuinnConnection;
use http::{Method, Request as HttpRequest, Response, StatusCode};
use http_body_util::BodyExt;
use std::{net::SocketAddr, sync::Arc};

/// 处理 QUIC 连接
///
/// 此函数处理 QUIC 连接并建立 HTTP/3 会话。
/// 它会接受多个 HTTP/3 请求并为每个请求生成一个处理任务。
pub(crate) async fn handle_quic_connection(
    incoming: quinn::Incoming,
    routes: Arc<Route>,
) -> Result<()> {
    info!("准备建立 QUIC 连接");
    let connection = incoming.await.context("等待 QUIC 连接建立失败")?;
    let remote = connection.remote_address();
    info!(%remote, "客户端连接建立");

    // 默认的 WebTransport Handler
    let handler = Arc::new(super::echo::EchoHandler::default());

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
    mut stream: RequestStream<h3_quinn::BidiStream<Bytes>, Bytes>,
    remote: SocketAddr,
    routes: Arc<Route>,
) -> Result<()> {
    let mut body_buf = BytesMut::new();
    while let Some(mut chunk) = stream
        .recv_data()
        .await
        .map_err(|err| anyhow!("读取 HTTP/3 请求体失败: {err}"))?
    {
        let bytes = chunk.copy_to_bytes(chunk.remaining());
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
        .await
        .map_err(|err| anyhow!("发送 HTTP/3 响应头失败: {err}"))?;
    while let Some(frame) = body.frame().await {
        let frame = frame.map_err(|err| anyhow!("读取响应体失败: {err}"))?;
        if let Ok(data) = frame.into_data() {
            if data.is_empty() {
                continue;
            }
            stream
                .send_data(data)
                .await
                .map_err(|err| anyhow!("发送 HTTP/3 响应数据失败: {err}"))?;
        }
    }
    stream
        .finish()
        .await
        .map_err(|err| anyhow!("结束 HTTP/3 响应失败: {err}"))?;
    Ok(())
}

async fn handle_webtransport_request(
    request: HttpRequest<()>,
    mut stream: RequestStream<h3_quinn::BidiStream<Bytes>, Bytes>,
    remote: SocketAddr,
    handler: Arc<dyn WebTransportHandler>,
) -> Result<()> {
    let draft_header = request
        .headers()
        .get("sec-webtransport-http3-draft")
        .cloned();
    let mut response_builder = Response::builder().status(StatusCode::OK);
    if let Some(value) = draft_header {
        response_builder = response_builder.header("sec-webtransport-http3-draft", value);
    }
    stream
        .send_response(response_builder.body(()).unwrap())
        .await
        .map_err(|err| anyhow!("发送 WebTransport 握手响应失败: {err}"))?;
    info!(%remote, "WebTransport 会话建立");
    let session = Arc::new(QuicSession::new(remote));
    let mut channel = WebTransportStream::new(stream);
    handler.handle(session, &mut channel).await
}
