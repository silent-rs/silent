use crate::header::{HeaderMap, HeaderValue};
use crate::prelude::PathParam;
use crate::{Request, Result, SilentError};
use futures::channel::oneshot;
use http::Extensions;
#[cfg(feature = "server")]
use hyper::upgrade::Upgraded as HyperUpgraded;
#[cfg(feature = "server")]
use hyper_util::rt::TokioIo;
// server 路径在 hyper_service 内部完成 compat 适配
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

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

pub struct Upgraded<S> {
    head: WebSocketParts,
    upgrade: S,
}

impl<S> Upgraded<S> {
    pub(crate) fn into_parts(self) -> (WebSocketParts, S) {
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
pub struct AsyncUpgradeRx<S>(AsyncUpgradeInner<S>);

struct AsyncUpgradeInner<S>(Arc<Mutex<Option<oneshot::Receiver<S>>>>);

impl<S> Clone for AsyncUpgradeInner<S> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

impl<S> Clone for AsyncUpgradeRx<S> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

impl<S> AsyncUpgradeRx<S> {
    pub fn new(rx: oneshot::Receiver<S>) -> Self {
        Self(AsyncUpgradeInner(Arc::new(Mutex::new(Some(rx)))))
    }
    pub fn take(&self) -> Option<oneshot::Receiver<S>> {
        // 忽略锁错误，按 None 处理
        self.0.0.lock().ok().and_then(|mut g| g.take())
    }
}

// Server 侧默认的 Upgraded IO 类型（Hyper Upgraded -> TokioIo -> futures-io compat）
#[cfg(feature = "server")]
pub type ServerUpgradedIo = tokio_util::compat::Compat<TokioIo<HyperUpgraded>>;

// Server 侧取升级连接的简化接口，供现有代码无缝使用
#[cfg(feature = "server")]
pub(crate) async fn on(mut req: Request) -> Result<Upgraded<ServerUpgradedIo>> {
    let headers = req.headers().clone();
    let path_params = req.path_params().clone();
    let params = req.params().clone();
    let mut extensions = req.take_extensions();
    let rx = extensions
        .remove::<AsyncUpgradeRx<ServerUpgradedIo>>()
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

// 通用版本：非 server 运行时（如 wasm 宿主）可注入任意实现 futures-io 的连接类型

pub async fn on_generic<S>(mut req: Request) -> Result<Upgraded<S>>
where
    S: Send + 'static,
{
    let headers = req.headers().clone();
    let path_params = req.path_params().clone();
    let params = req.params().clone();
    let mut extensions = req.take_extensions();
    // 从扩展中获取注入的升级接收器
    let rx = extensions
        .remove::<AsyncUpgradeRx<S>>()
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
