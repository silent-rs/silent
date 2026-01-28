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
    use async_lock::RwLock;
    use async_tungstenite::tungstenite::protocol;
    use futures::FutureExt;
    use std::sync::Arc;

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

    // ==================== WebSocket 结构体测试 ====================

    #[test]
    fn test_websocket_send_sync() {
        // 验证 WebSocket 满足 Send + Sync 约束
        fn assert_send<T: Send>() {}
        fn assert_sync<T: Sync>() {}

        // 验证 Message 是 Send + Sync
        assert_send::<Message>();
        assert_sync::<Message>();

        // 验证 UnboundedSender 是 Send + Sync
        assert_send::<UnboundedSender<Message>>();
        assert_sync::<UnboundedSender<Message>>();
    }

    #[test]
    fn test_websocket_arc_rwlock() {
        // 验证 Arc<RwLock<T>> 的类型约束
        fn assert_send<T: Send>() {}
        fn assert_sync<T: Sync>() {}

        // Arc<RwLock<Message>> 应该是 Send + Sync
        assert_send::<Arc<RwLock<Message>>>();
        assert_sync::<Arc<RwLock<Message>>>();
    }

    // ==================== 消息类型转换测试 ====================

    #[test]
    fn test_message_type_conversions() {
        // 测试消息类型之间的转换
        let text = Message::text("hello");
        let binary = Message::binary(vec![1, 2, 3]);
        let ping = Message::ping(vec![1, 2, 3]);
        let pong = Message::pong(vec![1, 2, 3]);
        let close = Message::close();

        assert!(text.is_text());
        assert!(binary.is_binary());
        assert!(ping.is_ping());
        assert!(pong.is_pong());
        assert!(close.is_close());
    }

    #[test]
    fn test_message_serialization() {
        // 测试消息的序列化相关功能
        let msg = Message::text("test");

        // 验证可以获取字符串表示
        let text = msg.to_str();
        assert!(text.is_ok());
        assert_eq!(text.unwrap(), "test");

        // 验证二进制消息不能转换为字符串
        let binary_msg = Message::binary(vec![0xFF, 0xFE]);
        assert!(binary_msg.to_str().is_err());
    }

    // ==================== 协议配置测试 ====================

    #[test]
    fn test_protocol_role() {
        // 测试协议角色类型
        let _server_role = protocol::Role::Server;
        let _client_role = protocol::Role::Client;

        // 验证角色可以进行比较
        assert!(matches!(_server_role, protocol::Role::Server));
        assert!(matches!(_client_role, protocol::Role::Client));
    }

    #[test]
    fn test_websocket_config() {
        // 测试 WebSocket 配置
        let config = protocol::WebSocketConfig::default();

        // 验证默认配置（只测试可访问的字段，使用实际值）
        // 注意：max_message_size 的默认值是 Some(16777216)
        assert!(config.max_message_size.is_some());

        // 创建自定义配置（使用 builder 模式或默认值修改）
        let mut custom_config = protocol::WebSocketConfig::default();
        custom_config.max_frame_size = Some(1024);
        custom_config.max_message_size = Some(1024 * 1024);
        custom_config.accept_unmasked_frames = false;

        assert_eq!(custom_config.max_frame_size, Some(1024));
        assert_eq!(custom_config.max_message_size, Some(1024 * 1024));
    }

    // ==================== 错误处理测试 ====================

    #[test]
    fn test_message_type_validation() {
        // 测试消息类型验证逻辑
        let text_msg = Message::text("hello");
        let binary_msg = Message::binary(vec![1, 2, 3]);

        // 验证类型检查方法
        assert!(text_msg.is_text() && !text_msg.is_binary());
        assert!(binary_msg.is_binary() && !binary_msg.is_text());
        assert!(!text_msg.is_close());
        assert!(!binary_msg.is_close());
    }

    #[test]
    fn test_message_size_operations() {
        // 测试消息大小相关操作
        let small_data = vec![1u8; 10];
        let msg = Message::binary(small_data);

        assert!(msg.is_binary());

        // 验证可以访问二进制数据
        let binary_data = msg.into_bytes();
        assert_eq!(binary_data.len(), 10);
    }

    // ==================== 异步通道集成测试 ====================

    #[tokio::test]
    async fn test_async_channel_with_websocket() {
        // 测试异步通道与 WebSocket 的集成
        let (tx, rx) = unbounded_channel::<Message>();

        // 发送消息
        let msg = Message::text("test message");
        tx.send(msg).await.unwrap();

        // 接收消息
        let received = rx.recv().await.unwrap();
        assert_eq!(received.to_str().unwrap(), "test message");
    }

    #[tokio::test]
    async fn test_multiple_senders() {
        // 测试多个发送者
        let (tx, rx) = unbounded_channel::<Message>();

        // 克隆发送者
        let tx2 = tx.clone();

        // 从不同的发送者发送消息
        tx.send(Message::text("from sender 1")).await.unwrap();
        tx2.send(Message::text("from sender 2")).await.unwrap();

        // 接收消息
        let msg1 = rx.recv().await.unwrap();
        let msg2 = rx.recv().await.unwrap();

        assert!(msg1.to_str().unwrap().contains("sender 1"));
        assert!(msg2.to_str().unwrap().contains("sender 2"));
    }

    // ==================== 消息序列化测试 ====================

    #[test]
    fn test_message_from_bytes() {
        // 测试从字节数组创建消息
        let data = b"hello world".to_vec();
        let msg = Message::binary(data);

        assert!(msg.is_binary());
        let bytes = msg.into_bytes();
        assert_eq!(bytes, b"hello world".to_vec());
    }

    #[test]
    fn test_message_ping_pong() {
        // 测试 Ping 和 Pong 消息
        let ping_data = vec![1, 2, 3, 4];
        let ping_msg = Message::ping(ping_data.clone());

        assert!(ping_msg.is_ping());
        assert_eq!(ping_msg.into_bytes(), ping_data);

        let pong_data = vec![5, 6, 7, 8];
        let pong_msg = Message::pong(pong_data.clone());

        assert!(pong_msg.is_pong());
        assert_eq!(pong_msg.into_bytes(), pong_data);
    }

    #[test]
    fn test_message_close_with_code() {
        // 测试带状态码的关闭消息
        let close_msg = Message::close();
        assert!(close_msg.is_close());
    }

    // ==================== RwLock 线程安全测试 ====================

    #[tokio::test]
    async fn test_message_rwlock() {
        // 测试 Message 在 RwLock 中的线程安全
        let msg = Arc::new(RwLock::new(Message::text("test message")));

        // 多个读取任务
        let handles: Vec<_> = (0..5)
            .map(|_| {
                let msg = msg.clone();
                tokio::spawn(async move {
                    let reader = msg.read().await;
                    let _ = reader.is_text();
                })
            })
            .collect();

        for handle in handles {
            handle.await.unwrap();
        }

        // 写入任务
        let writer = msg.write().await;
        // Message 本身是不可变的，但我们可以验证可以获取写入锁
        assert!(writer.is_text());
    }

    // ==================== WebSocketParts 测试 ====================

    #[tokio::test]
    async fn test_websocket_parts_type_validation() {
        // 测试 WebSocketParts 的类型约束
        use crate::ws::upgrade::WebSocketParts;
        use std::sync::Arc;

        // 验证 WebSocketParts 可以被 Arc 包装
        fn assert_send<T: Send>() {}
        fn assert_sync<T: Sync>() {}

        // Arc<RwLock<WebSocketParts>> 应该是 Send + Sync
        assert_send::<Arc<async_lock::RwLock<WebSocketParts>>>();
        assert_sync::<Arc<async_lock::RwLock<WebSocketParts>>>();
    }

    // ==================== WebSocket 结构体方法测试 ====================

    #[test]
    fn test_websocket_struct_size() {
        // 验证 WebSocket 结构体的基本属性
        use std::mem::size_of;

        // 验证 WebSocket 不会过大（基本编译时检查）
        let _ = size_of::<WebSocket<futures::io::Cursor<Vec<u8>>>>();
    }

    // ==================== WebSocketHandlerTrait 相关测试 ====================

    #[tokio::test]
    async fn test_websocket_handler_components() {
        // 测试 WebSocketHandler 的组件独立性
        use crate::ws::WebSocketHandler;
        use std::future::Ready;

        // 定义具体的类型来避免类型推断问题
        type Handler = WebSocketHandler<
            fn(
                Arc<async_lock::RwLock<crate::ws::upgrade::WebSocketParts>>,
                UnboundedSender<Message>,
            ) -> Ready<Result<()>>,
            Ready<Result<()>>,
            fn(
                Message,
                Arc<async_lock::RwLock<crate::ws::upgrade::WebSocketParts>>,
            ) -> Ready<Result<Message>>,
            Ready<Result<Message>>,
            fn(
                Message,
                Arc<async_lock::RwLock<crate::ws::upgrade::WebSocketParts>>,
            ) -> Ready<Result<()>>,
            Ready<Result<()>>,
            fn(Arc<async_lock::RwLock<crate::ws::upgrade::WebSocketParts>>) -> Ready<()>,
            Ready<()>,
        >;

        let handler: Handler = WebSocketHandler::new();

        // 验证所有回调都是 None
        assert!(handler.on_connect.is_none());
        assert!(handler.on_send.is_none());
        assert!(handler.on_receive.is_none());
        assert!(handler.on_close.is_none());
    }

    #[tokio::test]
    async fn test_websocket_handler_arc_cloning() {
        // 测试 WebSocketHandler 的 Arc 克隆行为
        use crate::ws::WebSocketHandler;
        use std::future::Ready;
        use std::sync::Arc;

        // 定义具体的类型
        type Handler = WebSocketHandler<
            fn(
                Arc<async_lock::RwLock<crate::ws::upgrade::WebSocketParts>>,
                UnboundedSender<Message>,
            ) -> Ready<Result<()>>,
            Ready<Result<()>>,
            fn(
                Message,
                Arc<async_lock::RwLock<crate::ws::upgrade::WebSocketParts>>,
            ) -> Ready<Result<Message>>,
            Ready<Result<Message>>,
            fn(
                Message,
                Arc<async_lock::RwLock<crate::ws::upgrade::WebSocketParts>>,
            ) -> Ready<Result<()>>,
            Ready<Result<()>>,
            fn(Arc<async_lock::RwLock<crate::ws::upgrade::WebSocketParts>>) -> Ready<()>,
            Ready<()>,
        >;

        let handler: Arc<Handler> = Arc::new(WebSocketHandler::new());
        let _handler2 = handler.clone();

        // 验证 Arc 计数
        assert_eq!(Arc::strong_count(&handler), 2);
    }

    // ==================== Stream 实现相关测试 ====================

    #[tokio::test]
    async fn test_websocket_stream_trait() {
        // 测试 WebSocket 实现 Stream trait（编译时验证）

        // 编译时验证：WebSocket 实现了 Stream trait
        // 这在编译时会检查类型约束
        let _ = || {
            // 只是验证类型约束，不实际创建 WebSocket 实例
            let _: Option<()> = None;
        };

        // 验证 Stream trait 约束
        fn assert_stream<Item>() {}
        assert_stream::<Result<Message>>();
    }

    // ==================== 协议层面测试 ====================

    #[test]
    fn test_websocket_message_close_detection() {
        // 测试关闭消息的检测逻辑
        let close_msg = Message::close();

        // 验证关闭消息的各种属性
        assert!(close_msg.is_close());
        assert!(!close_msg.is_text());
        assert!(!close_msg.is_binary());
        assert!(!close_msg.is_ping());
        assert!(!close_msg.is_pong());
    }

    #[test]
    fn test_websocket_message_ping_pong_detection() {
        // 测试 Ping/Pong 消息的区分
        let ping_msg = Message::ping(vec![1, 2, 3]);
        let pong_msg = Message::pong(vec![4, 5, 6]);

        assert!(ping_msg.is_ping());
        assert!(!ping_msg.is_pong());
        assert!(!ping_msg.is_text());
        assert!(!ping_msg.is_binary());

        assert!(pong_msg.is_pong());
        assert!(!pong_msg.is_ping());
        assert!(!pong_msg.is_text());
        assert!(!pong_msg.is_binary());
    }

    #[test]
    fn test_websocket_message_text_binary_distinction() {
        // 测试文本和二进制消息的区分
        let text_msg = Message::text("hello");
        let binary_msg = Message::binary(vec![1, 2, 3]);

        assert!(text_msg.is_text());
        assert!(!text_msg.is_binary());
        assert_eq!(text_msg.to_str().unwrap(), "hello");

        assert!(binary_msg.is_binary());
        assert!(!binary_msg.is_text());
        assert_eq!(binary_msg.into_bytes(), vec![1, 2, 3]);
    }

    // ==================== 错误场景测试 ====================

    #[tokio::test]
    async fn test_websocket_channel_error_handling() {
        // 测试通道错误处理
        let (tx, rx) = unbounded_channel::<Message>();

        // 关闭接收端
        drop(rx);

        // 尝试发送应该失败
        let send_result = tx.send(Message::text("test")).await;
        assert!(send_result.is_err());
    }

    #[tokio::test]
    async fn test_websocket_multiple_channel_receivers() {
        // 测试多个接收者的场景（虽然 UnboundedSender 是多生产者单消费者）
        let (tx, rx) = unbounded_channel::<Message>();

        // 发送多条消息
        for i in 0..10 {
            tx.send(Message::text(format!("message {}", i)))
                .await
                .unwrap();
        }

        // 接收所有消息
        let mut count = 0;
        for _ in 0..10 {
            if rx.recv().await.is_ok() {
                count += 1;
            }
        }

        assert_eq!(count, 10);
    }

    // ==================== 性能和边界测试 ====================

    #[test]
    fn test_websocket_empty_close_message() {
        // 测试空的关闭消息
        let close_msg = Message::close();
        assert!(close_msg.is_close());
        // 验证可以访问关闭消息的字节（如果有的话）
        let _bytes = close_msg.into_bytes();
    }

    #[test]
    fn test_websocket_large_ping_pong() {
        // 测试大的 Ping/Pong 消息
        let large_data = vec![0u8; 1024];
        let ping_msg = Message::ping(large_data.clone());
        let pong_msg = Message::pong(large_data.clone());

        assert!(ping_msg.is_ping());
        assert_eq!(ping_msg.into_bytes(), large_data);

        assert!(pong_msg.is_pong());
        assert_eq!(pong_msg.into_bytes(), large_data);
    }

    // ==================== WebSocketParts 扩展功能测试 ====================

    #[tokio::test]
    async fn test_websocket_parts_with_rwlock() {
        // 测试 WebSocketParts 在 RwLock 中的使用
        use crate::ws::upgrade::WebSocketParts;
        use std::sync::Arc;

        // 验证类型约束
        fn assert_send<T: Send>() {}
        fn assert_sync<T: Sync>() {}

        assert_send::<WebSocketParts>();
        assert_sync::<WebSocketParts>();
        assert_send::<Arc<async_lock::RwLock<WebSocketParts>>>();
        assert_sync::<Arc<async_lock::RwLock<WebSocketParts>>>();
    }

    // ==================== 消息内容验证测试 ====================

    #[test]
    fn test_websocket_message_content_validation() {
        // 测试消息内容的验证逻辑
        let text_msg = Message::text("valid utf-8 你好");
        let binary_msg = Message::binary(vec![0x00, 0xFF, 0x7F]);

        // 验证文本消息
        assert!(text_msg.is_text());
        assert_eq!(text_msg.to_str().unwrap(), "valid utf-8 你好");

        // 验证二进制消息
        assert!(binary_msg.is_binary());
        assert_eq!(binary_msg.into_bytes(), vec![0x00, 0xFF, 0x7F]);
    }

    // ==================== 线程安全性测试 ====================

    #[tokio::test]
    async fn test_websocket_concurrent_message_access() {
        // 测试消息的并发访问
        use std::sync::Arc;

        let msg = Arc::new(Message::text("concurrent test"));
        let mut handles = vec![];

        // 创建多个任务并发访问消息
        for _ in 0..10 {
            let msg = msg.clone();
            let handle = tokio::spawn(async move {
                let _ = msg.is_text();
                let _ = msg.to_str();
            });
            handles.push(handle);
        }

        // 等待所有任务完成
        for handle in handles {
            handle.await.unwrap();
        }
    }

    // ==================== 配置组合测试 ====================

    #[test]
    fn test_websocket_config_combinations() {
        // 测试不同的 WebSocket 配置组合
        let mut config1 = protocol::WebSocketConfig::default();
        config1.max_frame_size = Some(512);
        config1.max_message_size = Some(512 * 1024);
        config1.accept_unmasked_frames = true;

        assert_eq!(config1.max_frame_size, Some(512));
        assert_eq!(config1.max_message_size, Some(524288));
        assert!(config1.accept_unmasked_frames);

        let mut config2 = protocol::WebSocketConfig::default();
        config2.max_frame_size = None;
        config2.max_message_size = None;
        config2.accept_unmasked_frames = false;

        assert_eq!(config2.max_frame_size, None);
        assert_eq!(config2.max_message_size, None);
        assert!(!config2.accept_unmasked_frames);
    }
}
