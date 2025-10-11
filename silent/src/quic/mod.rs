#![cfg(feature = "quic")]

use anyhow::{anyhow, Context, Result};
use bytes::{Buf, Bytes, BytesMut};
use h3::ext::Protocol as H3Protocol;
use h3::server::{RequestResolver, RequestStream};
use h3_quinn::Connection as H3QuinnConnection;
use http::{Method, Request as HttpRequest, Response, StatusCode};
use http_body_util::BodyExt;
use quinn::{Endpoint, ServerConfig};
use scru128::Scru128Id;
use std::net::{SocketAddr, TcpListener};
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, AtomicU16, Ordering};
use std::sync::Arc;
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio_rustls::TlsAcceptor;
use tracing::{error, info, warn};

use crate::prelude::Route;
use crate::protocol::hyper_http::HyperHttpProtocol;
use crate::protocol::Protocol as _;
use crate::service::listener::{AcceptFuture, Listen, Listener};
use crate::service::{BoxError, ConnectionFuture, ConnectionService, Server};
use crate::service::connection::BoxedConnection;
use crate::SocketAddr as CoreSocketAddr;
use crate::{Handler, MiddleWareHandler, Next, Request, Response as SilentResponse};

#[derive(Clone)]
pub struct QuicSession {
    id: String,
    remote_addr: SocketAddr,
}

impl QuicSession {
    pub fn new(remote_addr: SocketAddr) -> Self {
        let id = Scru128Id::from_u128(rand::random()).to_string();
        Self { id, remote_addr }
    }
    pub fn id(&self) -> &str { &self.id }
    pub fn remote_addr(&self) -> SocketAddr { self.remote_addr }
}

pub struct WebTransportStream {
    inner: RequestStream<h3_quinn::BidiStream<Bytes>, Bytes>,
}

impl WebTransportStream {
    fn new(inner: RequestStream<h3_quinn::BidiStream<Bytes>, Bytes>) -> Self { Self { inner } }
    pub async fn recv_data(&mut self) -> Result<Option<Bytes>> {
        match self.inner.recv_data().await? {
            Some(mut buf) => Ok(Some(buf.copy_to_bytes(buf.remaining()))),
            None => Ok(None),
        }
    }
    pub async fn send_data(&mut self, data: Bytes) -> Result<()> { Ok(self.inner.send_data(data).await?) }
    pub async fn finish(&mut self) -> Result<()> { Ok(self.inner.finish().await?) }
}

#[async_trait::async_trait]
pub trait WebTransportHandler: Send + Sync {
    async fn start(&self) -> Result<()> { Ok(()) }
    async fn handle(&self, session: Arc<QuicSession>, stream: &mut WebTransportStream) -> Result<()>;
}

#[derive(Clone, Default)]
pub struct EchoHandler;

#[async_trait::async_trait]
impl WebTransportHandler for EchoHandler {
    async fn handle(&self, session: Arc<QuicSession>, stream: &mut WebTransportStream) -> Result<()> {
        let mut payload = Bytes::new();
        while let Some(chunk) = stream.recv_data().await? {
            if payload.is_empty() { payload = chunk; } else {
                let mut buf = Vec::with_capacity(payload.len() + chunk.len());
                buf.extend_from_slice(&payload);
                buf.extend_from_slice(&chunk);
                payload = Bytes::from(buf);
            }
        }
        let message = String::from_utf8(payload.to_vec()).unwrap_or_else(|_| "<binary>".to_string());
        info!(session_id = session.id(), remote = %session.remote_addr(), "收到 WebTransport 消息: {message}");
        let response = format!("echo(webtransport): {message}");
        stream.send_data(Bytes::from(response)).await?;
        stream.finish().await?;
        Ok(())
    }
}

#[derive(Clone)]
struct AltSvcMiddleware {
    quic_available: Arc<AtomicBool>,
    quic_port: Arc<AtomicU16>,
}

impl AltSvcMiddleware {
    fn new(quic_available: Arc<AtomicBool>, quic_port: Arc<AtomicU16>) -> Self { Self { quic_available, quic_port } }
}

#[async_trait::async_trait]
impl MiddleWareHandler for AltSvcMiddleware {
    async fn handle(&self, req: Request, next: &Next) -> crate::Result<SilentResponse> {
        let mut response = next.call(req).await?;
        if self.quic_available.load(Ordering::Relaxed) {
            let port = self.quic_port.load(Ordering::Relaxed);
            if port != 0 {
                let val = format!("h3=\":{}\"; ma=86400", port);
                if let Ok(h) = http::HeaderValue::from_str(&val) { response.headers_mut().insert("alt-svc", h); }
            }
        } else { response.headers_mut().remove("alt-svc"); }
        Ok(response)
    }
}

