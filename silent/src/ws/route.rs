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
