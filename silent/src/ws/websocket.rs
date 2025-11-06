use crate::log::debug;
use crate::ws::message::Message;
use crate::ws::upgrade::{Upgraded, WebSocketParts};
use crate::ws::websocket_handler::WebSocketHandler;
use crate::{Result, SilentError};
use anyhow::anyhow;
use async_channel::{Sender as UnboundedSender, unbounded as unbounded_channel};
use async_lock::RwLock;
use async_trait::async_trait;
use async_tungstenite::WebSocketStream;
use async_tungstenite::tungstenite::protocol;
use futures_util::sink::{Sink, SinkExt};
use futures_util::stream::{Stream, StreamExt};
use futures_util::{future, ready};
use hyper::upgrade::Upgraded as HyperUpgraded;
use hyper_util::rt::TokioIo;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use tokio_util::compat::TokioAsyncReadCompatExt;

pub struct WebSocket {
    parts: Arc<RwLock<WebSocketParts>>,
    upgrade: WebSocketStream<tokio_util::compat::Compat<TokioIo<HyperUpgraded>>>,
}

unsafe impl Sync for WebSocket {}

impl WebSocket {
    #[inline]
    pub(crate) async fn from_raw_socket(
        upgraded: Upgraded,
        role: protocol::Role,
        config: Option<protocol::WebSocketConfig>,
    ) -> Self {
        let (parts, upgraded) = upgraded.into_parts();
        let upgraded = TokioIo::new(upgraded).compat();
        Self {
            parts: Arc::new(RwLock::new(parts)),
            upgrade: WebSocketStream::from_raw_socket(upgraded, role, config).await,
        }
    }

    #[inline]
    pub fn into_parts(self) -> (Arc<RwLock<WebSocketParts>>, Self) {
        (self.parts.clone(), self)
    }

    /// Receive another message.
    ///
    /// Returns `None` if the stream has closed.
    #[allow(dead_code)]
    pub async fn recv(&mut self) -> Option<Result<Message>> {
        self.next().await
    }

    /// Send a message.
    #[allow(dead_code)]
    pub async fn send(&mut self, msg: Message) -> Result<()> {
        self.upgrade
            .send(msg.inner)
            .await
            .map_err(|e| anyhow!("send error: {}", e).into())
    }

    /// Gracefully close this websocket.
    #[allow(dead_code)]
    #[inline]
    pub async fn close(mut self) -> Result<()> {
        future::poll_fn(|cx| Pin::new(&mut self).poll_close(cx)).await
    }
}

impl Sink<Message> for WebSocket {
    type Error = SilentError;

