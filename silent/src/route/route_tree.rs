use async_trait::async_trait;
use http::StatusCode;
use memchr::memchr;
use smallvec::SmallVec;
use std::collections::HashMap;
use std::ops::Range;
use std::sync::Arc;

use crate::core::path_param::PathParam;
use crate::handler::Handler;
use crate::middleware::MiddleWareHandler;
use crate::route::handler_match::SpecialPath;
use crate::{Method, Next, Request, Response, SilentError};

#[derive(Clone)]
pub(crate) enum SpecialSeg {
    Root,
    Static(String),
    String { key: String },
    Int { key: String },
    I64 { key: String },
    I32 { key: String },
    U64 { key: String },
    U32 { key: String },
    Uuid { key: String },
    Path { key: String },
    FullPath { key: String },
}

impl SpecialSeg {
    fn is_full_path(&self) -> bool {
        matches!(self, SpecialSeg::FullPath { .. })
    }

    pub(crate) fn as_static_key(&self) -> Option<&str> {
        if let SpecialSeg::Static(value) = self {
            Some(value.as_str())
        } else {
            None
        }
    }
}

pub(crate) fn parse_special_seg(raw: String) -> SpecialSeg {
    if raw.is_empty() {
        return SpecialSeg::Root;
    }

    if raw.starts_with('<') && raw.ends_with('>') {
        match SpecialPath::from(raw.as_str()) {
            SpecialPath::String(key) => SpecialSeg::String { key },
            SpecialPath::Int(key) => SpecialSeg::Int { key },
            SpecialPath::I64(key) => SpecialSeg::I64 { key },
            SpecialPath::I32(key) => SpecialSeg::I32 { key },
            SpecialPath::U64(key) => SpecialSeg::U64 { key },
            SpecialPath::U32(key) => SpecialSeg::U32 { key },
            SpecialPath::UUid(key) => SpecialSeg::Uuid { key },
            SpecialPath::Path(key) => SpecialSeg::Path { key },
            SpecialPath::FullPath(key) => SpecialSeg::FullPath { key },
        }
    } else {
        SpecialSeg::Static(raw)
    }
}

struct CapturedStr {
    range: Range<usize>,
}

impl CapturedStr {
    fn new(value: &str, full: &str) -> Self {
        CapturedStr {
            range: slice_range(full, value),
        }
    }
}

struct PathMatch<'a> {
    remain: &'a str,
    capture: Option<PathMatchCapture>,
}

impl<'a> PathMatch<'a> {
    fn new(remain: &'a str, capture: Option<PathMatchCapture>) -> Self {
        Self { remain, capture }
    }
}

enum PathMatchCapture {
    Str(CapturedStr),
    Path(CapturedStr),
    Full(CapturedStr),
    I32(i32),
    I64(i64),
    U64(u64),
    U32(u32),
    Uuid(uuid::Uuid),
}

#[derive(Clone)]
pub struct RouteTree {
    pub(crate) children: Vec<RouteTree>,
    pub(crate) handler: HashMap<Method, Arc<dyn Handler>>,
    pub(crate) middlewares: Arc<[Arc<dyn MiddleWareHandler>]>,
    pub(crate) static_children: HashMap<Box<str>, usize>,
    pub(crate) dynamic_children: SmallVec<[usize; 4]>,
    pub(crate) middleware_start: usize,
    pub(crate) configs: Option<crate::Configs>,
    pub(crate) segment: SpecialSeg,
    pub(crate) has_handler: bool,
}

impl RouteTree {
    pub(crate) fn get_configs(&self) -> Option<&crate::Configs> {
        self.configs.as_ref()
    }

    fn call_path_only<'p>(&self, path: &'p str, full_path: &'p str) -> Option<PathMatch<'p>> {
        match &self.segment {
            SpecialSeg::Root => {
                let normalized = if path == "/" { "" } else { path };
                if !normalized.is_empty() && self.children.is_empty() {
                    None
                } else {
                    Some(PathMatch::new(normalized, None))
                }
            }
            SpecialSeg::Static(value) => {
                match_static_segment(value, path).map(|remain| PathMatch::new(remain, None))
            }
            SpecialSeg::String { .. } => {
                let (segment, remain) = strip_one_segment(path);
                if segment.is_empty() {
                    None
                } else {
                    Some(PathMatch::new(
                        remain,
                        Some(PathMatchCapture::Str(CapturedStr::new(segment, full_path))),
                    ))
                }
            }
            SpecialSeg::Int { .. } | SpecialSeg::I32 { .. } => {
                let (segment, remain) = strip_one_segment(path);
                if segment.is_empty() {
                    return None;
                }
                match segment.parse::<i32>() {
                    Ok(v) => Some(PathMatch::new(remain, Some(PathMatchCapture::I32(v)))),
                    Err(_) => None,
                }
            }
            SpecialSeg::I64 { .. } => {
                let (segment, remain) = strip_one_segment(path);
                if segment.is_empty() {
                    return None;
                }
                match segment.parse::<i64>() {
                    Ok(v) => Some(PathMatch::new(remain, Some(PathMatchCapture::I64(v)))),
                    Err(_) => None,
                }
            }
            SpecialSeg::U64 { .. } => {
                let (segment, remain) = strip_one_segment(path);
                if segment.is_empty() {
                    return None;
                }
                match segment.parse::<u64>() {
                    Ok(v) => Some(PathMatch::new(remain, Some(PathMatchCapture::U64(v)))),
                    Err(_) => None,
                }
            }
            SpecialSeg::U32 { .. } => {
                let (segment, remain) = strip_one_segment(path);
                if segment.is_empty() {
                    return None;
                }
                match segment.parse::<u32>() {
                    Ok(v) => Some(PathMatch::new(remain, Some(PathMatchCapture::U32(v)))),
                    Err(_) => None,
                }
            }
            SpecialSeg::Uuid { .. } => {
                let (segment, remain) = strip_one_segment(path);
                if segment.is_empty() {
                    return None;
                }
                match uuid::Uuid::parse_str(segment) {
                    Ok(v) => Some(PathMatch::new(remain, Some(PathMatchCapture::Uuid(v)))),
                    Err(_) => None,
                }
            }
            SpecialSeg::Path { .. } => {
                let (segment, remain) = strip_one_segment(path);
                Some(PathMatch::new(
                    remain,
                    Some(PathMatchCapture::Path(CapturedStr::new(segment, full_path))),
                ))
            }
            SpecialSeg::FullPath { .. } => {
                let trimmed = strip_leading_slash(path);
                let (_, remain) = strip_one_segment(path);
                Some(PathMatch::new(
                    remain,
                    Some(PathMatchCapture::Full(CapturedStr::new(trimmed, full_path))),
                ))
            }
        }
    }