pub async fn run_server(
    routes: Route,
    alt_svc_port: u16,
    quic_bind_addr: SocketAddr,
    https_bind_addr: SocketAddr,
    tls_acceptor: TlsAcceptor,
    quic_server_config: ServerConfig,
) -> Result<()> {
    let quic_available = Arc::new(AtomicBool::new(false));
    let quic_port = Arc::new(AtomicU16::new(alt_svc_port));
    let routes = Arc::new(routes.hook(AltSvcMiddleware::new(quic_available.clone(), quic_port.clone())));
    let handler: Arc<dyn WebTransportHandler> = Arc::new(EchoHandler);

    let http_listener = Listener::from(TcpListener::bind(https_bind_addr)?).tls(tls_acceptor);

    let quic_endpoint = match Endpoint::server(quic_server_config, quic_bind_addr) {
        Ok(endpoint) => { quic_available.store(true, Ordering::Relaxed); Some(endpoint) }
        Err(err) => { warn!("启动 QUIC Endpoint 失败: {err}"); quic_available.store(false, Ordering::Relaxed); None }
    };

    let service = HybridService::new(routes, handler, quic_available.clone(), quic_port.clone());
    let mut server = Server::new().listen(http_listener);
    if let Some(endpoint) = quic_endpoint { server = server.listen(QuicEndpointListener::new(endpoint)); }
    server.serve(service).await;
    Ok(())
}

struct HybridService {
    routes: Arc<Route>,
    handler: Arc<dyn WebTransportHandler>,
    quic_available: Arc<AtomicBool>,
    quic_port: Arc<AtomicU16>,
}

impl HybridService {
    fn new(routes: Arc<Route>, handler: Arc<dyn WebTransportHandler>, quic_available: Arc<AtomicBool>, quic_port: Arc<AtomicU16>) -> Self { Self { routes, handler, quic_available, quic_port } }
}

impl ConnectionService for HybridService {
    fn call(&self, stream: BoxedConnection, peer: CoreSocketAddr) -> ConnectionFuture {
        let routes = Arc::clone(&self.routes);
        let handler = Arc::clone(&self.handler);
        let quic_available = Arc::clone(&self.quic_available);
        let quic_port = Arc::clone(&self.quic_port);
        match stream.downcast::<QuicConnection>() {
            Ok(quic) => Box::pin(async move {
                let incoming = quic.into_incoming();
                match handle_quic_connection(incoming, routes, handler, quic_available.clone(), quic_port.clone()).await {
                    Ok(()) => Ok(()),
                    Err(err) => { quic_available.store(false, Ordering::Relaxed); Err(BoxError::from(err)) }
                }
            }),
            Err(stream) => Box::pin(async move { ConnectionService::call(&*routes, stream, peer).await }),
        }
    }
}

struct QuicEndpointListener { endpoint: Endpoint }
impl QuicEndpointListener { fn new(endpoint: Endpoint) -> Self { Self { endpoint } } }
impl Listen for QuicEndpointListener {
    fn accept(&self) -> AcceptFuture<'_> { Box::pin(async move {
        match self.endpoint.accept().await {
            Some(incoming) => {
                let remote = CoreSocketAddr::from(incoming.remote_address());
                let connection: BoxedConnection = Box::new(QuicConnection::new(incoming));
                Ok((connection, remote))
            }
            None => Err(std::io::Error::other("QUIC Endpoint 已关闭")),
        }
    }) }
    fn local_addr(&self) -> std::io::Result<CoreSocketAddr> { self.endpoint.local_addr().map(CoreSocketAddr::from).map_err(std::io::Error::other) }
}

