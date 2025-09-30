use async_trait::async_trait;
use http::StatusCode;
use memchr::memchr;
use std::collections::HashMap;
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

struct PathMatch<'a> {
    remain: &'a str,
    capture: Option<PathMatchCapture<'a>>,
}

impl<'a> PathMatch<'a> {
    fn new(remain: &'a str, capture: Option<PathMatchCapture<'a>>) -> Self {
        Self { remain, capture }
    }
}

enum PathMatchCapture<'a> {
    String(&'a str),
    Path(&'a str),
    Full(&'a str),
    I32(i32),
    I64(i64),
    U64(u64),
    U32(u32),
    Uuid(uuid::Uuid),
}

#[derive(Clone)]
pub struct RouteTree {
    pub(crate) children: Vec<RouteTree>,
    // 原先预构建的 Next 改为在调用时动态构建，以支持层级中间件
    pub(crate) handler: HashMap<Method, Arc<dyn Handler>>, // 当前结点的处理器集合
    pub(crate) middlewares: Vec<Arc<dyn MiddleWareHandler>>, // 当前结点的中间件集合
    pub(crate) configs: Option<crate::Configs>,
    pub(crate) segment: SpecialSeg,
    // 是否存在处理器（用于在子路由不匹配时回退到父路由处理器）
    pub(crate) has_handler: bool,
}

impl RouteTree {
    pub(crate) fn get_configs(&self) -> Option<&crate::Configs> {
        self.configs.as_ref()
    }

    fn strip_leading_slash(path: &str) -> &str {
        path.strip_prefix('/').unwrap_or(path)
    }

    fn strip_one_segment(path: &str) -> (&str, &str) {
        let trimmed = Self::strip_leading_slash(path);
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

        let trimmed = Self::strip_leading_slash(path);
        if trimmed == value {
            return Some("");
        }

        if trimmed.len() > value.len() && trimmed.starts_with(value) {
            let rest = &trimmed[value.len()..];
            if let Some(r) = rest.strip_prefix('/') {
                return Some(r);
            }
        }

        None
    }

    fn match_path_only<'a>(&self, path: &'a str) -> Option<PathMatch<'a>> {
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
                Self::match_static_segment(value, path).map(|remain| PathMatch::new(remain, None))
            }
            SpecialSeg::String { .. } => {
                let (segment, remain) = Self::strip_one_segment(path);
                if segment.is_empty() {
                    None
                } else {
                    Some(PathMatch::new(
                        remain,
                        Some(PathMatchCapture::String(segment)),
                    ))
                }
            }
            SpecialSeg::Int { .. } | SpecialSeg::I32 { .. } => {
                let (segment, remain) = Self::strip_one_segment(path);
                if segment.is_empty() {
                    return None;
                }
                match segment.parse::<i32>() {
                    Ok(v) => Some(PathMatch::new(remain, Some(PathMatchCapture::I32(v)))),
                    Err(_) => None,
                }
            }
            SpecialSeg::I64 { .. } => {
                let (segment, remain) = Self::strip_one_segment(path);
                if segment.is_empty() {
                    return None;
                }
                match segment.parse::<i64>() {
                    Ok(v) => Some(PathMatch::new(remain, Some(PathMatchCapture::I64(v)))),
                    Err(_) => None,
                }
            }
            SpecialSeg::U64 { .. } => {
                let (segment, remain) = Self::strip_one_segment(path);
                if segment.is_empty() {
                    return None;
                }
                match segment.parse::<u64>() {
                    Ok(v) => Some(PathMatch::new(remain, Some(PathMatchCapture::U64(v)))),
                    Err(_) => None,
                }
            }
            SpecialSeg::U32 { .. } => {
                let (segment, remain) = Self::strip_one_segment(path);
                if segment.is_empty() {
                    return None;
                }
                match segment.parse::<u32>() {
                    Ok(v) => Some(PathMatch::new(remain, Some(PathMatchCapture::U32(v)))),
                    Err(_) => None,
                }
            }
            SpecialSeg::Uuid { .. } => {
                let (segment, remain) = Self::strip_one_segment(path);
                if segment.is_empty() {
                    return None;
                }
                match uuid::Uuid::parse_str(segment) {
                    Ok(v) => Some(PathMatch::new(remain, Some(PathMatchCapture::Uuid(v)))),
                    Err(_) => None,
                }
            }
            SpecialSeg::Path { .. } => {
                let (segment, remain) = Self::strip_one_segment(path);
                Some(PathMatch::new(
                    remain,
                    Some(PathMatchCapture::Path(segment)),
                ))
            }
            SpecialSeg::FullPath { .. } => {
                let trimmed = Self::strip_leading_slash(path);
                let (_, remain) = Self::strip_one_segment(path);
                Some(PathMatch::new(
                    remain,
                    Some(PathMatchCapture::Full(trimmed)),
                ))
            }
        }
    }

    fn bind_params(&self, req: &mut Request, matched: &PathMatch<'_>) -> bool {
        match (&self.segment, &matched.capture) {
            (SpecialSeg::Root | SpecialSeg::Static(_), _) => true,
            (SpecialSeg::String { key }, Some(PathMatchCapture::String(value))) => {
                req.set_path_params(key.clone(), (*value).to_string().into());
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
            (SpecialSeg::Path { key }, Some(PathMatchCapture::Path(value))) => {
                req.set_path_params(key.clone(), PathParam::Path((*value).to_string()));
                true
            }
            (SpecialSeg::FullPath { key }, Some(PathMatchCapture::Full(value))) => {
                req.set_path_params(key.clone(), PathParam::Path((*value).to_string()));
                true
            }
            _ => false,
        }
    }
}