    fn bind_params(&self, req: &mut Request, matched: &PathMatch<'_>, source: &Arc<str>) -> bool {
        match (&self.segment, &matched.capture) {
            (SpecialSeg::Root | SpecialSeg::Static(_), _) => true,
            (SpecialSeg::String { key }, Some(PathMatchCapture::Str(captured))) => {
                req.set_path_params(
                    key.clone(),
                    PathParam::borrowed_str(Arc::clone(source), captured.range.clone()),
                );
                true
            }
            (SpecialSeg::Int { key }, Some(PathMatchCapture::I32(value)))
            | (SpecialSeg::I32 { key }, Some(PathMatchCapture::I32(value))) => {
                req.set_path_params(key.clone(), (*value).into());
                true
            }
            (SpecialSeg::I64 { key }, Some(PathMatchCapture::I64(value))) => {
                req.set_path_params(key.clone(), (*value).into());
                true
            }
            (SpecialSeg::U64 { key }, Some(PathMatchCapture::U64(value))) => {
                req.set_path_params(key.clone(), (*value).into());
                true
            }
            (SpecialSeg::U32 { key }, Some(PathMatchCapture::U32(value))) => {
                req.set_path_params(key.clone(), (*value).into());
                true
            }
            (SpecialSeg::Uuid { key }, Some(PathMatchCapture::Uuid(value))) => {
                req.set_path_params(key.clone(), (*value).into());
                true
            }
            (SpecialSeg::Path { key }, Some(PathMatchCapture::Path(captured))) => {
                req.set_path_params(
                    key.clone(),
                    PathParam::borrowed_path(Arc::clone(source), captured.range.clone()),
                );
                true
            }
            (SpecialSeg::FullPath { key }, Some(PathMatchCapture::Full(captured))) => {
                req.set_path_params(
                    key.clone(),
                    PathParam::borrowed_path(Arc::clone(source), captured.range.clone()),
                );
                true
            }
            _ => false,
        }
    }

    pub(crate) async fn call_with_path(
        &self,
        mut req: Request,
        offset: usize,
        path: Arc<str>,
    ) -> crate::error::SilentResult<Response> {
        if let Some(configs) = self.get_configs() {
            req.configs_mut().extend_from(configs);
        }

        let endpoint: Arc<dyn Handler> = Arc::new(ContinuationHandler::new(
            Arc::new(self.clone()),
            offset,
            Arc::clone(&path),
        ));
        let middleware_slice = &self.middlewares[self.middleware_start..];
        let next = Next::build(endpoint, middleware_slice);
        next.call(req).await
    }

    async fn call_children(
        &self,
        req: Request,
        offset: usize,
        path: Arc<str>,
    ) -> crate::error::SilentResult<Response> {
        let full_path = path.as_ref();
        let remain_slice = &full_path[offset..];

        let mut candidate_indices: SmallVec<[usize; 8]> = SmallVec::new();
        if remain_slice.is_empty() {
            candidate_indices.extend(self.static_children.values().copied());
            candidate_indices.extend(self.dynamic_children.iter().copied());
        } else {
            let (segment, _) = strip_one_segment(remain_slice);
            if let Some(&idx) = self.static_children.get(segment) {
                candidate_indices.push(idx);
            }
            candidate_indices.extend(self.dynamic_children.iter().copied());
        }

        for idx in candidate_indices {
            let child = &self.children[idx];
            if let Some(candidate) = child.call_path_only(remain_slice, full_path) {
                let next_offset = remain_offset(full_path, candidate.remain);
                if !child.path_can_resolve(next_offset, full_path) {
                    continue;
                }
                let mut real_req = req;
                if !child.bind_params(&mut real_req, &candidate, &path) {
                    return Err(SilentError::business_error(
                        StatusCode::NOT_FOUND,
                        "not found".to_string(),
                    ));
                }
                return child
                    .call_with_path(real_req, next_offset, Arc::clone(&path))
                    .await;
            }
        }

        if remain_slice.is_empty() {
            return if self.has_handler {
                self.handler.call(req).await
            } else {
                Err(SilentError::business_error(
                    StatusCode::NOT_FOUND,
                    "not found".to_string(),
                ))
            };
        }

        if self.segment.is_full_path() && self.has_handler {
            return self.handler.call(req).await;
        }

        Err(SilentError::business_error(
            StatusCode::NOT_FOUND,
            "not found".to_string(),
        ))
    }