pub struct QuicConnection { connecting: Option<quinn::Incoming> }
impl QuicConnection { fn new(connecting: quinn::Incoming) -> Self { Self { connecting: Some(connecting) } } fn into_incoming(mut self) -> quinn::Incoming { self.connecting.take().expect("connecting available") } }
impl AsyncRead for QuicConnection { fn poll_read(self: Pin<&mut Self>, _cx: &mut std::task::Context<'_>, buf: &mut ReadBuf<'_>) -> std::task::Poll<std::io::Result<()>> { buf.clear(); std::task::Poll::Ready(Ok(())) } }
impl AsyncWrite for QuicConnection {
    fn poll_write(self: Pin<&mut Self>, _cx: &mut std::task::Context<'_>, _buf: &[u8]) -> std::task::Poll<std::io::Result<usize>> { std::task::Poll::Ready(Ok(0)) }
    fn poll_flush(self: Pin<&mut Self>, _cx: &mut std::task::Context<'_>) -> std::task::Poll<std::io::Result<()>> { std::task::Poll::Ready(Ok(())) }
    fn poll_shutdown(self: Pin<&mut Self>, _cx: &mut std::task::Context<'_>) -> std::task::Poll<std::io::Result<()>> { std::task::Poll::Ready(Ok(())) }
}

async fn handle_quic_connection(
    incoming: quinn::Incoming,
    routes: Arc<Route>,
    handler: Arc<dyn WebTransportHandler>,
    quic_available: Arc<AtomicBool>,
    quic_port: Arc<AtomicU16>,
) -> Result<()> {
    info!("准备建立 QUIC 连接");
    let connection = incoming.await.context("等待 QUIC 连接建立失败")?;
    let remote = connection.remote_address();
    info!(%remote, "客户端连接建立");

    quic_available.store(true, Ordering::Relaxed);

    let mut builder = h3::server::builder();
    builder.enable_extended_connect(true).enable_datagram(true).enable_webtransport(true).max_webtransport_sessions(32);
    let mut h3_conn = builder.build(H3QuinnConnection::new(connection.clone())).await.context("构建 HTTP/3 连接失败")?;

    loop {
        match h3_conn.accept().await {
            Ok(Some(resolver)) => {
                let routes = Arc::clone(&routes);
                let handler = Arc::clone(&handler);
                let quic_available = Arc::clone(&quic_available);
                let quic_port = Arc::clone(&quic_port);
                tokio::spawn(async move {
                    if let Err(err) = handle_request(resolver, remote, routes, handler, quic_available, quic_port).await { error!(%remote, error = ?err, "处理 HTTP/3 请求失败"); }
                });
            }
            Ok(None) => break,
            Err(err) => { warn!(%remote, error = ?err, "HTTP/3 连接异常结束"); break; }
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
    quic_available: Arc<AtomicBool>,
    quic_port: Arc<AtomicU16>,
) -> Result<()> {
    let (request, stream) = resolver.resolve_request().await.map_err(|err| anyhow!("解析 HTTP/3 请求失败: {err}"))?;
    let protocol = request.extensions().get::<H3Protocol>().cloned();
    if request.method() == Method::CONNECT && matches!(protocol, Some(H3Protocol::WEB_TRANSPORT)) {
        handle_webtransport_request(request, stream, remote, handler, quic_port).await
    } else {
        handle_http3_request(request, stream, remote, routes, quic_available).await
    }
}

async fn handle_http3_request(
    request: HttpRequest<()>,
    mut stream: RequestStream<h3_quinn::BidiStream<Bytes>, Bytes>,
    remote: SocketAddr,
    routes: Arc<Route>,
    quic_available: Arc<AtomicBool>,
) -> Result<()> {
    let mut body_buf = BytesMut::new();
    while let Some(mut chunk) = stream.recv_data().await.map_err(|err| anyhow!("读取 HTTP/3 请求体失败: {err}"))? {
        let bytes = chunk.copy_to_bytes(chunk.remaining());
        if !bytes.is_empty() { body_buf.extend_from_slice(&bytes); }
    }
    let (parts, _) = request.into_parts();
    let body = if body_buf.is_empty() { crate::prelude::ReqBody::Empty } else { crate::prelude::ReqBody::Once(body_buf.freeze()) };
    let mut silent_req = Request::from_parts(parts, body);
    silent_req.set_remote(remote.into());
    let response = Handler::call(&*routes, silent_req).await.unwrap_or_else(Into::into);
    let hyper_response = HyperHttpProtocol::from_internal(response);
    let (parts, mut body) = hyper_response.into_parts();
    stream.send_response(Response::from_parts(parts, ())).await.map_err(|err| anyhow!("发送 HTTP/3 响应头失败: {err}"))?;
    while let Some(frame) = body.frame().await { let frame = frame.map_err(|err| anyhow!("读取响应体失败: {err}"))?; if let Ok(data) = frame.into_data() { if data.is_empty() { continue; } stream.send_data(data).await.map_err(|err| anyhow!("发送 HTTP/3 响应数据失败: {err}"))?; } }
    stream.finish().await.map_err(|err| anyhow!("结束 HTTP/3 响应失败: {err}"))?;
    quic_available.store(true, Ordering::Relaxed);
    Ok(())
}

async fn handle_webtransport_request(
    request: HttpRequest<()>,
    mut stream: RequestStream<h3_quinn::BidiStream<Bytes>, Bytes>,
    remote: SocketAddr,
    handler: Arc<dyn WebTransportHandler>,
    quic_port: Arc<AtomicU16>,
) -> Result<()> {
    let draft_header = request.headers().get("sec-webtransport-http3-draft").cloned();
    let mut response_builder = Response::builder().status(StatusCode::OK);
    if let Some(value) = draft_header { response_builder = response_builder.header("sec-webtransport-http3-draft", value); }
    let port = quic_port.load(Ordering::Relaxed);
    if port != 0 { let v = format!("h3=\":{}\"; ma=86400", port); response_builder = response_builder.header("alt-svc", v); }
    stream.send_response(response_builder.body(()).unwrap()).await.map_err(|err| anyhow!("发送 WebTransport 握手响应失败: {err}"))?;
    info!(%remote, "WebTransport 会话建立");
    let session = Arc::new(QuicSession::new(remote));
    let mut channel = WebTransportStream::new(stream);
    handler.handle(session, &mut channel).await
}