#[async_trait]
impl Handler for RouteTree {
    async fn call(&self, req: Request) -> crate::error::SilentResult<Response> {
        let (mut req, last_path) = req.split_url();
        if let Some(configs) = self.get_configs().cloned() {
            *req.configs_mut() = configs;
        }
        let Some(candidate) = self.match_path_only(last_path.as_str()) else {
            return Err(SilentError::business_error(
                StatusCode::NOT_FOUND,
                "not found".to_string(),
            ));
        };
        let remain = candidate.remain;
        if !self.bind_params(&mut req, &candidate) {
            return Err(SilentError::business_error(
                StatusCode::NOT_FOUND,
                "not found".to_string(),
            ));
        }
        self.call_with_path(req, remain).await
    }
}

// 优化：为Arc<RouteTree>实现Handler trait，避免不必要的克隆
#[async_trait]
impl Handler for Arc<RouteTree> {
    async fn call(&self, req: Request) -> crate::error::SilentResult<Response> {
        (**self).call(req).await
    }
}

impl RouteTree {
    pub(crate) async fn call_with_path(
        &self,
        mut req: Request,
        last_path: &str,
    ) -> crate::error::SilentResult<Response> {
        // 当前结点已被匹配，合并配置
        if let Some(configs) = self.get_configs() {
            req.configs_mut().extend_from(configs);
        }

        // 构建“继续向下匹配”的端点处理器，并按注册顺序包裹当前结点的中间件，形成真正的洋葱模型
        let endpoint: Arc<dyn Handler> =
            Arc::new(ContinuationHandler::new(Arc::new(self.clone()), last_path));
        let middlewares: Vec<Arc<dyn MiddleWareHandler>> = self.middlewares.clone();
        let next = Next::build(endpoint, middlewares);
        next.call(req).await
    }

    // 仅在当前结点已匹配后调用：尝试子路由，必要时回退到当前处理器
    async fn call_children(
        &self,
        req: Request,
        last_path: &str,
    ) -> crate::error::SilentResult<Response> {
        if last_path.is_empty() {
            for child in &self.children {
                if let Some(candidate) = child.match_path_only(last_path) {
                    if !child.path_can_resolve(candidate.remain) {
                        continue;
                    }
                    let mut real_req = req;
                    if !child.bind_params(&mut real_req, &candidate) {
                        debug_assert!(false, "unexpected path binding failure under empty branch");
                        return Err(SilentError::business_error(
                            StatusCode::NOT_FOUND,
                            "not found".to_string(),
                        ));
                    }
                    return child.call_with_path(real_req, candidate.remain).await;
                }
            }
            return if self.has_handler {
                self.handler.call(req).await
            } else {
                Err(SilentError::business_error(
                    StatusCode::NOT_FOUND,
                    "not found".to_string(),
                ))
            };
        }

        for child in &self.children {
            let Some(candidate) = child.match_path_only(last_path) else {
                continue;
            };
            if !child.path_can_resolve(candidate.remain) {
                continue;
            }
            let mut real_req = req;
            if !child.bind_params(&mut real_req, &candidate) {
                debug_assert!(false, "unexpected path binding failure");
                return Err(SilentError::business_error(
                    StatusCode::NOT_FOUND,
                    "not found".to_string(),
                ));
            }
            return child.call_with_path(real_req, candidate.remain).await;
        }

        if self.segment.is_full_path() && self.has_handler {
            return self.handler.call(req).await;
        }

        Err(SilentError::business_error(
            StatusCode::NOT_FOUND,
            "not found".to_string(),
        ))
    }

    // 仅使用路径做静态解析，判断该子树是否可能匹配（不关心方法）
    fn path_can_resolve(&self, last_path: &str) -> bool {
        if last_path.is_empty() {
            for child in &self.children {
                let Some(candidate) = child.match_path_only(last_path) else {
                    continue;
                };
                if child.path_can_resolve(candidate.remain) {
                    return true;
                }
            }
            return true;
        }

        for child in &self.children {
            let Some(candidate) = child.match_path_only(last_path) else {
                continue;
            };
            if child.path_can_resolve(candidate.remain) {
                return true;
            }
        }

        self.segment.is_full_path()
    }
}

// 继续匹配的端点处理器：在某结点的中间件执行完毕后，进入此处理器继续对子路由/当前处理器进行匹配与调用
struct ContinuationHandler {
    node: Arc<RouteTree>,
    last_path: String,
}

impl ContinuationHandler {
    fn new(node: Arc<RouteTree>, last_path: &str) -> Self {
        Self {
            node,
            last_path: last_path.to_string(),
        }
    }
}

#[async_trait]
impl Handler for ContinuationHandler {
    async fn call(&self, req: Request) -> crate::error::SilentResult<Response> {
        self.node.call_children(req, &self.last_path).await
    }
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
        let (req, _) = req.split_url();
        // trigger param parse via call
        let _ = tree.call_with_path(req, "/user/123").await;
    }

    #[tokio::test]
    async fn dfs_with_double_star_child_priority() {
        // <path:**> should capture the remaining path but allow child matching priority
        let route = Route::new("<path:**>")
            .get(hello)
            .append(Route::new("world").get(world));

        let routes = route.convert_to_route_tree();

        let mut req = Request::empty();
        req.set_remote("127.0.0.1:8080".parse().unwrap());
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
        req.set_remote("127.0.0.1:8080".parse().unwrap());
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
        req.set_remote("127.0.0.1:8080".parse().unwrap());
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
        req.set_remote("127.0.0.1:8080".parse().unwrap());
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
