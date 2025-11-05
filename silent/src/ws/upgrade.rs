use crate::header::{HeaderMap, HeaderValue};
use crate::prelude::PathParam;
use crate::{Request, Result, SilentError};
use futures::channel::oneshot;
use http::Extensions;
use hyper::upgrade::Upgraded as HyperUpgraded;
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

pub(crate) struct Upgraded {
    head: WebSocketParts,
    upgrade: HyperUpgraded,
}

#[allow(dead_code)]
impl Upgraded {
    pub(crate) fn into_parts(self) -> (WebSocketParts, HyperUpgraded) {
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

// 注入式升级接收器：服务器侧在收到可升级请求时，创建 oneshot::Receiver 并注入到 Request.extensions 中。
#[derive(Clone)]
pub struct AsyncUpgradeRx(AsyncUpgradeInner);

#[derive(Clone)]
struct AsyncUpgradeInner(Arc<Mutex<Option<oneshot::Receiver<HyperUpgraded>>>>);

impl AsyncUpgradeRx {
    pub fn new(rx: oneshot::Receiver<HyperUpgraded>) -> Self {
        Self(AsyncUpgradeInner(Arc::new(Mutex::new(Some(rx)))))
    }
    pub fn take(&self) -> Option<oneshot::Receiver<HyperUpgraded>> {
        // 忽略锁错误，按 None 处理
        self.0.0.lock().ok().and_then(|mut g| g.take())
    }
}

pub(crate) async fn on(mut req: Request) -> Result<Upgraded> {
    let headers = req.headers().clone();
    let path_params = req.path_params().clone();
    let params = req.params().clone();
    let mut extensions = req.take_extensions();
    // 从扩展中获取注入的升级接收器
    let rx = extensions
        .remove::<AsyncUpgradeRx>()
        .ok_or_else(|| SilentError::WsError("No upgrade channel available".to_string()))?;
    let rx = rx
        .take()
        .ok_or_else(|| SilentError::WsError("Upgrade channel missing".to_string()))?;
    let upgrade = rx
        .await
        .map_err(|e| SilentError::WsError(format!("upgrade await error: {e}")))?;
    Ok(Upgraded {
        head: WebSocketParts {
            path_params,
            params,
            headers,
            extensions,
        },
        upgrade,
    })
}
