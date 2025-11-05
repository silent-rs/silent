use crate::ws::handler::websocket_handler;
use crate::ws::websocket::{WebSocket, WebSocketHandlerTrait};
use crate::ws::websocket_handler::WebSocketHandler;
use crate::ws::{Message, WebSocketParts, upgrade};
use crate::{Handler, Request, Response, Result};
use async_trait::async_trait;
use async_tungstenite::tungstenite::protocol;
use std::future::Future;
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio::sync::mpsc::UnboundedSender;
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
        tokio::task::spawn(async move {
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
        });
        Ok(res)
    }
}
