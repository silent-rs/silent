use crate::core::connection::Connection;
use crate::header::{HeaderMap, HeaderValue};
use crate::prelude::PathParam;
use crate::{Request, Result, SilentError};
use futures::channel::oneshot;
use http::Extensions;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

#[allow(dead_code)]
#[derive(Debug)]
pub struct WebSocketParts {
    path_params: HashMap<String, PathParam>,
    params: HashMap<String, String>,
    headers: HeaderMap<HeaderValue>,
    extensions: Extensions,
}

impl WebSocketParts {
    #[inline]
    pub fn path_params(&self) -> &HashMap<String, PathParam> {
        &self.path_params
    }

    #[inline]
    pub fn params_mut(&mut self) -> &mut HashMap<String, String> {
        &mut self.params
    }

    #[inline]
    pub fn params(&self) -> &HashMap<String, String> {
        &self.params
    }

    #[inline]
    pub fn headers(&self) -> &HeaderMap<HeaderValue> {
        &self.headers
    }

    #[inline]
    pub fn extensions(&self) -> &Extensions {
        &self.extensions
    }

    #[inline]
    pub fn extensions_mut(&mut self) -> &mut Extensions {
        &mut self.extensions
    }
}

pub enum UpgradedIo {
    Hyper(upgrade::Upgraded),
    Futures(Box<dyn Connection + Send>),
}

pub(crate) struct Upgraded {
    head: WebSocketParts,
    upgrade: UpgradedIo,
}

#[allow(dead_code)]
impl Upgraded {
    pub(crate) fn into_parts(self) -> (WebSocketParts, UpgradedIo) {
        (self.head, self.upgrade)
    }

    #[inline]
    pub fn path_params(&self) -> &HashMap<String, PathParam> {
        &self.head.path_params
    }

    #[inline]
    pub fn params(&self) -> &HashMap<String, String> {
        &self.head.params
    }

    #[inline]
    pub fn headers(&self) -> &HeaderMap<HeaderValue> {
        &self.head.headers
    }

    #[inline]
    pub fn extensions(&self) -> &Extensions {
        &self.head.extensions
    }

    #[inline]
    pub fn extensions_mut(&mut self) -> &mut Extensions {
        &mut self.head.extensions
    }
}

pub(crate) async fn on(mut req: Request) -> Result<Upgraded> {
    let headers = req.headers().clone();
    let path_params = req.path_params().clone();
    let params = req.params().clone();
    let mut extensions = req.take_extensions();
    // 优先使用 AsyncIoTransport 注入的升级通道
    if let Some(rx) = extensions.remove::<AsyncUpgradeRx>() {
        let rx = rx
            .take()
            .ok_or_else(|| SilentError::WsError("Upgrade channel missing".into()))?;
        let stream = rx.await.map_err(|e| SilentError::WsError(e.to_string()))?;
        return Ok(Upgraded {
            head: WebSocketParts {
                path_params,
                params,
                headers,
                extensions,
            },
            upgrade: UpgradedIo::Futures(stream),
        });
    }
    Err(SilentError::WsError("No upgrade channel available".into()))
}

// AsyncIoTransport 注入的升级接收器类型
#[derive(Clone)]
pub struct AsyncUpgradeRx(AsyncUpgradeInner);

#[allow(clippy::type_complexity)]
#[derive(Clone)]
struct AsyncUpgradeInner(Arc<Mutex<Option<oneshot::Receiver<Box<dyn Connection + Send>>>>>);

impl AsyncUpgradeRx {
    pub fn new(rx: oneshot::Receiver<Box<dyn Connection + Send>>) -> Self {
        Self(AsyncUpgradeInner(Arc::new(Mutex::new(Some(rx)))))
    }
    pub fn take(&self) -> Option<oneshot::Receiver<Box<dyn Connection + Send>>> {
        self.0.0.lock().ok().and_then(|mut g| g.take())
    }
}
