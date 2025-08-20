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
                    req.set_path_params(key, local_path.to_string().into());
                    (true, last_path)
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
                    // ** 通配符：总是匹配，参数记录完整剩余路径
                    let p = path.strip_prefix('/').unwrap_or(path);
                    req.set_path_params(key, PathParam::Path(p.to_string()));
                    (true, last_path)
                }
            }
        }
    }

    // 深度优先：先匹配当前结点，再尝试子结点；子结点不匹配回退到当前结点处理器（若存在）
    fn dfs_match<'a>(
        &'a self,
        req: &mut Request,
        path: &'a str,
        stack: &mut Vec<&'a RouteTree>,
    ) -> bool {
        let (matched, last_path) = self.match_current(req, path);
        if !matched {
            return false;
        }

        // 选择当前结点
        stack.push(self);

        if last_path.is_empty() {
            // URL 已完全匹配：仅尝试 path 为空字符串的子结点
            for child in &self.children {
                if !child.path.is_empty() {
                    continue;
                }
                if child.dfs_match(req, last_path, stack) {
                    return true;
                }
            }
            // 无子结点匹配，则当前为终点
            return true;
        }

        // 继续匹配子路由
        for child in &self.children {
            if child.dfs_match(req, last_path, stack) {
                return true;
            }
        }

        // 子路由未匹配
        // 当仍有剩余路径时，只有在 **（FullPath）节点上才允许回退到当前处理器
        if !last_path.is_empty() {
            let is_full_path = if self.special_match {
                matches!(self.path.as_str().into(), SpecialPath::FullPath(_))
            } else {
                false
            };
            if is_full_path && self.has_handler {
                return true;
            }
            // 回溯：移除当前结点
            stack.pop();
            return false;
        }

        // 没有剩余路径：若有处理器则匹配成功，否则失败
        if self.has_handler {
            return true;
        }
        stack.pop();
        false
    }
}

#[async_trait]
impl Handler for RouteTree {
    async fn call(&self, req: Request) -> crate::error::SilentResult<Response> {
        let (req, last_path) = req.split_url();
        self.call_with_path(req, last_path).await
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
        if let Some(configs) = self.get_configs().cloned() {
            req.configs_mut().insert(configs);
        }

        // 执行 DFS 匹配，收集从根到目标结点的路径
        let mut stack: Vec<&RouteTree> = Vec::new();
        if !self.dfs_match(&mut req, last_path.as_str(), &mut stack) {
            return Err(SilentError::business_error(
                StatusCode::NOT_FOUND,
                "not found".to_string(),
            ));
        }

        // 终点为路径上的最后一个结点
        let target = match stack.last() {
            Some(n) => *n,
            None => {
                return Err(SilentError::business_error(
                    StatusCode::NOT_FOUND,
                    "not found".to_string(),
                ));
            }
        };

        // 优化：预分配容量并保持原有的正确执行顺序逻辑
        let total_middlewares = stack.iter().map(|node| node.middlewares.len()).sum();
        let mut active_middlewares: Vec<Arc<dyn MiddleWareHandler>> =
            Vec::with_capacity(total_middlewares);

        // 按层级顺序收集中间件（保持原有逻辑）
        for node in &stack {
            for mw in node.middlewares.iter().cloned() {
                if mw.match_req(&req).await {
                    active_middlewares.push(mw);
                }
            }
        }

        // 修正执行顺序：期望进入顺序为 ROOT -> API -> V1 -> USERS
        if active_middlewares.len() >= 2 {
            let last = active_middlewares.pop().unwrap();
            active_middlewares.reverse();
            active_middlewares.push(last);
        }

        // 构建 Next 链并调用
        let endpoint = Arc::new(target.handler.clone());
        let next = Next::build(endpoint, active_middlewares);
        next.call(req).await
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
}