    fn path_can_resolve(&self, offset: usize, full_path: &str) -> bool {
        let remain = &full_path[offset..];
        let mut candidate_indices: SmallVec<[usize; 8]> = SmallVec::new();
        if remain.is_empty() {
            candidate_indices.extend(self.static_children.values().copied());
            candidate_indices.extend(self.dynamic_children.iter().copied());
        } else {
            let (segment, _) = strip_one_segment(remain);
            if let Some(&idx) = self.static_children.get(segment) {
                candidate_indices.push(idx);
            }
            candidate_indices.extend(self.dynamic_children.iter().copied());
        }

        for idx in candidate_indices {
            let child = &self.children[idx];
            if let Some(candidate) = child.call_path_only(remain, full_path) {
                let next_offset = remain_offset(full_path, candidate.remain);
                if child.path_can_resolve(next_offset, full_path) {
                    return true;
                }
            }
        }

        remain.is_empty() || self.segment.is_full_path()
    }
}

#[async_trait]
impl Handler for RouteTree {
    async fn call(&self, req: Request) -> crate::error::SilentResult<Response> {
        let mut req = req;
        if let Some(configs) = self.get_configs().cloned() {
            *req.configs_mut() = configs;
        }

        let path_source = Arc::<str>::from(req.uri().path().to_string());
        let full_path = &*path_source;
        req.set_path_source(path_source.clone());

        let Some(candidate) = self.call_path_only(full_path, full_path) else {
            return Err(SilentError::business_error(
                StatusCode::NOT_FOUND,
                "not found".to_string(),
            ));
        };

        if !self.bind_params(&mut req, &candidate, &path_source) {
            return Err(SilentError::business_error(
                StatusCode::NOT_FOUND,
                "not found".to_string(),
            ));
        }

        let offset = remain_offset(full_path, candidate.remain);
        self.call_with_path(req, offset, path_source).await
    }
}

// 优化：为Arc<RouteTree>实现Handler trait，避免不必要的克隆
#[async_trait]
impl Handler for Arc<RouteTree> {
    async fn call(&self, req: Request) -> crate::error::SilentResult<Response> {
        (**self).call(req).await
    }
}

struct ContinuationHandler {
    node: Arc<RouteTree>,
    offset: usize,
    path: Arc<str>,
}

impl ContinuationHandler {
    fn new(node: Arc<RouteTree>, offset: usize, path: Arc<str>) -> Self {
        Self { node, offset, path }
    }
}

#[async_trait]
impl Handler for ContinuationHandler {
    async fn call(&self, req: Request) -> crate::error::SilentResult<Response> {
        self.node
            .call_children(req, self.offset, Arc::clone(&self.path))
            .await
    }
}

fn strip_leading_slash(path: &str) -> &str {
    path.strip_prefix('/').unwrap_or(path)
}

fn strip_one_segment(path: &str) -> (&str, &str) {
    let trimmed = strip_leading_slash(path);
    if trimmed.is_empty() {
        return ("", "");
    }

    let bytes = trimmed.as_bytes();
    if let Some(idx) = memchr(b'/', bytes) {
        (&trimmed[..idx], &trimmed[idx + 1..])
    } else {
        (trimmed, "")
    }
}

fn match_static_segment<'a>(value: &str, path: &'a str) -> Option<&'a str> {
    if value.is_empty() {
        return Some(path);
    }

    let trimmed = strip_leading_slash(path);
    if trimmed == value {
        return Some("");
    }

    if trimmed.len() > value.len() && trimmed.starts_with(value) {
        let rest = &trimmed[value.len()..];
        if let Some(rem) = rest.strip_prefix('/') {
            return Some(rem);
        }
    }

    None
}

fn slice_range(full: &str, slice: &str) -> Range<usize> {
    let full_ptr = full.as_ptr() as usize;
    let slice_ptr = slice.as_ptr() as usize;
    let start = slice_ptr.saturating_sub(full_ptr);
    let end = start + slice.len();
    start..end
}

