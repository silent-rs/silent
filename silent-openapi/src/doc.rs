use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use http::Method;
use once_cell::sync::Lazy;

use silent::prelude::{HandlerGetter, Route};
use silent::{
    Handler, HandlerWrapper, Request as SilentRequest, Response as SilentResponse,
    Result as SilentResult,
};
use utoipa::openapi::{Components, ComponentsBuilder, OpenApi};

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

/// 响应类型元信息
#[derive(Clone, Debug)]
pub enum ResponseMeta {
    TextPlain,
    Json { type_name: &'static str },
}

static RESPONSE_REGISTRY: Lazy<Mutex<HashMap<usize, ResponseMeta>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

pub fn register_response_by_ptr(ptr: usize, meta: ResponseMeta) {
    let mut map = RESPONSE_REGISTRY
        .lock()
        .expect("response registry poisoned");
    map.insert(ptr, meta);
}

pub(crate) fn lookup_response_by_handler_ptr(ptr: usize) -> Option<ResponseMeta> {
    RESPONSE_REGISTRY
        .lock()
        .ok()
        .and_then(|m| m.get(&ptr).cloned())
}

pub fn list_registered_json_types() -> Vec<&'static str> {
    let map = RESPONSE_REGISTRY.lock().ok();
    let mut out = Vec::new();
    if let Some(map) = map {
        for meta in map.values() {
            if let ResponseMeta::Json { type_name } = meta
                && !out.contains(type_name)
            {
                out.push(*type_name);
            }
        }
    }
    out
}

// ====== 请求元信息注册 ======

