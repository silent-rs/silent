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

    fn split_once(path: &str) -> (String, String) {
        let mut p = path.to_string();
        if p.starts_with('/') {
            p = p[1..].to_string();
        }
        p.split_once('/')
            .map(|(a, b)| (a.to_string(), b.to_string()))
            .unwrap_or((p, "".to_string()))
    }

    // 匹配当前结点：返回是否匹配以及剩余路径
    fn match_current(&self, req: &mut Request, path: &str) -> (bool, String) {
        // 空路径（根结点）特殊处理
        if self.path.is_empty() {
            let normalized_path = if path == "/" {
                "".to_string()
            } else {
                path.to_string()
            };
            if !normalized_path.is_empty() && self.children.is_empty() {
                return (false, String::new());
            }
            return (true, normalized_path);
        }

        let (local_path, last_path) = Self::split_once(path);

        if !self.special_match {
            if self.path == local_path {
                (true, last_path)
            } else {
                (false, String::new())
            }
        } else {
            match self.path.as_str().into() {
                SpecialPath::String(key) => {
                    req.set_path_params(key, local_path.into());
                    (true, last_path)
                }
                SpecialPath::Int(key) => match local_path.parse::<i32>() {
                    Ok(v) => {
                        req.set_path_params(key, v.into());
                        (true, last_path)
                    }
                    Err(_) => (false, String::new()),
                },
                SpecialPath::I64(key) => match local_path.parse::<i64>() {
                    Ok(v) => {
                        req.set_path_params(key, v.into());
                        (true, last_path)
                    }
                    Err(_) => (false, String::new()),
                },
                SpecialPath::I32(key) => match local_path.parse::<i32>() {
                    Ok(v) => {
                        req.set_path_params(key, v.into());
                        (true, last_path)
                    }
                    Err(_) => (false, String::new()),
                },
                SpecialPath::U64(key) => match local_path.parse::<u64>() {
                    Ok(v) => {
                        req.set_path_params(key, v.into());
                        (true, last_path)
                    }
                    Err(_) => (false, String::new()),
                },
                SpecialPath::U32(key) => match local_path.parse::<u32>() {
                    Ok(v) => {
                        req.set_path_params(key, v.into());
                        (true, last_path)
                    }
                    Err(_) => (false, String::new()),
                },
                SpecialPath::UUid(key) => match local_path.parse::<uuid::Uuid>() {
                    Ok(v) => {
                        req.set_path_params(key, v.into());
                        (true, last_path)
                    }
                    Err(_) => (false, String::new()),
                },
                SpecialPath::Path(key) => {
                    req.set_path_params(key, PathParam::Path(local_path));
                    (true, last_path)
                }
                SpecialPath::FullPath(key) => {
                    // ** 通配符：总是匹配，参数记录完整剩余路径
                    let mut p = path.to_string();
                    if p.starts_with('/') {
                        p = p[1..].to_string();
                    }
                    req.set_path_params(key, PathParam::Path(p));
                    (true, last_path)
                }
            }
        }
    }

    // 深度优先：先匹配当前结点，再尝试子结点；子结点不匹配回退到当前结点处理器（若存在）
    fn dfs_match<'a>(
        &'a self,
        req: &mut Request,
        path: String,
        stack: &mut Vec<&'a RouteTree>,
    ) -> bool {
        let (matched, last_path) = self.match_current(req, &path);
        if !matched {
            return false;
        }

        // 选择当前结点
        stack.push(self);

        if last_path.is_empty() {
            // 深度优先：优先尝试匹配子结点（例如空路径子路由）
            for child in &self.children {
                if child.dfs_match(req, last_path.clone(), stack) {
                    return true;
                }
            }
            // 无子结点匹配，则当前为终点
            return true;
        }

        // 继续匹配子路由
        for child in &self.children {
            if child.dfs_match(req, last_path.clone(), stack) {
                return true;
            }
        }

        // 子路由未匹配，若当前存在处理器则回退到当前
        if self.has_handler {
            true
        } else {
            // 回溯：移除当前结点
            stack.pop();
            false
        }
    }
}

#[async_trait]
impl Handler for RouteTree {
    async fn call(&self, req: Request) -> crate::error::SilentResult<Response> {
        let mut req = req;
        if let Some(configs) = self.get_configs().cloned() {
            req.configs_mut().insert(configs);
        }

        let (mut req, last_path) = req.split_url();

        // 执行 DFS 匹配，收集从根到目标结点的路径
        let mut stack: Vec<&RouteTree> = Vec::new();
        if !self.dfs_match(&mut req, last_path, &mut stack) {
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

        // 过滤可用中间件（按层级顺序）
        let mut active_middlewares: Vec<Arc<dyn MiddleWareHandler>> = Vec::new();
        for node in &stack {
            for mw in node.middlewares.iter().cloned() {
                if mw.match_req(&req).await {
                    active_middlewares.push(mw);
                }
            }
        }

        // 修正执行顺序：期望进入顺序为 ROOT -> API -> V1 -> USERS
        // Next::build 逻辑：
        // 1) 先 pop 最后一个元素，作为最内层（靠近 endpoint）
        // 2) 对剩余元素按迭代顺序 fold，最后被 fold 的成为最外层
        // 因此，只要保证“剩余元素”的反向顺序是 ROOT -> API -> V1，且最后一个被 pop 的是 USERS，
        // 就能得到期望顺序。
        // 构造方式：将除最后一个外的中间件逆序，然后把最后一个追加到末尾。
        if active_middlewares.len() >= 2 {
            let last = active_middlewares.pop().unwrap();
            active_middlewares.reverse();
            active_middlewares.push(last);
        }

        // 以目标结点的 handler 作为终端，构建 Next 链并调用
        let endpoint = Arc::new(target.handler.clone());
        let next = Next::build(endpoint, active_middlewares);
        next.call(req).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::prelude::HandlerAppend;
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
