use crate::Result;
use crate::ws::Message;
use crate::ws::upgrade::WebSocketParts;
use async_channel::Sender as UnboundedSender;
use async_lock::RwLock;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

pub type FnOnConnect = dyn Fn(
        Arc<RwLock<WebSocketParts>>,
        UnboundedSender<Message>,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send + Sync + 'static>>
    + Send
    + Sync
    + 'static;
pub type FnOnSendFut = dyn Future<Output = Result<Message>> + Send + Sync + 'static;
pub type FnOnNoneResultFut = dyn Future<Output = Result<()>> + Send + Sync + 'static;
pub type FnOnSend = dyn Fn(
        Message,
        Arc<RwLock<WebSocketParts>>,
    ) -> Pin<Box<dyn Future<Output = Result<Message>> + Send + Sync + 'static>>
    + Send
    + Sync
    + 'static;
pub type FnOnReceive = dyn Fn(
        Message,
        Arc<RwLock<WebSocketParts>>,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send + Sync + 'static>>
    + Send
    + Sync
    + 'static;
pub type FnOnClose = dyn Fn(
        Arc<RwLock<WebSocketParts>>,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send + Sync + 'static>>
    + Send
    + Sync
    + 'static;
