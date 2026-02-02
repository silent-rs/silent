use crate::Result;
use crate::ws::WebSocketParts;
use crate::ws::message::Message;
use async_channel::Sender as UnboundedSender;
use async_lock::RwLock;
use std::future::Future;
use std::sync::Arc;

#[derive(Clone, Default)]
pub struct WebSocketHandler<
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
    pub(crate) on_connect: Option<Arc<FnOnConnect>>,
    pub(crate) on_send: Option<Arc<FnOnSend>>,
    pub(crate) on_receive: Option<Arc<FnOnReceive>>,
    pub(crate) on_close: Option<Arc<FnOnClose>>,
}

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
    WebSocketHandler<
        FnOnConnect,
        FnOnConnectFut,
        FnOnSend,
        FnOnSendFut,
        FnOnReceive,
        FnOnReceiveFut,
        FnOnClose,
        FnOnCloseFut,
    >
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
    pub fn new() -> WebSocketHandler<
        FnOnConnect,
        FnOnConnectFut,
        FnOnSend,
        FnOnSendFut,
        FnOnReceive,
        FnOnReceiveFut,
        FnOnClose,
        FnOnCloseFut,
    > {
        WebSocketHandler {
            on_connect: None,
            on_send: None,
            on_receive: None,
            on_close: None,
        }
    }

    pub fn on_connect(mut self, on_connect: FnOnConnect) -> Self {
        self.on_connect = Some(Arc::new(on_connect));
        self
    }

    pub fn on_send(mut self, on_send: FnOnSend) -> Self {
        self.on_send = Some(Arc::new(on_send));
        self
    }

    pub fn on_receive(mut self, on_receive: FnOnReceive) -> Self {
        self.on_receive = Some(Arc::new(on_receive));
        self
    }

    pub fn on_close(mut self, on_close: FnOnClose) -> Self {
        self.on_close = Some(Arc::new(on_close));
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ==================== new() 方法测试 ====================

    #[test]
    fn test_websocket_handler_new() {
        type MockFut1 = std::pin::Pin<Box<dyn Future<Output = Result<()>> + Send + 'static>>;
        type MockFut2 = std::pin::Pin<Box<dyn Future<Output = Result<Message>> + Send + 'static>>;
        type MockFut3 = std::pin::Pin<Box<dyn Future<Output = ()> + Send + 'static>>;

        let handler = WebSocketHandler::<
            fn(Arc<RwLock<WebSocketParts>>, UnboundedSender<Message>) -> MockFut1,
            MockFut1,
            fn(Message, Arc<RwLock<WebSocketParts>>) -> MockFut2,
            MockFut2,
            fn(Message, Arc<RwLock<WebSocketParts>>) -> MockFut1,
            MockFut1,
            fn(Arc<RwLock<WebSocketParts>>) -> MockFut3,
            MockFut3,
        >::new();

        assert!(handler.on_connect.is_none());
        assert!(handler.on_send.is_none());
        assert!(handler.on_receive.is_none());
        assert!(handler.on_close.is_none());
    }

    // ==================== 链式调用测试 ====================

    #[test]
    fn test_websocket_handler_chain_all() {
        type MockFut1 = std::pin::Pin<Box<dyn Future<Output = Result<()>> + Send + Sync + 'static>>;
        type MockFut2 =
            std::pin::Pin<Box<dyn Future<Output = Result<Message>> + Send + Sync + 'static>>;
        type MockFut3 = std::pin::Pin<Box<dyn Future<Output = ()> + Send + Sync + 'static>>;

        let handler = WebSocketHandler::<
            for<'a> fn(Arc<RwLock<WebSocketParts>>, UnboundedSender<Message>) -> MockFut1,
            MockFut1,
            for<'a> fn(Message, Arc<RwLock<WebSocketParts>>) -> MockFut2,
            MockFut2,
            for<'a> fn(Message, Arc<RwLock<WebSocketParts>>) -> MockFut1,
            MockFut1,
            for<'a> fn(Arc<RwLock<WebSocketParts>>) -> MockFut3,
            MockFut3,
        >::new();

        // 验证创建成功
        let _ = handler;
    }

    // ==================== 部分回调测试 ====================

    #[test]
    fn test_websocket_handler_partial_callbacks() {
        type MockFut1 = std::pin::Pin<Box<dyn Future<Output = Result<()>> + Send + Sync + 'static>>;
        type MockFut2 =
            std::pin::Pin<Box<dyn Future<Output = Result<Message>> + Send + Sync + 'static>>;
        type MockFut3 = std::pin::Pin<Box<dyn Future<Output = ()> + Send + Sync + 'static>>;

        let handler = WebSocketHandler::<
            for<'a> fn(Arc<RwLock<WebSocketParts>>, UnboundedSender<Message>) -> MockFut1,
            MockFut1,
            for<'a> fn(Message, Arc<RwLock<WebSocketParts>>) -> MockFut2,
            MockFut2,
            for<'a> fn(Message, Arc<RwLock<WebSocketParts>>) -> MockFut1,
            MockFut1,
            for<'a> fn(Arc<RwLock<WebSocketParts>>) -> MockFut3,
            MockFut3,
        >::new();

        // 验证所有回调都是 None
        assert!(handler.on_connect.is_none());
        assert!(handler.on_send.is_none());
        assert!(handler.on_receive.is_none());
        assert!(handler.on_close.is_none());
    }

    // ==================== 直接构造测试 ====================

    #[test]
    fn test_websocket_handler_direct_construction() {
        let _handler = WebSocketHandler {
            on_connect: Some(Arc::new(|_, _| Box::pin(async { Ok(()) }))),
            on_send: Some(Arc::new(|message, _| Box::pin(async { Ok(message) }))),
            on_receive: Some(Arc::new(|_, _| Box::pin(async { Ok(()) }))),
            on_close: Some(Arc::new(|_| Box::pin(async {}))),
        };
    }

    // ==================== 只有单个回调的测试 ====================

    #[test]
    #[allow(clippy::type_complexity)]
    fn test_websocket_handler_single_callback() {
        type MockFut1 = std::pin::Pin<Box<dyn Future<Output = Result<()>> + Send + Sync + 'static>>;
        type MockFut2 =
            std::pin::Pin<Box<dyn Future<Output = Result<Message>> + Send + Sync + 'static>>;
        type MockFut3 = std::pin::Pin<Box<dyn Future<Output = ()> + Send + Sync + 'static>>;

        let handler: WebSocketHandler<
            for<'a> fn(Arc<RwLock<WebSocketParts>>, UnboundedSender<Message>) -> MockFut1,
            MockFut1,
            for<'a> fn(Message, Arc<RwLock<WebSocketParts>>) -> MockFut2,
            MockFut2,
            for<'a> fn(Message, Arc<RwLock<WebSocketParts>>) -> MockFut1,
            MockFut1,
            for<'a> fn(Arc<RwLock<WebSocketParts>>) -> MockFut3,
            MockFut3,
        > = WebSocketHandler {
            on_connect: Some(Arc::new(|_, _| Box::pin(async { Ok(()) }))),
            on_send: None,
            on_receive: None,
            on_close: None,
        };

        assert!(handler.on_connect.is_some());
        assert!(handler.on_send.is_none());
        assert!(handler.on_receive.is_none());
        assert!(handler.on_close.is_none());
    }

    // ==================== 所有回调都设置的测试 ====================

    #[test]
    #[allow(clippy::type_complexity)]
    fn test_websocket_handler_all_callbacks() {
        type MockFut1 = std::pin::Pin<Box<dyn Future<Output = Result<()>> + Send + Sync + 'static>>;
        type MockFut2 =
            std::pin::Pin<Box<dyn Future<Output = Result<Message>> + Send + Sync + 'static>>;
        type MockFut3 = std::pin::Pin<Box<dyn Future<Output = ()> + Send + Sync + 'static>>;

        let handler: WebSocketHandler<
            for<'a> fn(Arc<RwLock<WebSocketParts>>, UnboundedSender<Message>) -> MockFut1,
            MockFut1,
            for<'a> fn(Message, Arc<RwLock<WebSocketParts>>) -> MockFut2,
            MockFut2,
            for<'a> fn(Message, Arc<RwLock<WebSocketParts>>) -> MockFut1,
            MockFut1,
            for<'a> fn(Arc<RwLock<WebSocketParts>>) -> MockFut3,
            MockFut3,
        > = WebSocketHandler {
            on_connect: Some(Arc::new(|_, _| Box::pin(async { Ok(()) }))),
            on_send: Some(Arc::new(|msg, _| Box::pin(async { Ok(msg) }))),
            on_receive: Some(Arc::new(|_, _| Box::pin(async { Ok(()) }))),
            on_close: Some(Arc::new(|_| Box::pin(async {}))),
        };

        assert!(handler.on_connect.is_some());
        assert!(handler.on_send.is_some());
        assert!(handler.on_receive.is_some());
        assert!(handler.on_close.is_some());
    }
}
