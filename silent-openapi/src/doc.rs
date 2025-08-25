use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use http::Method;
use once_cell::sync::Lazy;

use silent::prelude::{HandlerGetter, Route};
use silent::{
    Handler, HandlerWrapper, Request as SilentRequest, Response as SilentResponse,
    Result as SilentResult,
};

/// 用于标注接口文档的元信息
#[derive(Clone, Debug)]
pub struct DocMeta {
    pub summary: Option<String>,
    pub description: Option<String>,
}

static DOC_REGISTRY: Lazy<Mutex<HashMap<usize, DocMeta>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

pub fn register_doc_by_ptr(ptr: usize, summary: Option<&str>, description: Option<&str>) {
    let mut map = DOC_REGISTRY.lock().expect("doc registry poisoned");
    map.insert(
        ptr,
        DocMeta {
            summary: summary.map(|s| s.to_string()),
            description: description.map(|s| s.to_string()),
        },
    );
}

pub(crate) fn lookup_doc_by_handler_ptr(ptr: usize) -> Option<DocMeta> {
    DOC_REGISTRY.lock().ok().and_then(|m| m.get(&ptr).cloned())
}

/// 路由文档标注扩展：在完成 handler 挂载后，追加文档说明
pub trait RouteDocMarkExt {
    fn doc(self, method: Method, summary: &str, description: &str) -> Self;
}

/// 便捷构造：将基于 Request 的处理函数包装为 `Arc<dyn Handler>` 并注册文档
pub fn handler_with_doc<F, Fut, T>(f: F, summary: &str, description: &str) -> Arc<dyn Handler>
where
    F: Fn(SilentRequest) -> Fut + Send + Sync + 'static,
    Fut: core::future::Future<Output = SilentResult<T>> + Send + 'static,
    T: Into<SilentResponse> + Send + 'static,
{
    let handler = Arc::new(HandlerWrapper::new(f));
    let ptr = Arc::as_ptr(&handler) as *const () as usize;
    register_doc_by_ptr(ptr, Some(summary), Some(description));
    handler
}

impl RouteDocMarkExt for Route {
    fn doc(self, method: Method, summary: &str, description: &str) -> Self {
        if let Some(handler) = self.handler.get(&method).cloned() {
            let ptr = Arc::as_ptr(&handler) as *const () as usize;
            register_doc_by_ptr(ptr, Some(summary), Some(description));
        }
        self
    }
}

/// 便捷追加：同时挂载处理器并标注文档
pub trait RouteDocAppendExt {
    fn get_with_doc(self, handler: Arc<dyn Handler>, summary: &str, description: &str) -> Self;
    fn post_with_doc(self, handler: Arc<dyn Handler>, summary: &str, description: &str) -> Self;
    fn put_with_doc(self, handler: Arc<dyn Handler>, summary: &str, description: &str) -> Self;
    fn delete_with_doc(self, handler: Arc<dyn Handler>, summary: &str, description: &str) -> Self;
    fn patch_with_doc(self, handler: Arc<dyn Handler>, summary: &str, description: &str) -> Self;
    fn options_with_doc(self, handler: Arc<dyn Handler>, summary: &str, description: &str) -> Self;
}

impl RouteDocAppendExt for Route {
    fn get_with_doc(self, handler: Arc<dyn Handler>, summary: &str, description: &str) -> Self {
        let ptr = Arc::as_ptr(&handler) as *const () as usize;
        register_doc_by_ptr(ptr, Some(summary), Some(description));
        <Route as HandlerGetter>::handler(self, Method::GET, handler)
    }

    fn post_with_doc(self, handler: Arc<dyn Handler>, summary: &str, description: &str) -> Self {
        let ptr = Arc::as_ptr(&handler) as *const () as usize;
        register_doc_by_ptr(ptr, Some(summary), Some(description));
        <Route as HandlerGetter>::handler(self, Method::POST, handler)
    }

    fn put_with_doc(self, handler: Arc<dyn Handler>, summary: &str, description: &str) -> Self {
        let ptr = Arc::as_ptr(&handler) as *const () as usize;
        register_doc_by_ptr(ptr, Some(summary), Some(description));
        <Route as HandlerGetter>::handler(self, Method::PUT, handler)
    }

    fn delete_with_doc(self, handler: Arc<dyn Handler>, summary: &str, description: &str) -> Self {
        let ptr = Arc::as_ptr(&handler) as *const () as usize;
        register_doc_by_ptr(ptr, Some(summary), Some(description));
        <Route as HandlerGetter>::handler(self, Method::DELETE, handler)
    }

    fn patch_with_doc(self, handler: Arc<dyn Handler>, summary: &str, description: &str) -> Self {
        let ptr = Arc::as_ptr(&handler) as *const () as usize;
        register_doc_by_ptr(ptr, Some(summary), Some(description));
        <Route as HandlerGetter>::handler(self, Method::PATCH, handler)
    }

    fn options_with_doc(self, handler: Arc<dyn Handler>, summary: &str, description: &str) -> Self {
        let ptr = Arc::as_ptr(&handler) as *const () as usize;
        register_doc_by_ptr(ptr, Some(summary), Some(description));
        <Route as HandlerGetter>::handler(self, Method::OPTIONS, handler)
    }
}
