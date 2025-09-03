use async_trait::async_trait;
use http::StatusCode;
use std::collections::HashMap;
use std::sync::Arc;

use crate::core::path_param::PathParam;
use crate::handler::Handler;
use crate::middleware::MiddleWareHandler;
use crate::route::handler_match::SpecialPath;
use crate::{Method, Next, Request, Response, SilentError};

#[derive(Clone)]
pub(crate) struct RouteTree {
    pub(crate) children: Vec<RouteTree>,
    // 原先预构建的 Next 改为在调用时动态构建，以支持层级中间件
    pub(crate) handler: HashMap<Method, Arc<dyn Handler>>, // 当前结点的处理器集合
    pub(crate) middlewares: Vec<Arc<dyn MiddleWareHandler>>, // 当前结点的中间件集合
    pub(crate) configs: Option<crate::Configs>,
    pub(crate) special_match: bool,
    pub(crate) path: String,
    // 是否存在处理器（用于在子路由不匹配时回退到父路由处理器）
    pub(crate) has_handler: bool,
}

impl RouteTree {
    pub(crate) fn get_configs(&self) -> Option<&crate::Configs> {
        self.configs.as_ref()
    }

    fn split_once(path: &str) -> (&str, &str) {
        let p = path.strip_prefix('/').unwrap_or(path);
        p.split_once('/').unwrap_or((p, ""))
    }

    // 匹配当前结点：返回是否匹配以及剩余路径
    fn match_current<'p>(&self, req: &mut Request, path: &'p str) -> (bool, &'p str) {
        // 空路径（根结点）特殊处理
        if self.path.is_empty() {
            let normalized_path = if path == "/" { "" } else { path };
            if !normalized_path.is_empty() && self.children.is_empty() {
                return (false, "");
            }
            return (true, normalized_path);
        }

        let (local_path, last_path) = Self::split_once(path);

        if !self.special_match {
            // 支持节点 path 含有多段（例如 "api/v1"）
            let p = path.strip_prefix('/').unwrap_or(path);
            let node_path = self.path.as_str();
            if p == node_path {
                return (true, "");
            }
            if let Some(rem) = p.strip_prefix(node_path) {
                // 需要严格的段边界：要么完全相等，要么后面是 '/'
                if let Some(rem) = rem.strip_prefix('/') {
                    return (true, rem);
                }
            }
            (false, "")
        } else {
            match self.path.as_str().into() {
                SpecialPath::String(key) => {
                    // 必须存在实际的路径段，空字符串不匹配
                    if local_path.is_empty() {
                        (false, "")
                    } else {
                        req.set_path_params(key, local_path.to_string().into());
                        (true, last_path)
                    }
                }
                SpecialPath::Int(key) => match local_path.parse::<i32>() {
                    Ok(v) => {
                        req.set_path_params(key, v.into());
                        (true, last_path)
                    }
                    Err(_) => (false, ""),
                },
                SpecialPath::I64(key) => match local_path.parse::<i64>() {
                    Ok(v) => {
                        req.set_path_params(key, v.into());
                        (true, last_path)
                    }
                    Err(_) => (false, ""),
                },
                SpecialPath::I32(key) => match local_path.parse::<i32>() {
                    Ok(v) => {
                        req.set_path_params(key, v.into());
                        (true, last_path)
                    }
                    Err(_) => (false, ""),
                },
                SpecialPath::U64(key) => match local_path.parse::<u64>() {
                    Ok(v) => {
                        req.set_path_params(key, v.into());
                        (true, last_path)
                    }
                    Err(_) => (false, ""),
                },
                SpecialPath::U32(key) => match local_path.parse::<u32>() {
                    Ok(v) => {
                        req.set_path_params(key, v.into());
                        (true, last_path)
                    }
                    Err(_) => (false, ""),
                },
                SpecialPath::UUid(key) => match local_path.parse::<uuid::Uuid>() {
                    Ok(v) => {
                        req.set_path_params(key, v.into());
                        (true, last_path)
                    }
                    Err(_) => (false, ""),
                },
                SpecialPath::Path(key) => {
                    req.set_path_params(key, PathParam::Path(local_path.to_string()));
                    (true, last_path)
                }
                SpecialPath::FullPath(key) => {
                    // ** 通配符：记录完整剩余路径，且允许继续尝试子结点匹配。
                    // 若后续子结点无法匹配，dfs_match 中会在有处理器时回退到当前结点。
                    let p = path.strip_prefix('/').unwrap_or(path);
                    req.set_path_params(key, PathParam::Path(p.to_string()));
                    (true, last_path)
                }
            }
        }
    }

    // 旧的 DFS 收集中间件逻辑已移除
}