/// 请求参数/请求体元信息
#[derive(Clone, Debug)]
pub enum RequestMeta {
    /// JSON 请求体（对应 Json<T> 提取器）
    JsonBody { type_name: &'static str },
    /// 表单请求体（对应 Form<T> 提取器）
    FormBody { type_name: &'static str },
    /// 查询参数（对应 Query<T> 提取器）
    QueryParams { type_name: &'static str },
}

static REQUEST_REGISTRY: Lazy<Mutex<HashMap<usize, Vec<RequestMeta>>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

pub fn register_request_by_ptr(ptr: usize, meta: RequestMeta) {
    let mut map = REQUEST_REGISTRY.lock().expect("request registry poisoned");
    map.entry(ptr).or_default().push(meta);
}

pub(crate) fn lookup_request_by_handler_ptr(ptr: usize) -> Option<Vec<RequestMeta>> {
    REQUEST_REGISTRY
        .lock()
        .ok()
        .and_then(|m| m.get(&ptr).cloned())
}

// ====== ToSchema 完整 schema 注册 ======
type SchemaRegFn = fn(&mut Components);
static SCHEMA_REGISTRY: Lazy<Mutex<Vec<SchemaRegFn>>> = Lazy::new(|| Mutex::new(Vec::new()));

pub fn register_schema_for<T>()
where
    T: crate::ToSchema + ::utoipa::PartialSchema + 'static,
{
    fn add_impl<U: crate::ToSchema + ::utoipa::PartialSchema>(components: &mut Components) {
        let mut refs: Vec<(
            String,
            ::utoipa::openapi::RefOr<::utoipa::openapi::schema::Schema>,
        )> = Vec::new();
        <U as crate::ToSchema>::schemas(&mut refs);
        for (name, schema) in refs {
            components.schemas.entry(name).or_insert(schema);
        }
        let name = <U as crate::ToSchema>::name().into_owned();
        let schema = <U as ::utoipa::PartialSchema>::schema();
        components.schemas.entry(name).or_insert(schema);
    }
    let mut reg = SCHEMA_REGISTRY.lock().expect("schema registry poisoned");
    reg.push(add_impl::<T> as SchemaRegFn);
}

pub fn apply_registered_schemas(openapi: &mut OpenApi) {
    let mut components = openapi
        .components
        .clone()
        .unwrap_or_else(|| ComponentsBuilder::new().build());
    if let Ok(reg) = SCHEMA_REGISTRY.lock() {
        for f in reg.iter() {
            f(&mut components);
        }
    }
    openapi.components = Some(components);
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Serialize;
    use utoipa::ToSchema;

    async fn ok_handler(_req: SilentRequest) -> SilentResult<SilentResponse> {
        Ok(SilentResponse::text("ok"))
    }

    #[test]
    fn test_register_and_lookup_doc() {
        let handler = Arc::new(HandlerWrapper::new(|_req: SilentRequest| async move {
            Ok::<_, silent::SilentError>(SilentResponse::text("doc"))
        }));
        let ptr = Arc::as_ptr(&handler) as *const () as usize;
        register_doc_by_ptr(ptr, Some("summary"), Some("desc"));
        let got = lookup_doc_by_handler_ptr(ptr).expect("doc meta");
        assert_eq!(got.summary.as_deref(), Some("summary"));
        assert_eq!(got.description.as_deref(), Some("desc"));
    }

    #[test]
    fn test_register_and_lookup_response() {
        let handler = Arc::new(HandlerWrapper::new(ok_handler));
        let ptr = Arc::as_ptr(&handler) as *const () as usize;
        register_response_by_ptr(ptr, ResponseMeta::TextPlain);
        let got = lookup_response_by_handler_ptr(ptr).expect("resp meta");
        matches!(got, ResponseMeta::TextPlain);
    }

    #[test]
    fn test_list_registered_json_types() {
        let h1 = Arc::new(HandlerWrapper::new(ok_handler));
        let h2 = Arc::new(HandlerWrapper::new(ok_handler));
        let p1 = Arc::as_ptr(&h1) as *const () as usize;
        let p2 = Arc::as_ptr(&h2) as *const () as usize;
        register_response_by_ptr(p1, ResponseMeta::Json { type_name: "User" });
        register_response_by_ptr(p2, ResponseMeta::Json { type_name: "User" });
        let list = list_registered_json_types();
        assert!(list.contains(&"User"));
        assert_eq!(list.len(), 1);
    }

    #[derive(Serialize, ToSchema)]
    struct FooSchema {
        id: i32,
        name: String,
    }

    #[test]
    fn test_register_schema_and_apply() {
        register_schema_for::<FooSchema>();
        let mut openapi = crate::OpenApiDoc::new("T", "1").into_openapi();
        apply_registered_schemas(&mut openapi);
        let components = openapi.components.expect("components");
        assert!(components.schemas.contains_key("FooSchema"));
    }

    // ====== 枚举变体文档测试 ======

    #[derive(Serialize, ToSchema)]
    #[allow(dead_code)]
    enum ApiResponse {
        Success { data: String },
        Error { code: i32, message: String },
    }

    #[test]
    fn test_register_enum_schema() {
        register_schema_for::<ApiResponse>();
        let mut openapi = crate::OpenApiDoc::new("T", "1").into_openapi();
        apply_registered_schemas(&mut openapi);
        let components = openapi.components.expect("components");
        assert!(components.schemas.contains_key("ApiResponse"));
    }

    #[derive(Serialize, ToSchema)]
    #[allow(dead_code)]
    enum Status {
        Active,
        Inactive,
        Pending,
    }

    #[test]
    fn test_register_unit_enum_schema() {
        register_schema_for::<Status>();
        let mut openapi = crate::OpenApiDoc::new("T", "1").into_openapi();
        apply_registered_schemas(&mut openapi);
        let components = openapi.components.expect("components");
        assert!(components.schemas.contains_key("Status"));
    }

    #[derive(Serialize, ToSchema)]
    struct NestedData {
        value: i32,
    }

    #[derive(Serialize, ToSchema)]
    #[allow(dead_code)]
    enum ComplexEnum {
        WithStruct(NestedData),
        WithString(String),
        Empty,
    }

    #[test]
    fn test_register_enum_with_nested_schemas() {
        register_schema_for::<ComplexEnum>();
        let mut openapi = crate::OpenApiDoc::new("T", "1").into_openapi();
        apply_registered_schemas(&mut openapi);
        let components = openapi.components.expect("components");
        assert!(components.schemas.contains_key("ComplexEnum"));
        // 嵌套的 NestedData 也应被注册
        assert!(components.schemas.contains_key("NestedData"));
    }

    #[test]
    fn test_register_request_and_lookup() {
        let handler = Arc::new(HandlerWrapper::new(ok_handler));
        let ptr = Arc::as_ptr(&handler) as *const () as usize;
        register_request_by_ptr(ptr, RequestMeta::JsonBody { type_name: "User" });
        register_request_by_ptr(
            ptr,
            RequestMeta::QueryParams {
                type_name: "Filter",
            },
        );
        let got = lookup_request_by_handler_ptr(ptr).expect("request meta");
        assert_eq!(got.len(), 2);
        assert!(matches!(
            &got[0],
            RequestMeta::JsonBody { type_name: "User" }
        ));
        assert!(matches!(
            &got[1],
            RequestMeta::QueryParams {
                type_name: "Filter"
            }
        ));
    }
}
