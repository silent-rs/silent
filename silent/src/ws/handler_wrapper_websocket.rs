use crate::ws::handler::websocket_handler;
use crate::ws::websocket::{WebSocket, WebSocketHandlerTrait};
use crate::ws::websocket_handler::WebSocketHandler;
use crate::ws::{Message, WebSocketParts, upgrade};
use crate::{Handler, Request, Response, Result};
use async_channel::Sender as UnboundedSender;
use async_lock::RwLock;
use async_trait::async_trait;
use async_tungstenite::tungstenite::protocol;
use std::future::Future;
use std::sync::Arc;
use tracing::error;

#[allow(clippy::type_complexity)]
#[derive(Clone)]
pub struct HandlerWrapperWebSocket<
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
    pub config: Option<protocol::WebSocketConfig>,
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
    HandlerWrapperWebSocket<
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
    pub fn new(config: Option<protocol::WebSocketConfig>) -> Self {
        Self {
            config,
            handler: Arc::new(WebSocketHandler::new()),
        }
    }

    pub fn set_handler(
        mut self,
        handler: WebSocketHandler<
            FnOnConnect,
            FnOnConnectFut,
            FnOnSend,
            FnOnSendFut,
            FnOnReceive,
            FnOnReceiveFut,
            FnOnClose,
            FnOnCloseFut,
        >,
    ) -> Self {
        self.handler = Arc::from(handler);
        self
    }
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
> Handler
    for HandlerWrapperWebSocket<
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
    async fn call(&self, req: Request) -> Result<Response> {
        let res = websocket_handler(&req)?;
        let config = self.config;
        let handler = self.handler.clone();
        async_global_executor::spawn(async move {
            match upgrade::on(req).await {
                Ok(upgrade) => {
                    let ws =
                        WebSocket::from_raw_socket(upgrade, protocol::Role::Server, config).await;
                    if let Err(e) = ws.handle(handler).await {
                        error!("upgrade handle error: {}", e)
                    }
                }
                Err(e) => {
                    error!("upgrade error: {}", e)
                }
            }
        })
        .detach();
        Ok(res)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::header::HeaderValue;

    // 定义简化的测试类型别名
    type MockConnect = fn(
        Arc<RwLock<WebSocketParts>>,
        UnboundedSender<Message>,
    ) -> std::pin::Pin<Box<dyn Future<Output = Result<()>> + Send>>;
    type MockSend = fn(
        Message,
        Arc<RwLock<WebSocketParts>>,
    ) -> std::pin::Pin<Box<dyn Future<Output = Result<Message>> + Send>>;
    type MockRecv = fn(
        Message,
        Arc<RwLock<WebSocketParts>>,
    ) -> std::pin::Pin<Box<dyn Future<Output = Result<()>> + Send>>;
    type MockClose =
        fn(Arc<RwLock<WebSocketParts>>) -> std::pin::Pin<Box<dyn Future<Output = ()> + Send>>;

    type MockFutOk = std::pin::Pin<Box<dyn Future<Output = Result<()>> + Send>>;
    type MockFutMsg = std::pin::Pin<Box<dyn Future<Output = Result<Message>> + Send>>;
    type MockFutUnit = std::pin::Pin<Box<dyn Future<Output = ()> + Send>>;

    // ==================== 构造函数测试 ====================

    #[test]
    fn test_handler_wrapper_new_with_none_config() {
        let wrapper = HandlerWrapperWebSocket::<
            MockConnect,
            MockFutOk,
            MockSend,
            MockFutMsg,
            MockRecv,
            MockFutOk,
            MockClose,
            MockFutUnit,
        >::new(None);

        assert!(wrapper.config.is_none());
    }

    #[test]
    fn test_handler_wrapper_new_with_some_config() {
        let config = async_tungstenite::tungstenite::protocol::WebSocketConfig::default();
        let wrapper = HandlerWrapperWebSocket::<
            MockConnect,
            MockFutOk,
            MockSend,
            MockFutMsg,
            MockRecv,
            MockFutOk,
            MockClose,
            MockFutUnit,
        >::new(Some(config));

        assert!(wrapper.config.is_some());
    }

    // ==================== set_handler 测试 ====================

    #[test]
    fn test_handler_wrapper_set_handler() {
        let wrapper = HandlerWrapperWebSocket::<
            MockConnect,
            MockFutOk,
            MockSend,
            MockFutMsg,
            MockRecv,
            MockFutOk,
            MockClose,
            MockFutUnit,
        >::new(None);

        let handler = WebSocketHandler::new();
        let wrapper = wrapper.set_handler(handler);

        // 验证 set_handler 返回 self
        let _ = wrapper;
    }

    // ==================== Handler trait 测试 ====================

    #[tokio::test]
    async fn test_handler_wrapper_call_valid_websocket_request() {
        let wrapper = HandlerWrapperWebSocket::<
            MockConnect,
            MockFutOk,
            MockSend,
            MockFutMsg,
            MockRecv,
            MockFutOk,
            MockClose,
            MockFutUnit,
        >::new(None);

        let mut req = Request::empty();
        req.headers_mut()
            .insert("upgrade", HeaderValue::from_static("websocket"));
        req.headers_mut().insert(
            "sec-websocket-key",
            HeaderValue::from_static("dGhlIHNhbXBsZSBub25jZQ=="),
        );

        let result = wrapper.call(req).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_handler_wrapper_call_invalid_websocket_request() {
        let wrapper = HandlerWrapperWebSocket::<
            MockConnect,
            MockFutOk,
            MockSend,
            MockFutMsg,
            MockRecv,
            MockFutOk,
            MockClose,
            MockFutUnit,
        >::new(None);

        let req = Request::empty(); // 缺少必需的 WebSocket headers

        let result = wrapper.call(req).await;
        // 应该返回错误，因为这不是有效的 WebSocket 请求
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_handler_wrapper_call_with_config() {
        let config = async_tungstenite::tungstenite::protocol::WebSocketConfig::default();
        let wrapper = HandlerWrapperWebSocket::<
            MockConnect,
            MockFutOk,
            MockSend,
            MockFutMsg,
            MockRecv,
            MockFutOk,
            MockClose,
            MockFutUnit,
        >::new(Some(config));

        let mut req = Request::empty();
        req.headers_mut()
            .insert("upgrade", HeaderValue::from_static("websocket"));
        req.headers_mut().insert(
            "sec-websocket-key",
            HeaderValue::from_static("dGhlIHNhbXBsZSBub25jZQ=="),
        );

        let result = wrapper.call(req).await;
        assert!(result.is_ok());
    }

    // ==================== 类型验证测试 ====================

    #[test]
    fn test_handler_wrapper_send_sync() {
        // 验证 HandlerWrapperWebSocket 是 Send + Sync 的
        fn is_send_sync<T: Send + Sync>() {}

        is_send_sync::<
            HandlerWrapperWebSocket<
                MockConnect,
                MockFutOk,
                MockSend,
                MockFutMsg,
                MockRecv,
                MockFutOk,
                MockClose,
                MockFutUnit,
            >,
        >();
    }
}