#[async_trait]
impl Handler for RouteTree {
    async fn call(&self, req: Request) -> crate::error::SilentResult<Response> {
        let (mut req, last_path) = req.split_url();
        if let Some(configs) = self.get_configs().cloned() {
            *req.configs_mut() = configs;
        }
        // 入口处匹配当前结点一次
        let (matched, remain) = self.match_current(&mut req, last_path.as_str());
        if !matched {
            return Err(SilentError::business_error(
                StatusCode::NOT_FOUND,
                "not found".to_string(),
            ));
        }
        self.call_with_path(req, remain.to_string()).await
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
        last_path: String,
    ) -> crate::error::SilentResult<Response> {
        // 当前结点已被匹配，合并配置
        if let Some(configs) = self.get_configs().cloned() {
            req.configs_mut().insert(configs);
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
        last_path: String,
    ) -> crate::error::SilentResult<Response> {
        if last_path.is_empty() {
            // 空路径：优先让子结点有机会处理（例如特殊路径）
            for child in &self.children {
                // 先用临时请求做预判
                let (pre_matched, rem) = child.match_current(&mut Request::empty(), "");
                if pre_matched && child.can_resolve(rem, req.method()) {
                    // 再用真实请求执行一次匹配以写入路径参数
                    let mut real_req = req;
                    let (matched, rem2) = child.match_current(&mut real_req, "");
                    debug_assert!(matched);
                    return child.call_with_path(real_req, rem2.to_string()).await;
                }
            }
            // 子结点均不可处理：若当前结点有处理器则直接调用
            return if self.has_handler && self.method_allowed(req.method()) {
                self.handler.call(req).await
            } else {
                Err(SilentError::business_error(
                    StatusCode::NOT_FOUND,
                    "not found".to_string(),
                ))
            };
        }

        // 仍有剩余路径：在兄弟结点间尝试回溯
        for child in &self.children {
            // 先用临时 Request 进行预匹配
            let (pre_matched, rem) = child.match_current(&mut Request::empty(), last_path.as_str());
            if pre_matched && child.can_resolve(rem, req.method()) {
                // 再用真实请求执行一次匹配以写入路径参数
                let mut real_req = req;
                let (matched, rem2) = child.match_current(&mut real_req, last_path.as_str());
                debug_assert!(matched);
                return child.call_with_path(real_req, rem2.to_string()).await;
            }
        }

        // 子路由未匹配：仅当当前为 **（FullPath）并且存在处理器且方法允许时可回退到当前处理器
        let is_full_path = if self.special_match {
            matches!(self.path.as_str().into(), SpecialPath::FullPath(_))
        } else {
            false
        };
        if is_full_path && self.has_handler && self.method_allowed(req.method()) {
            let handler_map = Arc::new(self.handler.clone());
            return handler_map.call(req).await;
        }

        Err(SilentError::business_error(
            StatusCode::NOT_FOUND,
            "not found".to_string(),
        ))
    }

    // 判断当前结点是否存在可处理给定 HTTP 方法的处理器（含 HEAD -> GET 回退）
    fn method_allowed(&self, method: &Method) -> bool {
        if self.handler.contains_key(method) {
            return true;
        }
        *method == Method::HEAD && self.handler.contains_key(&Method::GET)
    }

    // 仅使用路径与方法做静态解析，判断是否能够在该子树中解析到可用处理器
    fn can_resolve(&self, last_path: &str, method: &Method) -> bool {
        // 优先尝试子结点
        if !last_path.is_empty() {
            for child in &self.children {
                let (matched, rem) = child.match_current(&mut Request::empty(), last_path);
                if matched && child.can_resolve(rem, method) {
                    return true;
                }
            }
        } else {
            // 空路径：子结点也许仍可处理（如特殊路径）
            for child in &self.children {
                let (matched, rem) = child.match_current(&mut Request::empty(), last_path);
                if matched && child.can_resolve(rem, method) {
                    return true;
                }
            }
            // 子结点不行则看当前结点
            if self.has_handler && self.method_allowed(method) {
                return true;
            }
        }

        // 子路由未匹配：仅当 ** 可回退
        if !last_path.is_empty() {
            let is_full_path = if self.special_match {
                matches!(self.path.as_str().into(), SpecialPath::FullPath(_))
            } else {
                false
            };
            if is_full_path && self.has_handler && self.method_allowed(method) {
                return true;
            }
        }

        false
    }
}

// 继续匹配的端点处理器：在某结点的中间件执行完毕后，进入此处理器继续对子路由/当前处理器进行匹配与调用
struct ContinuationHandler {
    node: Arc<RouteTree>,
    last_path: String,
}

impl ContinuationHandler {
    fn new(node: Arc<RouteTree>, last_path: String) -> Self {
        Self { node, last_path }
    }
}

#[async_trait]
impl Handler for ContinuationHandler {
    async fn call(&self, req: Request) -> crate::error::SilentResult<Response> {
        self.node.call_children(req, self.last_path.clone()).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::route::Route;
    use bytes::Bytes;
    use http_body_util::BodyExt;

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
        let _ = tree.call_with_path(req, "/user/123".into()).await;
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
