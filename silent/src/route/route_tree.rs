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
                if segment.is_empty() {
                    return None;
                }
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
}
