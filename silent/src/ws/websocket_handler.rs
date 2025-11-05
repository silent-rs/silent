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

    #[test]
    fn handler_test() {
        let _handler = WebSocketHandler {
            on_connect: Some(Arc::new(|_, _| async { Ok(()) })),
            on_send: Some(Arc::new(|message, _| async { Ok(message) })),
            on_receive: Some(Arc::new(|_, _| async { Ok(()) })),
            on_close: Some(Arc::new(|_| async {})),
        };
    }
}
