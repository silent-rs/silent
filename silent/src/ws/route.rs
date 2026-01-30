use crate::prelude::{HandlerGetter, Message, Result, WebSocketParts};
use crate::route::Route;
use crate::ws::{HandlerWrapperWebSocket, WebSocketHandler};
use async_channel::Sender as UnboundedSender;
use async_lock::RwLock;
use async_tungstenite::tungstenite::protocol::WebSocketConfig;
use http::Method;
use std::future::Future;
use std::sync::Arc;

pub trait WSHandlerAppend<
    FnOnConnect,
    FnOnConnectFut,
    FnOnSend,
    FnOnSendFut,
    FnOnReceive,
    FnOnReceiveFut,
    FnOnClose,
    FnOnCloseFut,
>: HandlerGetter where
    FnOnConnect: Fn(Arc<RwLock<WebSocketParts>>, UnboundedSender<Message>) -> FnOnConnectFut
        + Send
        + Sync
        + 'static,
    FnOnConnectFut: Future<Output = Result<()>> + Send + Sync + 'static,
    FnOnSend: Fn(Message, Arc<RwLock<WebSocketParts>>) -> FnOnSendFut + Send + Sync + 'static,
    FnOnSendFut: Future<Output = Result<Message>> + Send + Sync + 'static,
    FnOnReceive: Fn(Message, Arc<RwLock<WebSocketParts>>) -> FnOnReceiveFut + Send + Sync + 'static,
    FnOnReceiveFut: Future<Output = Result<()>> + Send + Sync + 'static,
    FnOnClose: Fn(Arc<RwLock<WebSocketParts>>) -> FnOnCloseFut + Send + Sync + 'static,
    FnOnCloseFut: Future<Output = ()> + Send + Sync + 'static,
{
    fn ws(
        self,
        config: Option<WebSocketConfig>,
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
    ) -> Self;
    fn ws_handler_append(
        &mut self,
        handler: HandlerWrapperWebSocket<
            FnOnConnect,
            FnOnConnectFut,
            FnOnSend,
            FnOnSendFut,
            FnOnReceive,
            FnOnReceiveFut,
            FnOnClose,
            FnOnCloseFut,
        >,
    ) {
        let handler = Arc::new(handler);
        self.get_handler_mut().insert(Method::GET, handler);
    }
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
    WSHandlerAppend<
        FnOnConnect,
        FnOnConnectFut,
        FnOnSend,
        FnOnSendFut,
        FnOnReceive,
        FnOnReceiveFut,
        FnOnClose,
        FnOnCloseFut,
    > for Route
where
    FnOnConnect: Fn(Arc<RwLock<WebSocketParts>>, UnboundedSender<Message>) -> FnOnConnectFut
        + Send
        + Sync
        + 'static,
    FnOnConnectFut: Future<Output = Result<()>> + Send + Sync + 'static,
    FnOnSend: Fn(Message, Arc<RwLock<WebSocketParts>>) -> FnOnSendFut + Send + Sync + 'static,
    FnOnSendFut: Future<Output = Result<Message>> + Send + Sync + 'static,
    FnOnReceive: Fn(Message, Arc<RwLock<WebSocketParts>>) -> FnOnReceiveFut + Send + Sync + 'static,
    FnOnReceiveFut: Future<Output = Result<()>> + Send + Sync + 'static,
    FnOnClose: Fn(Arc<RwLock<WebSocketParts>>) -> FnOnCloseFut + Send + Sync + 'static,
    FnOnCloseFut: Future<Output = ()> + Send + Sync + 'static,
{
    fn ws(
        mut self,
        config: Option<WebSocketConfig>,
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
        let handler = HandlerWrapperWebSocket::new(config).set_handler(handler);
        self.ws_handler_append(handler);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ws::websocket_handler::WebSocketHandler;

    // 定义测试类型别名
    type MockConnect = fn(
        Arc<RwLock<WebSocketParts>>,
        UnboundedSender<Message>,
    ) -> std::pin::Pin<Box<dyn Future<Output = Result<()>> + Send + Sync>>;
    type MockSend = fn(
        Message,
        Arc<RwLock<WebSocketParts>>,
    )
        -> std::pin::Pin<Box<dyn Future<Output = Result<Message>> + Send + Sync>>;
    type MockRecv = fn(
        Message,
        Arc<RwLock<WebSocketParts>>,
    ) -> std::pin::Pin<Box<dyn Future<Output = Result<()>> + Send + Sync>>;
    type MockClose = fn(
        Arc<RwLock<WebSocketParts>>,
    ) -> std::pin::Pin<Box<dyn Future<Output = ()> + Send + Sync>>;

    // ==================== ws() 方法测试 ====================

    #[test]
    fn test_route_ws_with_none_config() {
        let handler = WebSocketHandler::<
            MockConnect,
            std::pin::Pin<Box<dyn Future<Output = Result<()>> + Send + Sync>>,
            MockSend,
            std::pin::Pin<Box<dyn Future<Output = Result<Message>> + Send + Sync>>,
            MockRecv,
            std::pin::Pin<Box<dyn Future<Output = Result<()>> + Send + Sync>>,
            MockClose,
            std::pin::Pin<Box<dyn Future<Output = ()> + Send + Sync>>,
        >::new();
        let route = Route::new("websocket").ws(None, handler);

        // 验证路由被创建
        assert_eq!(route.path, "websocket");
    }

    #[test]
    fn test_route_ws_with_some_config() {
        let config = WebSocketConfig::default();
        let handler = WebSocketHandler::<
            MockConnect,
            std::pin::Pin<Box<dyn Future<Output = Result<()>> + Send + Sync>>,
            MockSend,
            std::pin::Pin<Box<dyn Future<Output = Result<Message>> + Send + Sync>>,
            MockRecv,
            std::pin::Pin<Box<dyn Future<Output = Result<()>> + Send + Sync>>,
            MockClose,
            std::pin::Pin<Box<dyn Future<Output = ()> + Send + Sync>>,
        >::new();
        let route = Route::new("websocket").ws(Some(config), handler);

        // 验证路由被创建
        assert_eq!(route.path, "websocket");
    }

    #[test]
    fn test_route_ws_with_nested_path() {
        let handler = WebSocketHandler::<
            MockConnect,
            std::pin::Pin<Box<dyn Future<Output = Result<()>> + Send + Sync>>,
            MockSend,
            std::pin::Pin<Box<dyn Future<Output = Result<Message>> + Send + Sync>>,
            MockRecv,
            std::pin::Pin<Box<dyn Future<Output = Result<()>> + Send + Sync>>,
            MockClose,
            std::pin::Pin<Box<dyn Future<Output = ()> + Send + Sync>>,
        >::new();
        let route = Route::new("api/websocket").ws(None, handler);

        // 验证嵌套路由被创建
        assert_eq!(route.path, "api");
        assert!(!route.children.is_empty());
    }

    #[test]
    fn test_route_ws_chain() {
        let handler1 = WebSocketHandler::<
            MockConnect,
            std::pin::Pin<Box<dyn Future<Output = Result<()>> + Send + Sync>>,
            MockSend,
            std::pin::Pin<Box<dyn Future<Output = Result<Message>> + Send + Sync>>,
            MockRecv,
            std::pin::Pin<Box<dyn Future<Output = Result<()>> + Send + Sync>>,
            MockClose,
            std::pin::Pin<Box<dyn Future<Output = ()> + Send + Sync>>,
        >::new();
        let handler2 = WebSocketHandler::<
            MockConnect,
            std::pin::Pin<Box<dyn Future<Output = Result<()>> + Send + Sync>>,
            MockSend,
            std::pin::Pin<Box<dyn Future<Output = Result<Message>> + Send + Sync>>,
            MockRecv,
            std::pin::Pin<Box<dyn Future<Output = Result<()>> + Send + Sync>>,
            MockClose,
            std::pin::Pin<Box<dyn Future<Output = ()> + Send + Sync>>,
        >::new();
        let route = Route::new("websocket")
            .ws(None, handler1)
            .ws(None, handler2);

        // 验证链式调用返回 Route
        assert_eq!(route.path, "websocket");
    }

    // ==================== 集成测试 ====================

    #[test]
    fn test_route_ws_integration() {
        let handler = WebSocketHandler::<
            MockConnect,
            std::pin::Pin<Box<dyn Future<Output = Result<()>> + Send + Sync>>,
            MockSend,
            std::pin::Pin<Box<dyn Future<Output = Result<Message>> + Send + Sync>>,
            MockRecv,
            std::pin::Pin<Box<dyn Future<Output = Result<()>> + Send + Sync>>,
            MockClose,
            std::pin::Pin<Box<dyn Future<Output = ()> + Send + Sync>>,
        >::new();
        let route = Route::new("chat").ws(None, handler);

        // 验证路由结构
        assert_eq!(route.path, "chat");
        assert!(route.handler.contains_key(&Method::GET));
    }

    #[test]
    fn test_route_ws_multiple_handlers() {
        let handler1 = WebSocketHandler::<
            MockConnect,
            std::pin::Pin<Box<dyn Future<Output = Result<()>> + Send + Sync>>,
            MockSend,
            std::pin::Pin<Box<dyn Future<Output = Result<Message>> + Send + Sync>>,
            MockRecv,
            std::pin::Pin<Box<dyn Future<Output = Result<()>> + Send + Sync>>,
            MockClose,
            std::pin::Pin<Box<dyn Future<Output = ()> + Send + Sync>>,
        >::new();
        let handler2 = WebSocketHandler::<
            MockConnect,
            std::pin::Pin<Box<dyn Future<Output = Result<()>> + Send + Sync>>,
            MockSend,
            std::pin::Pin<Box<dyn Future<Output = Result<Message>> + Send + Sync>>,
            MockRecv,
            std::pin::Pin<Box<dyn Future<Output = Result<()>> + Send + Sync>>,
            MockClose,
            std::pin::Pin<Box<dyn Future<Output = ()> + Send + Sync>>,
        >::new();

        let route = Route::new("ws1").ws(None, handler1).ws(None, handler2);

        // 后添加的处理器会覆盖前面的
        assert_eq!(route.path, "ws1");
        assert!(route.handler.contains_key(&Method::GET));
    }

    #[test]
    fn test_route_ws_empty_path() {
        let handler = WebSocketHandler::<
            MockConnect,
            std::pin::Pin<Box<dyn Future<Output = Result<()>> + Send + Sync>>,
            MockSend,
            std::pin::Pin<Box<dyn Future<Output = Result<Message>> + Send + Sync>>,
            MockRecv,
            std::pin::Pin<Box<dyn Future<Output = Result<()>> + Send + Sync>>,
            MockClose,
            std::pin::Pin<Box<dyn Future<Output = ()> + Send + Sync>>,
        >::new();
        let route = Route::new("").ws(None, handler);

        // 空路径应该创建一个默认路由
        assert_eq!(route.path, "");
    }

    #[test]
    fn test_route_ws_root_path() {
        let handler = WebSocketHandler::<
            MockConnect,
            std::pin::Pin<Box<dyn Future<Output = Result<()>> + Send + Sync>>,
            MockSend,
            std::pin::Pin<Box<dyn Future<Output = Result<Message>> + Send + Sync>>,
            MockRecv,
            std::pin::Pin<Box<dyn Future<Output = Result<()>> + Send + Sync>>,
            MockClose,
            std::pin::Pin<Box<dyn Future<Output = ()> + Send + Sync>>,
        >::new();
        let route = Route::new_root().ws(None, handler);

        // 根路由应该有正确的路径
        assert_eq!(route.path, "");
    }

    // ==================== 类型验证测试 ====================

    #[test]
    fn test_ws_handler_append_trait_bound() {
        // 验证 WSHandlerAppend trait 可以用于 Route
        fn accepts_ws_append<T>(_: T)
        where
            T: WSHandlerAppend<
                    MockConnect,
                    std::pin::Pin<Box<dyn Future<Output = Result<()>> + Send + Sync>>,
                    MockSend,
                    std::pin::Pin<Box<dyn Future<Output = Result<Message>> + Send + Sync>>,
                    MockRecv,
                    std::pin::Pin<Box<dyn Future<Output = Result<()>> + Send + Sync>>,
                    MockClose,
                    std::pin::Pin<Box<dyn Future<Output = ()> + Send + Sync>>,
                >,
        {
        }

        let route = Route::new("test");
        accepts_ws_append(route);
    }
}
