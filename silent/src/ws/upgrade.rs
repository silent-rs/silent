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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::header::HeaderValue;
    use crate::prelude::PathParam;
    use std::collections::HashMap;

    // ==================== WebSocketParts 测试 ====================

    #[test]
    fn test_websocket_parts_new() {
        let parts = WebSocketParts {
            path_params: HashMap::new(),
            params: HashMap::new(),
            headers: HeaderMap::new(),
            extensions: Extensions::default(),
        };

        assert_eq!(parts.path_params().len(), 0);
        assert_eq!(parts.params().len(), 0);
        assert_eq!(parts.headers().len(), 0);
    }

    #[test]
    fn test_websocket_parts_path_params() {
        let mut path_params = HashMap::new();
        path_params.insert("id".to_string(), PathParam::from("123".to_string()));
        path_params.insert("name".to_string(), PathParam::from("test".to_string()));

        let parts = WebSocketParts {
            path_params,
            params: HashMap::new(),
            headers: HeaderMap::new(),
            extensions: Extensions::default(),
        };

        assert_eq!(parts.path_params().len(), 2);
        assert!(parts.path_params().contains_key("id"));
        assert!(parts.path_params().contains_key("name"));
    }

    #[test]
    fn test_websocket_parts_params() {
        let mut params = HashMap::new();
        params.insert("key1".to_string(), "value1".to_string());
        params.insert("key2".to_string(), "value2".to_string());

        let parts = WebSocketParts {
            path_params: HashMap::new(),
            params,
            headers: HeaderMap::new(),
            extensions: Extensions::default(),
        };

        assert_eq!(parts.params().len(), 2);
        assert_eq!(parts.params().get("key1"), Some(&"value1".to_string()));
    }

    #[test]
    fn test_websocket_parts_params_mut() {
        let mut parts = WebSocketParts {
            path_params: HashMap::new(),
            params: HashMap::new(),
            headers: HeaderMap::new(),
            extensions: Extensions::default(),
        };

        parts
            .params_mut()
            .insert("key".to_string(), "value".to_string());
        assert_eq!(parts.params().len(), 1);
        assert_eq!(parts.params().get("key"), Some(&"value".to_string()));
    }

    #[test]
    fn test_websocket_parts_headers() {
        let mut headers = HeaderMap::new();
        headers.insert("content-type", HeaderValue::from_static("text/plain"));
        headers.insert("authorization", HeaderValue::from_static("Bearer token"));

        let parts = WebSocketParts {
            path_params: HashMap::new(),
            params: HashMap::new(),
            headers,
            extensions: Extensions::default(),
        };

        assert_eq!(parts.headers().len(), 2);
        assert!(parts.headers().get("content-type").is_some());
        assert!(parts.headers().get("authorization").is_some());
    }

    #[test]
    fn test_websocket_parts_extensions() {
        let mut extensions = Extensions::default();
        extensions.insert("test_data");
        extensions.insert(42i32);

        let parts = WebSocketParts {
            path_params: HashMap::new(),
            params: HashMap::new(),
            headers: HeaderMap::new(),
            extensions,
        };

        assert_eq!(parts.extensions().get::<&str>(), Some(&"test_data"));
        assert_eq!(parts.extensions().get::<i32>(), Some(&42));
    }

    #[test]
    fn test_websocket_parts_extensions_mut() {
        let mut parts = WebSocketParts {
            path_params: HashMap::new(),
            params: HashMap::new(),
            headers: HeaderMap::new(),
            extensions: Extensions::default(),
        };

        parts.extensions_mut().insert("test_data");
        assert_eq!(parts.extensions().get::<&str>(), Some(&"test_data"));
    }

    // ==================== AsyncUpgradeRx 测试 ====================

    #[test]
    fn test_async_upgrade_rx_new() {
        let (_tx, rx) = oneshot::channel::<i32>();
        let async_rx = AsyncUpgradeRx::new(rx);

        // 验证创建成功
        let taken = async_rx.take();
        assert!(taken.is_some());
    }

    #[test]
    fn test_async_upgrade_rx_take() {
        let (_tx, rx) = oneshot::channel::<i32>();
        let async_rx = AsyncUpgradeRx::new(rx);

        // 第一次 take 应该成功
        let taken1 = async_rx.take();
        assert!(taken1.is_some());

        // 第二次 take 应该返回 None（已经被取走）
        let taken2 = async_rx.take();
        assert!(taken2.is_none());
    }

    #[test]
    fn test_async_upgrade_rx_clone() {
        let (_tx, rx) = oneshot::channel::<i32>();
        let async_rx1 = AsyncUpgradeRx::new(rx);
        let async_rx2 = async_rx1.clone();

        // 两个 clone 都应该能够访问同一个 channel
        let taken1 = async_rx1.take();
        assert!(taken1.is_some());

        // 因为它们共享同一个 Arc<Mutex<Option<Receiver>>>
        // 当一个被 take 后，另一个应该得到 None
        let taken2 = async_rx2.take();
        assert!(taken2.is_none());
    }

    #[test]
    fn test_async_upgrade_rx_multiple_clones() {
        let (_tx, rx) = oneshot::channel::<i32>();
        let rx1 = AsyncUpgradeRx::new(rx);
        let rx2 = rx1.clone();
        let rx3 = rx2.clone();

        // 所有 clone 都应该指向同一个内部状态
        let _taken = rx1.take();
        assert!(rx2.take().is_none());
        assert!(rx3.take().is_none());
    }

    // ==================== Upgraded 测试 ====================

    #[test]
    fn test_upgraded_into_parts() {
        // 创建一个简单的类型 S 作为测试
        let test_value = 42i32;

        let parts = WebSocketParts {
            path_params: HashMap::new(),
            params: HashMap::new(),
            headers: HeaderMap::new(),
            extensions: Extensions::default(),
        };

        let upgraded = Upgraded {
            head: parts,
            upgrade: test_value,
        };

        let (returned_parts, returned_value) = upgraded.into_parts();
        assert_eq!(returned_value, 42);
        assert_eq!(returned_parts.path_params().len(), 0);
    }

    #[test]
    fn test_upgraded_path_params() {
        let mut path_params = HashMap::new();
        path_params.insert("id".to_string(), PathParam::from("123".to_string()));

        let parts = WebSocketParts {
            path_params,
            params: HashMap::new(),
            headers: HeaderMap::new(),
            extensions: Extensions::default(),
        };

        let upgraded = Upgraded {
            head: parts,
            upgrade: 42i32,
        };

        assert!(upgraded.path_params().contains_key("id"));
    }

    #[test]
    fn test_upgraded_params() {
        let mut params = HashMap::new();
        params.insert("key".to_string(), "value".to_string());

        let parts = WebSocketParts {
            path_params: HashMap::new(),
            params,
            headers: HeaderMap::new(),
            extensions: Extensions::default(),
        };

        let upgraded = Upgraded {
            head: parts,
            upgrade: 42i32,
        };

        assert_eq!(upgraded.params().get("key"), Some(&"value".to_string()));
    }

    #[test]
    fn test_upgraded_headers() {
        let mut headers = HeaderMap::new();
        headers.insert("x-test", HeaderValue::from_static("test_value"));

        let parts = WebSocketParts {
            path_params: HashMap::new(),
            params: HashMap::new(),
            headers,
            extensions: Extensions::default(),
        };

        let upgraded = Upgraded {
            head: parts,
            upgrade: 42i32,
        };

        assert!(upgraded.headers().get("x-test").is_some());
    }

    #[test]
    fn test_upgraded_extensions() {
        let mut extensions = Extensions::default();
        extensions.insert("test_extension");

        let parts = WebSocketParts {
            path_params: HashMap::new(),
            params: HashMap::new(),
            headers: HeaderMap::new(),
            extensions,
        };

        let upgraded = Upgraded {
            head: parts,
            upgrade: 42i32,
        };

        assert_eq!(upgraded.extensions().get::<&str>(), Some(&"test_extension"));
    }

    #[test]
    fn test_upgraded_extensions_mut() {
        let parts = WebSocketParts {
            path_params: HashMap::new(),
            params: HashMap::new(),
            headers: HeaderMap::new(),
            extensions: Extensions::default(),
        };

        let mut upgraded = Upgraded {
            head: parts,
            upgrade: 42i32,
        };

        upgraded.extensions_mut().insert("new_extension");
        assert_eq!(upgraded.extensions().get::<&str>(), Some(&"new_extension"));
    }

    // ==================== 组合测试 ====================

    #[test]
    fn test_websocket_parts_with_all_fields() {
        let mut path_params = HashMap::new();
        path_params.insert("id".to_string(), PathParam::from("123".to_string()));

        let mut params = HashMap::new();
        params.insert("key".to_string(), "value".to_string());

        let mut headers = HeaderMap::new();
        headers.insert("x-test", HeaderValue::from_static("test"));

        let mut extensions = Extensions::default();
        extensions.insert(42i32);

        let parts = WebSocketParts {
            path_params,
            params,
            headers,
            extensions,
        };

        assert_eq!(parts.path_params().len(), 1);
        assert_eq!(parts.params().len(), 1);
        assert_eq!(parts.headers().len(), 1);
        assert_eq!(parts.extensions().get::<i32>(), Some(&42));
    }

    #[test]
    fn test_upgraded_complete_lifecycle() {
        // 创建包含所有字段的 WebSocketParts
        let mut path_params = HashMap::new();
        path_params.insert("id".to_string(), PathParam::from("abc".to_string()));

        let mut params = HashMap::new();
        params.insert("user".to_string(), "test".to_string());

        let mut headers = HeaderMap::new();
        headers.insert("auth", HeaderValue::from_static("token123"));

        let mut extensions = Extensions::default();
        extensions.insert("session_id");

        let parts = WebSocketParts {
            path_params,
            params,
            headers,
            extensions,
        };

        // 创建 Upgraded
        let upgraded = Upgraded {
            head: parts,
            upgrade: "upgrade_value",
        };

        // 验证所有字段都可以访问
        assert!(upgraded.path_params().contains_key("id"));
        assert_eq!(upgraded.params().get("user"), Some(&"test".to_string()));
        assert!(upgraded.headers().get("auth").is_some());
        assert_eq!(upgraded.extensions().get::<&str>(), Some(&"session_id"));

        // into_parts 测试
        let (returned_parts, returned_value) = upgraded.into_parts();
        assert_eq!(returned_value, "upgrade_value");
        assert!(returned_parts.path_params().contains_key("id"));
    }

    // ==================== 边界条件测试 ====================

    #[test]
    fn test_websocket_parts_empty() {
        let parts = WebSocketParts {
            path_params: HashMap::new(),
            params: HashMap::new(),
            headers: HeaderMap::new(),
            extensions: Extensions::default(),
        };

        assert!(parts.path_params().is_empty());
        assert!(parts.params().is_empty());
        assert!(parts.headers().is_empty());
    }

    #[test]
    fn test_async_upgrade_rx_empty_after_take() {
        let (_tx, rx) = oneshot::channel::<i32>();
        let async_rx = AsyncUpgradeRx::new(rx);

        let _ = async_rx.take();
        assert!(async_rx.take().is_none());
    }

    #[test]
    fn test_websocket_parts_large_params() {
        let mut params = HashMap::new();
        for i in 0..100 {
            params.insert(format!("key{}", i), format!("value{}", i));
        }

        let parts = WebSocketParts {
            path_params: HashMap::new(),
            params,
            headers: HeaderMap::new(),
            extensions: Extensions::default(),
        };

        assert_eq!(parts.params().len(), 100);
    }

    #[test]
    fn test_websocket_parts_multiple_extensions() {
        let mut extensions = Extensions::default();
        extensions.insert("string_data");
        extensions.insert(42i32);
        extensions.insert(vec![1u8, 2, 3]);

        let parts = WebSocketParts {
            path_params: HashMap::new(),
            params: HashMap::new(),
            headers: HeaderMap::new(),
            extensions,
        };

        assert_eq!(parts.extensions().get::<&str>(), Some(&"string_data"));
        assert_eq!(parts.extensions().get::<i32>(), Some(&42));
        assert_eq!(parts.extensions().get::<Vec<u8>>(), Some(&vec![1, 2, 3]));
    }
}
