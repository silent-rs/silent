use crate::Result;
use crate::log::{debug, error};
use crate::ws::message::Message;
use crate::ws::upgrade::WebSocketParts;
use crate::ws::websocket_handler::WebSocketHandler;
use anyhow::anyhow;
use async_channel::{Sender as UnboundedSender, unbounded as unbounded_channel};
use async_lock::RwLock;
use async_trait::async_trait;
use async_tungstenite::tungstenite::protocol;
use async_tungstenite::{WebSocketReceiver, WebSocketSender, WebSocketStream};
use futures::io::{AsyncRead, AsyncWrite};
use futures_util::ready;
use futures_util::stream::{Stream, StreamExt};
// no direct dependency on hyper types here
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
// no direct compat usage here; constructed upstream

pub struct WebSocket<S>
where
    S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    parts: Arc<RwLock<WebSocketParts>>,
    upgrade: WebSocketStream<S>,
}

impl<S> WebSocket<S>
where
    S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    #[inline]
    pub(crate) async fn from_raw_socket(
        upgraded: crate::ws::upgrade::Upgraded<S>,
        role: protocol::Role,
        config: Option<protocol::WebSocketConfig>,
    ) -> Self {
        let (parts, upgraded) = upgraded.into_parts();
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
    pub async fn recv(&mut self) -> Option<Result<Message>> {
        self.next().await
    }

    /// Send a message.
    pub async fn send(&mut self, msg: Message) -> Result<()> {
        self.upgrade
            .send(msg.inner)
            .await
            .map_err(|e| anyhow!("send error: {}", e).into())
    }

    /// Gracefully close this websocket.
    #[inline]
    pub async fn close(mut self) -> Result<()> {
        self.upgrade
            .close(None)
            .await
            .map_err(|e| anyhow!("close error: {}", e).into())
    }
}

impl<S> WebSocket<S>
where
    S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    #[inline]
    pub fn split(self) -> (WebSocketSender<S>, WebSocketReceiver<S>) {
        let Self { parts: _, upgrade } = self;
        upgrade.split()
    }
}

// Removed Sink<Message> impl due to async-tungstenite >=0.32 API changes.

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
    S,
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
    > for WebSocket<S>
where
    S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
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
                    match on_send(message.clone(), sender_parts.clone()).await {
                        Ok(message) => message,
                        Err(e) => {
                            error!("websocket on_send error: {}", e);
                            continue;
                        }
                    }
                } else {
                    message
                };

                debug!("send message: {:?}", message);
                if let Err(e) = ws_tx.send(message.inner).await {
                    error!("websocket send error: {}", e);
                    break;
                }
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
                        && on_receive(Message { inner: message }, receiver_parts.clone())
                            .await
                            .is_err()
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

impl<S> Stream for WebSocket<S>
where
    S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
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

#[cfg(test)]
mod tests {
    use super::*;
    use async_channel::unbounded as unbounded_channel;

    use futures::FutureExt;
    // ==================== Message 类型测试 ====================

    #[test]
    fn test_message_creation() {
        // 验证可以创建不同类型的消息
        let text_msg = Message::text("hello");
        let binary_msg = Message::binary(vec![1, 2, 3]);
        let close_msg = Message::close();

        // 验证消息类型
        assert!(text_msg.is_text());
        assert!(binary_msg.is_binary());
        assert!(close_msg.is_close());
    }

    #[test]
    fn test_message_cloning() {
        // 验证消息可以克隆
        let msg = Message::text("test");
        let msg2 = msg.clone();

        assert_eq!(msg.to_str().unwrap(), msg2.to_str().unwrap());
    }

    // ==================== Channel 行为测试 ====================

    #[test]
    fn test_channel_creation_and_clone() {
        // 测试通道创建和克隆
        let (tx, _rx) = unbounded_channel::<Message>();

        // 验证 sender 可以克隆
        let _tx2 = tx.clone();
    }

    #[test]
    fn test_channel_send() {
        // 测试通道发送
        let (tx, _rx) = unbounded_channel::<Message>();

        let msg = Message::text("test message");

        // 发送消息并立即等待结果
        let _ = tx.send(msg).now_or_never();
    }

    #[test]
    fn test_channel_close() {
        // 测试通道关闭
        let (tx, _rx) = unbounded_channel::<Message>();

        // 关闭 sender
        drop(tx);
    }

    // ==================== 边界条件测试 ====================

    #[test]
    fn test_empty_message() {
        // 测试空消息
        let msg = Message::text("");
        assert_eq!(msg.to_str().unwrap(), "");
    }

    #[test]
    fn test_large_binary_message() {
        // 测试大二进制消息
        let large_data = vec![0u8; 1024 * 1024]; // 1MB
        let msg = Message::binary(large_data);
        assert!(msg.is_binary());
    }

    #[test]
    fn test_unicode_message() {
        // 测试 Unicode 消息
        let unicode_str = "你好世界 🌍";
        let msg = Message::text(unicode_str);
        assert_eq!(msg.to_str().unwrap(), unicode_str);
    }

    // ==================== 类型验证测试 ====================

    #[test]
    fn test_message_inner_field() {
        // 测试 Message 的 inner 字段访问
        let msg = Message::text("test");

        // 验证可以访问 inner 字段
        let _inner = msg.inner;
    }
}