    #[inline]
    fn poll_ready(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<()>> {
        Pin::new(&mut self.upgrade)
            .poll_ready(cx)
            .map_err(|e| anyhow!("poll_ready error: {}", e).into())
    }

    #[inline]
    fn start_send(mut self: Pin<&mut Self>, item: Message) -> Result<()> {
        Pin::new(&mut self.upgrade)
            .start_send(item.inner)
            .map_err(|e| anyhow!("start_send error: {}", e).into())
    }

    #[inline]
    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<Result<()>> {
        Pin::new(&mut self.upgrade)
            .poll_flush(cx)
            .map_err(|e| anyhow!("poll_flush error: {}", e).into())
    }

    #[inline]
    fn poll_close(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<Result<()>> {
        Pin::new(&mut self.upgrade)
            .poll_close(cx)
            .map_err(|e| anyhow!("poll_close error: {}", e).into())
    }
}

#[async_trait]
pub trait WebSocketHandlerTrait<
    FnOnConnect,
    FnOnConnectFut,
    FnOnSend,
    FnOnSendFut,
    FnOnReceive,
    FnOnReceiveFut,
    FnOnClose,
    FnOnCloseFut,
> where
    FnOnConnect: Fn(Arc<RwLock<WebSocketParts>>, UnboundedSender<Message>) -> FnOnConnectFut
        + Send
        + Sync
        + 'static,
    FnOnConnectFut: Future<Output = Result<()>> + Send + 'static,
    FnOnSend: Fn(Message, Arc<RwLock<WebSocketParts>>) -> FnOnSendFut + Send + Sync + 'static,
    FnOnSendFut: Future<Output = Result<Message>> + Send + 'static,
    FnOnReceive: Fn(Message, Arc<RwLock<WebSocketParts>>) -> FnOnReceiveFut + Send + Sync + 'static,
    FnOnReceiveFut: Future<Output = Result<()>> + Send + 'static,
    FnOnClose: Fn(Arc<RwLock<WebSocketParts>>) -> FnOnCloseFut + Send + Sync + 'static,
    FnOnCloseFut: Future<Output = ()> + Send + 'static,
{
    async fn handle(
        self,
        handler: Arc<
            WebSocketHandler<
                FnOnConnect,
                FnOnConnectFut,
                FnOnSend,
                FnOnSendFut,
                FnOnReceive,
                FnOnReceiveFut,
                FnOnClose,
                FnOnCloseFut,
            >,
        >,
    ) -> Result<()>;
}

#[async_trait]
impl<
    FnOnConnect,
    FnOnConnectFut,
    FnOnSend,
    FnOnSendFut,
    FnOnReceive,
    FnOnReceiveFut,
    FnOnClose,
    FnOnCloseFut,
>
    WebSocketHandlerTrait<
        FnOnConnect,
        FnOnConnectFut,
        FnOnSend,
        FnOnSendFut,
        FnOnReceive,
        FnOnReceiveFut,
        FnOnClose,
        FnOnCloseFut,
    > for WebSocket
where
    FnOnConnect: Fn(Arc<RwLock<WebSocketParts>>, UnboundedSender<Message>) -> FnOnConnectFut
        + Send
        + Sync
        + 'static,
    FnOnConnectFut: Future<Output = Result<()>> + Send + 'static,
    FnOnSend: Fn(Message, Arc<RwLock<WebSocketParts>>) -> FnOnSendFut + Send + Sync + 'static,
    FnOnSendFut: Future<Output = Result<Message>> + Send + 'static,
    FnOnReceive: Fn(Message, Arc<RwLock<WebSocketParts>>) -> FnOnReceiveFut + Send + Sync + 'static,
    FnOnReceiveFut: Future<Output = Result<()>> + Send + 'static,
    FnOnClose: Fn(Arc<RwLock<WebSocketParts>>) -> FnOnCloseFut + Send + Sync + 'static,
    FnOnCloseFut: Future<Output = ()> + Send + 'static,
{
    async fn handle(
        self,
        handler: Arc<
            WebSocketHandler<
                FnOnConnect,
                FnOnConnectFut,
                FnOnSend,
                FnOnSendFut,
                FnOnReceive,
                FnOnReceiveFut,
                FnOnClose,
                FnOnCloseFut,
            >,
        >,
    ) -> Result<()> {
        // let WebSocketHandler { on_connect, on_send, on_receive, on_close, } = handler;
        let on_connect = handler.on_connect.clone();
        let on_send = handler.on_send.clone();
        let on_receive = handler.on_receive.clone();
        let on_close = handler.on_close.clone();

        let (parts, ws) = self.into_parts();
        let (mut ws_tx, mut ws_rx) = ws.split();

        let (tx, rx) = unbounded_channel();
        debug!("on_connect: {:?}", parts);
        if let Some(on_connect) = on_connect {
            on_connect(parts.clone(), tx.clone()).await?;
        }
        let sender_parts = parts.clone();
        let receiver_parts = parts;

        let fut = async move {
            while let Ok(message) = rx.recv().await {
                let message = if let Some(on_send) = on_send.clone() {
                    on_send(message.clone(), sender_parts.clone())
                        .await
                        .unwrap()
                } else {
                    message
                };

                debug!("send message: {:?}", message);
                ws_tx.send(message).await.unwrap();
            }
        };
        async_global_executor::spawn(fut).detach();
        let fut = async move {
            while let Some(message) = ws_rx.next().await {
                if let Ok(message) = message {
                    if message.is_close() {
                        break;
                    }
                    debug!("receive message: {:?}", message);
                    if let Some(on_receive) = on_receive.clone()
                        && on_receive(message, receiver_parts.clone()).await.is_err()
                    {
                        break;
                    }
                }
            }

            if let Some(on_close) = on_close {
                on_close(receiver_parts).await;
            }
        };
        async_global_executor::spawn(fut).detach();
        Ok(())
    }
}

impl Stream for WebSocket {
    type Item = Result<Message>;

    #[inline]
    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<Self::Item>> {
        match ready!(Pin::new(&mut self.upgrade).poll_next(cx)) {
            Some(Ok(item)) => Poll::Ready(Some(Ok(Message { inner: item }))),
            Some(Err(e)) => {
                debug!("websocket poll error: {}", e);
                Poll::Ready(Some(Err(anyhow!("websocket poll error: {}", e).into())))
            }
            None => {
                debug!("websocket closed");
                Poll::Ready(None)
            }
        }
    }
}