fn remain_offset(full: &str, remain: &str) -> usize {
    debug_assert!(full.len() >= remain.len());
    full.len() - remain.len()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::route::Route;
    use bytes::Bytes;
    use http_body_util::BodyExt;
    use std::sync::Arc;

    async fn hello(_: Request) -> Result<String, SilentError> {
        Ok("hello".to_string())
    }

    async fn world<'a>(_: Request) -> Result<&'a str, SilentError> {
        Ok("world")
    }

    fn make_path_arc(path: &str) -> Arc<str> {
        Arc::<str>::from(path.to_owned())
    }

    #[tokio::test]
    async fn route_path_conflicts_and_root_cases() {
        async fn hello(_: Request) -> Result<String, SilentError> {
            Ok("hello".into())
        }
        async fn world<'a>(_: Request) -> Result<&'a str, SilentError> {
            Ok("world")
        }

        // path conflict
        let route = Route::new("")
            .append(Route::new("api").get(hello))
            .append(Route::new("api/v1").get(world));
        let tree = route.convert_to_route_tree();

        let mut req = Request::empty();
        *req.uri_mut() = "/api".parse().unwrap();
        let mut res = tree.call(req).await.unwrap();
        assert_eq!(
            res.body.frame().await.unwrap().unwrap().data_ref().unwrap(),
            &Bytes::from("hello")
        );

        let mut req = Request::empty();
        *req.uri_mut() = "/api/v1".parse().unwrap();
        let mut res = tree.call(req).await.unwrap();
        assert_eq!(
            res.body.frame().await.unwrap().unwrap().data_ref().unwrap(),
            &Bytes::from("world")
        );

        // root matching
        let route = Route::new("").get(hello);
        let tree = route.convert_to_route_tree();
        let mut req = Request::empty();
        *req.uri_mut() = "/".parse().unwrap();
        let mut res = tree.call(req).await.unwrap();
        assert_eq!(
            res.body.frame().await.unwrap().unwrap().data_ref().unwrap(),
            &Bytes::from("hello")
        );

        // typed params
        let route = Route::new("")
            .append(Route::new("user/<id:i64>").get(hello))
            .append(Route::new("post/<slug>").get(world));
        let tree = route.convert_to_route_tree();
        let mut req = Request::empty();
        *req.uri_mut() = "/user/123".parse().unwrap();
        req.set_path_source(make_path_arc("/user/123"));
        let _ = tree
            .call_with_path(req, 0, make_path_arc("/user/123"))
            .await;
    }

    #[tokio::test]
    async fn dfs_with_double_star_child_priority() {
        // <path:**> should capture the remaining path but allow child matching priority
        let route = Route::new("<path:**>")
            .get(hello)
            .append(Route::new("world").get(world));

        let routes = route.convert_to_route_tree();

        let mut req = Request::empty();
        req.set_remote(
            "127.0.0.1:8080"
                .parse::<crate::core::remote_addr::RemoteAddr>()
                .unwrap(),
        );
        *req.uri_mut() = "/hello/world".parse().unwrap();

        let mut res = routes.call(req).await.unwrap();
        let body = res
            .body
            .frame()
            .await
            .unwrap()
            .unwrap()
            .data_ref()
            .unwrap()
            .clone();
        assert_eq!(body, Bytes::from("world"));
    }

    #[tokio::test]
    async fn dfs_with_double_star_parent_fallback() {
        // If child doesn't match, fallback to parent handler under **
        let route = Route::new("<path:**>")
            .get(hello)
            .append(Route::new("world").get(world));
        let routes = route.convert_to_route_tree();

        let mut req = Request::empty();
        req.set_remote(
            "127.0.0.1:8080"
                .parse::<crate::core::remote_addr::RemoteAddr>()
                .unwrap(),
        );
        *req.uri_mut() = "/hello/world1".parse().unwrap();

        let mut res = routes.call(req).await.unwrap();
        let body = res
            .body
            .frame()
            .await
            .unwrap()
            .unwrap()
            .data_ref()
            .unwrap()
            .clone();
        assert_eq!(body, Bytes::from("hello"));
    }

    #[tokio::test]
    async fn dfs_collects_layered_middlewares() {
        #[derive(Clone)]
        struct CounterMw(Arc<std::sync::atomic::AtomicUsize>);
        #[async_trait::async_trait]
        impl MiddleWareHandler for CounterMw {
            async fn handle(
                &self,
                req: Request,
                next: &Next,
            ) -> crate::error::SilentResult<Response> {
                self.0.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                next.call(req).await
            }
        }

        async fn ok(_: Request) -> Result<String, SilentError> {
            Ok("ok".into())
        }

        let c1 = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let c2 = Arc::new(std::sync::atomic::AtomicUsize::new(0));

        let route = Route::new("")
            .hook(CounterMw(c1.clone()))
            .append(Route::new("api").hook(CounterMw(c2.clone())).get(ok));
        let routes = route.convert_to_route_tree();

        let mut req = Request::empty();
        req.set_remote(
            "127.0.0.1:8080"
                .parse::<crate::core::remote_addr::RemoteAddr>()
                .unwrap(),
        );
        *req.uri_mut() = "/api".parse().unwrap();

        let _ = routes.call(req).await.unwrap();
        assert_eq!(c1.load(std::sync::atomic::Ordering::SeqCst), 1);
        assert_eq!(c2.load(std::sync::atomic::Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn oauth2_applications_get_should_not_405() {
        async fn ok(_: Request) -> Result<String, SilentError> {
            Ok("ok".into())
        }
        async fn created(_: Request) -> Result<String, SilentError> {
            Ok("created".into())
        }
        async fn get_detail(_: Request) -> Result<String, SilentError> {
            Ok("detail".into())
        }
        async fn updated(_: Request) -> Result<String, SilentError> {
            Ok("updated".into())
        }
        async fn deleted(_: Request) -> Result<String, SilentError> {
            Ok("deleted".into())
        }
        async fn patched(_: Request) -> Result<String, SilentError> {
            Ok("patched".into())
        }

        // 构造与用户描述一致的路由
        let route = Route::new("").append(
            Route::new("oauth2")
                // OAuth2应用管理路由
                .append(
                    Route::new("applications")
                        .get(ok) // 获取应用列表
                        .post(created) // 创建应用
                        .append(
                            Route::new("<id:str>")
                                .get(get_detail)
                                .put(updated)
                                .delete(deleted)
                                .append(Route::new("status").patch(patched))
                                .append(Route::new("regenerate-secret").post(created))
                                .append(Route::new("access-config").put(updated)),
                        ),
                )
                // OAuth2应用信息获取路由（用于授权页面）
                .append(Route::new("application/info/<app_key:str>").get(ok))
                // OAuth2授权流程路由
                .append(Route::new("application/user-authorize").post(created))
                .append(Route::new("application/user-authorize/code").post(created))
                // 审计日志管理路由
                .append(
                    Route::new("audit-logs")
                        .get(ok)
                        .append(Route::new("stats").get(ok))
                        .append(Route::new("export").post(created)),
                )
                // 用户管理路由
                .append(Route::new("my-applications").get(ok)),
        );

        let routes = route.convert_to_route_tree();

        // 发起 GET /oauth2/applications，应命中列表GET而不是405
        let mut req = Request::empty();
        req.set_remote(
            "127.0.0.1:8080"
                .parse::<crate::core::remote_addr::RemoteAddr>()
                .unwrap(),
        );
        *req.uri_mut() = "/oauth2/applications".parse().unwrap();
        *req.method_mut() = Method::GET;

        let mut res = routes.call(req).await.expect("should route ok");
        let body = res
            .body
            .frame()
            .await
            .unwrap()
            .unwrap()
            .data_ref()
            .unwrap()
            .clone();
        assert_eq!(body, Bytes::from("ok"));
    }

    // ==================== SpecialSeg 方法测试 ====================

    #[test]
    fn test_special_seg_is_full_path() {
        assert!(!SpecialSeg::Root.is_full_path());
        assert!(!SpecialSeg::Static("test".to_string()).is_full_path());
        assert!(
            !SpecialSeg::String {
                key: "k".to_string()
            }
            .is_full_path()
        );
        assert!(
            SpecialSeg::FullPath {
                key: "k".to_string()
            }
            .is_full_path()
        );
        assert!(
            !SpecialSeg::Path {
                key: "k".to_string()
            }
            .is_full_path()
        );
    }

    #[test]
    fn test_special_seg_as_static_key() {
        assert_eq!(
            SpecialSeg::Static("api".to_string()).as_static_key(),
            Some("api")
        );
        assert_eq!(SpecialSeg::Root.as_static_key(), None);
        assert_eq!(
            SpecialSeg::String {
                key: "k".to_string()
            }
            .as_static_key(),
            None
        );
    }

    // ==================== parse_special_seg 函数测试 ====================

    #[test]
    fn test_parse_special_seg_empty() {
        assert!(matches!(
            parse_special_seg("".to_string()),
            SpecialSeg::Root
        ));
    }

    #[test]
    fn test_parse_special_seg_static() {
        assert!(matches!(
            parse_special_seg("api".to_string()),
            SpecialSeg::Static(_)
        ));
    }

    #[test]
    fn test_parse_special_seg_string_param() {
        match parse_special_seg("<id:str>".to_string()) {
            SpecialSeg::String { key } => assert_eq!(key, "id"),
            _ => panic!("Expected String segment"),
        }
    }

    #[test]
    fn test_parse_special_seg_int_param() {
        match parse_special_seg("<id:int>".to_string()) {
            SpecialSeg::Int { key } => assert_eq!(key, "id"),
            _ => panic!("Expected Int segment"),
        }
    }

    #[test]
    fn test_parse_special_seg_i64_param() {
        match parse_special_seg("<id:i64>".to_string()) {
            SpecialSeg::I64 { key } => assert_eq!(key, "id"),
            _ => panic!("Expected I64 segment"),
        }
    }

    #[test]
    fn test_parse_special_seg_i32_param() {
        match parse_special_seg("<id:i32>".to_string()) {
            SpecialSeg::I32 { key } => assert_eq!(key, "id"),
            _ => panic!("Expected I32 segment"),
        }
    }

    #[test]
    fn test_parse_special_seg_u64_param() {
        match parse_special_seg("<id:u64>".to_string()) {
            SpecialSeg::U64 { key } => assert_eq!(key, "id"),
            _ => panic!("Expected U64 segment"),
        }
    }

    #[test]
    fn test_parse_special_seg_u32_param() {
        match parse_special_seg("<id:u32>".to_string()) {
            SpecialSeg::U32 { key } => assert_eq!(key, "id"),
            _ => panic!("Expected U32 segment"),
        }
    }

    #[test]
    fn test_parse_special_seg_uuid_param() {
        match parse_special_seg("<id:uuid>".to_string()) {
            SpecialSeg::Uuid { key } => assert_eq!(key, "id"),
            _ => panic!("Expected Uuid segment"),
        }
    }

    #[test]
    fn test_parse_special_seg_path_param() {
        match parse_special_seg("<path:path>".to_string()) {
            SpecialSeg::Path { key } => assert_eq!(key, "path"),
            _ => panic!("Expected Path segment"),
        }
    }

    #[test]
    fn test_parse_special_seg_full_path_param() {
        match parse_special_seg("<path:**>".to_string()) {
            SpecialSeg::FullPath { key } => assert_eq!(key, "path"),
            _ => panic!("Expected FullPath segment"),
        }
    }

    // ==================== CapturedStr 测试 ====================

    #[test]
    fn test_captured_str_new() {
        let full = "/api/users/123";
        // value 必须是 full 的子切片，这样指针计算才正确
        let value = &full[5..10]; // "users"
        let captured = CapturedStr::new(value, full);
        // slice_range 使用指针计算，获取的是子字符串在完整字符串中的位置
        assert_eq!(captured.range.start, 5);
        assert_eq!(captured.range.end, 10);
        assert_eq!(&full[captured.range.clone()], "users");
    }

    // ==================== PathMatch 测试 ====================

    #[test]
    fn test_path_match_new() {
        let match_result = PathMatch::new("remain", None);
        assert_eq!(match_result.remain, "remain");
        assert!(match_result.capture.is_none());
    }

    #[test]
    fn test_path_match_new_with_capture() {
        let full = "/api/users";
        let captured = CapturedStr::new("users", full);
        let match_result = PathMatch::new("remain", Some(PathMatchCapture::Str(captured)));
        assert_eq!(match_result.remain, "remain");
        assert!(match_result.capture.is_some());
    }

    // ==================== 辅助函数测试 ====================

    #[test]
    fn test_strip_leading_slash() {
        assert_eq!(strip_leading_slash("/api/users"), "api/users");
        assert_eq!(strip_leading_slash("api/users"), "api/users");
        assert_eq!(strip_leading_slash("/"), "");
        assert_eq!(strip_leading_slash(""), "");
    }

    #[test]
    fn test_strip_one_segment() {
        // 测试单段路径
        let (seg, remain) = strip_one_segment("/api");
        assert_eq!(seg, "api");
        assert_eq!(remain, "");

        // 测试多段路径
        let (seg, remain) = strip_one_segment("/api/users");
        assert_eq!(seg, "api");
        assert_eq!(remain, "users");

        // 测试三段路径
        let (seg, remain) = strip_one_segment("/api/users/123");
        assert_eq!(seg, "api");
        assert_eq!(remain, "users/123");

        // 测试空路径
        let (seg, remain) = strip_one_segment("");
        assert_eq!(seg, "");
        assert_eq!(remain, "");

        // 测试无斜杠路径
        let (seg, remain) = strip_one_segment("api");
        assert_eq!(seg, "api");
        assert_eq!(remain, "");
    }

    #[test]
    fn test_match_static_segment() {
        // 精确匹配
        assert_eq!(match_static_segment("api", "/api"), Some(""));
        assert_eq!(match_static_segment("api", "/api/"), Some(""));

        // 带子路径匹配
        assert_eq!(match_static_segment("api", "/api/users"), Some("users"));
        assert_eq!(
            match_static_segment("api", "/api/users/123"),
            Some("users/123")
        );

        // 不匹配
        assert_eq!(match_static_segment("api", "/v1"), None);
        assert_eq!(match_static_segment("api", "/users"), None);

        // 空值匹配
        assert_eq!(match_static_segment("", "/any"), Some("/any"));
        assert_eq!(match_static_segment("", "/"), Some("/"));
    }

    #[test]
    fn test_slice_range() {
        let full = "/api/users/123";
        // slice 必须是 full 的子切片
        // 位置: 0=/ 1=a 2=p 3=i 4=/ 5=u 6=s 7=e 8=r 9=s 10=/ 11=1 12=2 13=3
        let slice = &full[5..10]; // "users"
        let range = slice_range(full, slice);
        assert_eq!(range.start, 5);
        assert_eq!(range.end, 10);
        assert_eq!(&full[range], "users");
    }

    #[test]
    fn test_slice_range_with_prefix() {
        let full = "/api/users/123";
        // slice 必须是 full 的子切片
        // 位置: 0=/ 1=a 2=p 3=i 4=/ 5=u 6=s 7=e 8=r 9=s 10=/ 11=1 12=2 13=3
        let slice = &full[1..4]; // "api"
        let range = slice_range(full, slice);
        assert_eq!(range.start, 1);
        assert_eq!(range.end, 4);
        assert_eq!(&full[range], "api");
    }

    #[test]
    fn test_remain_offset() {
        let full = "/api/users/123";
        // remain_offset = full.len() - remain.len()
        assert_eq!(remain_offset(full, ""), 14); // 14 - 0
        assert_eq!(remain_offset(full, "123"), 11); // 14 - 3
        assert_eq!(remain_offset(full, "users/123"), 5); // 14 - 9
        assert_eq!(remain_offset(full, "api/users/123"), 1); // 14 - 13 (缺少前导斜杠)
    }

    // ==================== RouteTree::call_path_only 测试 ====================

    #[test]
    fn test_route_tree_call_path_only_root() {
        let route = Route::new("").get(hello);
        let tree = route.convert_to_route_tree();

        // 测试根路径匹配
        let result = tree.call_path_only("/", "/");
        assert!(result.is_some());
        assert_eq!(result.unwrap().remain, "");
    }

    #[test]
    fn test_route_tree_call_path_only_static() {
        let route = Route::new("api").get(hello);
        let tree = route.convert_to_route_tree();

        // 测试静态路径匹配
        let result = tree.call_path_only("/api", "/api");
        assert!(result.is_some());
        assert_eq!(result.unwrap().remain, "");
    }

    #[test]
    fn test_route_tree_call_path_only_string_param() {
        let route = Route::new("<id:str>").get(hello);
        let tree = route.convert_to_route_tree();

        // 测试字符串参数匹配
        let result = tree.call_path_only("/123", "/123");
        assert!(result.is_some());
        assert!(result.unwrap().capture.is_some());
    }

    #[test]
    fn test_route_tree_call_path_only_int_param_valid() {
        let route = Route::new("<id:int>").get(hello);
        let tree = route.convert_to_route_tree();

        // 测试有效整数参数
        let result = tree.call_path_only("/123", "/123");
        assert!(result.is_some());
    }

    #[test]
    fn test_route_tree_call_path_only_int_param_invalid() {
        let route = Route::new("<id:int>").get(hello);
        let tree = route.convert_to_route_tree();

        // 测试无效整数参数
        let result = tree.call_path_only("/abc", "/abc");
        assert!(result.is_none());
    }

    #[test]
    fn test_route_tree_call_path_only_i64_param() {
        let route = Route::new("<id:i64>").get(hello);
        let tree = route.convert_to_route_tree();

        let result = tree.call_path_only("/123", "/123");
        assert!(result.is_some());
    }

    #[test]
    fn test_route_tree_call_path_only_u64_param() {
        let route = Route::new("<id:u64>").get(hello);
        let tree = route.convert_to_route_tree();

        let result = tree.call_path_only("/123", "/123");
        assert!(result.is_some());
    }

    #[test]
    fn test_route_tree_call_path_only_uuid_param_valid() {
        let route = Route::new("<id:uuid>").get(hello);
        let tree = route.convert_to_route_tree();

        // 测试有效 UUID
        let uuid_str = "550e8400-e29b-41d4-a716-446655440000";
        let path = format!("/{}", uuid_str);
        let result = tree.call_path_only(path.as_str(), path.as_str());
        assert!(result.is_some());
    }

    #[test]
    fn test_route_tree_call_path_only_uuid_param_invalid() {
        let route = Route::new("<id:uuid>").get(hello);
        let tree = route.convert_to_route_tree();

        // 测试无效 UUID
        let result = tree.call_path_only("/not-a-uuid", "/not-a-uuid");
        assert!(result.is_none());
    }

    #[test]
    fn test_route_tree_call_path_only_path_param() {
        let route = Route::new("<path:path>").get(hello);
        let tree = route.convert_to_route_tree();

        let result = tree.call_path_only("/api/users/123", "/api/users/123");
        assert!(result.is_some());
        assert!(result.unwrap().capture.is_some());
    }

    #[test]
    fn test_route_tree_call_path_only_full_path_param() {
        let route = Route::new("<path:**>").get(hello);
        let tree = route.convert_to_route_tree();

        let result = tree.call_path_only("/api/users/123", "/api/users/123");
        assert!(result.is_some());
    }

    // ==================== RouteTree::bind_params 测试 ====================

    #[tokio::test]
    async fn test_route_tree_bind_params_string() {
        let route = Route::new("<id:str>").get(hello);
        let tree = route.convert_to_route_tree();

        let full_path = Arc::<str>::from("/123");
        let captured = CapturedStr::new("123", &full_path);
        let match_result = PathMatch::new("", Some(PathMatchCapture::Str(captured)));

        let mut req = Request::empty();
        let result = tree.bind_params(&mut req, &match_result, &full_path);
        assert!(result);
    }

    #[tokio::test]
    async fn test_route_tree_bind_params_int() {
        let route = Route::new("<id:int>").get(hello);
        let tree = route.convert_to_route_tree();

        let full_path = Arc::<str>::from("/123");
        let match_result = PathMatch::new("", Some(PathMatchCapture::I32(123)));

        let mut req = Request::empty();
        let result = tree.bind_params(&mut req, &match_result, &full_path);
        assert!(result);
    }

    #[tokio::test]
    async fn test_route_tree_bind_params_path() {
        let route = Route::new("<path:path>").get(hello);
        let tree = route.convert_to_route_tree();

        let full_path = Arc::<str>::from("/api/users/123");
        let captured = CapturedStr::new("api/users/123", &full_path);
        let match_result = PathMatch::new("", Some(PathMatchCapture::Path(captured)));

        let mut req = Request::empty();
        let result = tree.bind_params(&mut req, &match_result, &full_path);
        assert!(result);
    }

    // ==================== RouteTree::path_can_resolve 测试 ====================

    #[tokio::test]
    async fn test_route_tree_path_can_resolve_leaf() {
        let route = Route::new("api").get(hello);
        let tree = route.convert_to_route_tree();

        // 叶子节点，没有子节点，应该可以解析
        let result = tree.path_can_resolve(4, "/api");
        assert!(result);
    }

    #[tokio::test]
    async fn test_route_tree_path_can_resolve_with_children() {
        let route = Route::new("api")
            .append(Route::new("v1").get(hello))
            .get(world);
        let tree = route.convert_to_route_tree();

        // 有子节点，但路径为空，有 handler，应该可以解析
        let result = tree.path_can_resolve(4, "/api");
        assert!(result);
    }

    #[tokio::test]
    async fn test_route_tree_path_can_resolve_full_path() {
        let route = Route::new("<path:**>").get(hello);
        let tree = route.convert_to_route_tree();

        // FullPath 节点，应该可以解析
        let result = tree.path_can_resolve(0, "/api/users/123");
        assert!(result);
    }

    // ==================== ContinuationHandler 测试 ====================

    #[tokio::test]
    async fn test_continuation_handler_creation() {
        let route = Route::new("api").get(hello);
        let tree = route.convert_to_route_tree();
        let arc_tree = Arc::new(tree);

        let _handler = ContinuationHandler::new(arc_tree, 0, make_path_arc("/api"));
        // 验证 ContinuationHandler 可以创建
    }

    // ==================== RouteTree Handler trait 测试 ====================

    #[tokio::test]
    async fn test_route_tree_handler_with_configs() {
        async fn with_configs(_req: Request) -> Result<String, SilentError> {
            Ok("with_configs".into())
        }

        let mut route = Route::new("api");
        route.configs = Some(crate::Configs::default());
        route = route.get(with_configs);
        let tree = route.convert_to_route_tree();

        let mut req = Request::empty();
        req.set_remote(
            "127.0.0.1:8080"
                .parse::<crate::core::remote_addr::RemoteAddr>()
                .unwrap(),
        );
        *req.uri_mut() = "/api".parse().unwrap();

        let result: crate::error::SilentResult<Response> = tree.call(req).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_route_tree_handler_not_found() {
        let route = Route::new("api").get(hello);
        let tree = route.convert_to_route_tree();

        let mut req = Request::empty();
        req.set_remote(
            "127.0.0.1:8080"
                .parse::<crate::core::remote_addr::RemoteAddr>()
                .unwrap(),
        );
        *req.uri_mut() = "/not_found".parse().unwrap();

        let result = tree.call(req).await;
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().status(), StatusCode::NOT_FOUND);
    }

    // ==================== Arc<RouteTree> Handler trait 测试 ====================

    #[tokio::test]
    async fn test_arc_route_tree_handler() {
        let route = Route::new("api").get(hello);
        let tree = route.convert_to_route_tree();
        let arc_tree = Arc::new(tree);

        let mut req = Request::empty();
        req.set_remote(
            "127.0.0.1:8080"
                .parse::<crate::core::remote_addr::RemoteAddr>()
                .unwrap(),
        );
        *req.uri_mut() = "/api".parse().unwrap();

        let result = arc_tree.call(req).await;
        assert!(result.is_ok());
    }

    // ==================== 边界条件测试 ====================

    #[tokio::test]
    async fn test_route_tree_empty_path() {
        let route = Route::new("").get(hello);
        let tree = route.convert_to_route_tree();

        let mut req = Request::empty();
        req.set_remote(
            "127.0.0.1:8080"
                .parse::<crate::core::remote_addr::RemoteAddr>()
                .unwrap(),
        );
        *req.uri_mut() = "/".parse().unwrap();

        let result = tree.call(req).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_route_tree_deep_nesting() {
        let route = Route::new("api").append(
            Route::new("v1").append(
                Route::new("users")
                    .append(Route::new("<id:i64>").append(
                        Route::new("posts").append(Route::new("<post_id:u64>").get(hello)),
                    )),
            ),
        );
        let tree = route.convert_to_route_tree();

        let mut req = Request::empty();
        req.set_remote(
            "127.0.0.1:8080"
                .parse::<crate::core::remote_addr::RemoteAddr>()
                .unwrap(),
        );
        *req.uri_mut() = "/api/v1/users/123/posts/456".parse().unwrap();

        let result = tree.call(req).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_route_tree_multiple_methods() {
        async fn get_handler(_: Request) -> Result<&'static str, SilentError> {
            Ok("GET")
        }
        async fn post_handler(_: Request) -> Result<&'static str, SilentError> {
            Ok("POST")
        }
        async fn put_handler(_: Request) -> Result<&'static str, SilentError> {
            Ok("PUT")
        }

        let route = Route::new("api")
            .get(get_handler)
            .post(post_handler)
            .put(put_handler);
        let tree = route.convert_to_route_tree();

        // 测试 GET
        let mut req_get = Request::empty();
        req_get.set_remote(
            "127.0.0.1:8080"
                .parse::<crate::core::remote_addr::RemoteAddr>()
                .unwrap(),
        );
        *req_get.uri_mut() = "/api".parse().unwrap();
        *req_get.method_mut() = Method::GET;
        let mut res_get = tree.call(req_get).await.unwrap();
        assert_eq!(
            res_get
                .body
                .frame()
                .await
                .unwrap()
                .unwrap()
                .data_ref()
                .unwrap(),
            &Bytes::from("GET")
        );

        // 测试 POST
        let mut req_post = Request::empty();
        req_post.set_remote(
            "127.0.0.1:8080"
                .parse::<crate::core::remote_addr::RemoteAddr>()
                .unwrap(),
        );
        *req_post.uri_mut() = "/api".parse().unwrap();
        *req_post.method_mut() = Method::POST;
        let mut res_post = tree.call(req_post).await.unwrap();
        assert_eq!(
            res_post
                .body
                .frame()
                .await
                .unwrap()
                .unwrap()
                .data_ref()
                .unwrap(),
            &Bytes::from("POST")
        );

        // 测试 PUT
        let mut req_put = Request::empty();
        req_put.set_remote(
            "127.0.0.1:8080"
                .parse::<crate::core::remote_addr::RemoteAddr>()
                .unwrap(),
        );
        *req_put.uri_mut() = "/api".parse().unwrap();
        *req_put.method_mut() = Method::PUT;
        let mut res_put = tree.call(req_put).await.unwrap();
        assert_eq!(
            res_put
                .body
                .frame()
                .await
                .unwrap()
                .unwrap()
                .data_ref()
                .unwrap(),
            &Bytes::from("PUT")
        );
    }

    // ==================== 路径参数提取测试 ====================

    #[tokio::test]
    async fn test_route_tree_extract_string_param() {
        async fn get_user(req: Request) -> Result<String, SilentError> {
            let id: String = req.get_path_params("id").unwrap();
            Ok(format!("user_id: {}", id))
        }

        let route = Route::new("users").append(Route::new("<id:str>").get(get_user));
        let tree = route.convert_to_route_tree();

        let mut req = Request::empty();
        req.set_remote(
            "127.0.0.1:8080"
                .parse::<crate::core::remote_addr::RemoteAddr>()
                .unwrap(),
        );
        *req.uri_mut() = "/users/john_doe".parse().unwrap();

        let mut res = tree.call(req).await.unwrap();
        assert_eq!(
            res.body.frame().await.unwrap().unwrap().data_ref().unwrap(),
            &Bytes::from("user_id: john_doe")
        );
    }

    #[tokio::test]
    async fn test_route_tree_extract_int_param() {
        async fn get_user_by_id(req: Request) -> Result<String, SilentError> {
            let id: i32 = req.get_path_params("id").unwrap();
            Ok(format!("user_id: {}", id))
        }

        let route = Route::new("users").append(Route::new("<id:int>").get(get_user_by_id));
        let tree = route.convert_to_route_tree();

        let mut req = Request::empty();
        req.set_remote(
            "127.0.0.1:8080"
                .parse::<crate::core::remote_addr::RemoteAddr>()
                .unwrap(),
        );
        *req.uri_mut() = "/users/12345".parse().unwrap();

        let mut res = tree.call(req).await.unwrap();
        assert_eq!(
            res.body.frame().await.unwrap().unwrap().data_ref().unwrap(),
            &Bytes::from("user_id: 12345")
        );
    }

    #[tokio::test]
    async fn test_route_tree_extract_uuid_param() {
        async fn get_by_uuid(req: Request) -> Result<String, SilentError> {
            let id: uuid::Uuid = req.get_path_params("id").unwrap();
            Ok(format!("uuid: {}", id))
        }

        let route = Route::new("items").append(Route::new("<id:uuid>").get(get_by_uuid));
        let tree = route.convert_to_route_tree();

        let uuid_str = "550e8400-e29b-41d4-a716-446655440000";
        let mut req = Request::empty();
        req.set_remote(
            "127.0.0.1:8080"
                .parse::<crate::core::remote_addr::RemoteAddr>()
                .unwrap(),
        );
        *req.uri_mut() = format!("/items/{}", uuid_str).parse().unwrap();

        let mut res = tree.call(req).await.unwrap();
        let frame = res.body.frame().await.unwrap().unwrap();
        let body = frame.data_ref().unwrap();
        assert!(body.starts_with(b"uuid:"));
    }
}
