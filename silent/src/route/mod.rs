use async_trait::async_trait;
use http::StatusCode;
pub use root::RootRoute;
pub use route_service::RouteService;
use std::collections::HashMap;
use std::fmt;
use std::sync::Arc;

use crate::handler::Handler;
#[cfg(feature = "static")]
use crate::handler::static_handler;
use crate::middleware::MiddleWareHandler;
#[cfg(feature = "static")]
use crate::prelude::HandlerGetter;
use crate::{Method, Next, Request, Response, SilentError};

pub(crate) mod handler_append;
mod handler_match;
mod root;
mod route_service;

pub trait RouterAdapt {
    fn into_router(self) -> Route;
}

#[derive(Clone)]
pub struct Route {
    pub path: String,
    pub handler: HashMap<Method, Arc<dyn Handler>>,
    pub children: Vec<Route>,
    pub middlewares: Vec<Arc<dyn MiddleWareHandler>>,
    special_match: bool,
    create_path: String,
}

impl RouterAdapt for Route {
    fn into_router(self) -> Route {
        self
    }
}

impl Default for Route {
    fn default() -> Self {
        Self::new("")
    }
}

impl fmt::Debug for Route {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fn get_route_str(pre_fix: String, route: &Route) -> String {
            let space_pre_fix = format!("    {pre_fix}");
            let mut route_strs: Vec<String> = route
                .children
                .iter()
                .filter(|r| !r.handler.is_empty() || !r.children.is_empty())
                .map(|r| get_route_str(space_pre_fix.clone(), r))
                .collect();
            if !route.handler.is_empty() || !route.children.is_empty() {
                let methods: Vec<String> = route.handler.keys().map(|m| m.to_string()).collect();
                let methods_str = if methods.is_empty() {
                    "".to_string()
                } else {
                    format!("({})", methods.join(","))
                };
                route_strs.insert(0, format!("{}{}{}", pre_fix, route.path, methods_str));
            }
            route_strs.join("\n")
        }
        write!(f, "{}", get_route_str("".to_string(), self))
    }
}

impl Route {
    pub fn new(path: &str) -> Self {
        let path = path.trim_start_matches('/');
        let mut paths = path.splitn(2, '/');
        let first_path = paths.next().unwrap_or("");
        let last_path = paths.next().unwrap_or("");
        let route = Route {
            path: first_path.to_string(),
            handler: HashMap::new(),
            children: Vec::new(),
            middlewares: Vec::new(),
            special_match: first_path.starts_with('<') && first_path.ends_with('>'),
            create_path: path.to_string(),
        };
        if last_path.is_empty() {
            route
        } else {
            route.append_route(Route::new(last_path))
        }
    }
    fn append_route(mut self, route: Route) -> Self {
        // 不再需要扩展中间件，因为我们移除了中间件传播机制
        self.children.push(route);
        self
    }
    fn get_append_real_route(&mut self, create_path: &str) -> &mut Self {
        if !create_path.contains('/') {
            self
        } else {
            let mut paths = create_path.splitn(2, '/');
            let _first_path = paths.next().unwrap_or("");
            let last_path = paths.next().unwrap_or("");
            let route = self
                .children
                .iter_mut()
                .find(|r| r.create_path == last_path);
            let route = route.unwrap();
            route.get_append_real_route(last_path)
        }
    }
    pub fn append<R: RouterAdapt>(mut self, route: R) -> Self {
        let route = route.into_router();
        let real_route = self.get_append_real_route(&self.create_path.clone());
        real_route.children.push(route);
        self
    }
    pub fn extend<R: RouterAdapt>(&mut self, route: R) {
        let route = route.into_router();
        let real_route = self.get_append_real_route(&self.create_path.clone());
        real_route.children.push(route);
    }
    pub fn hook(mut self, handler: impl MiddleWareHandler + 'static) -> Self {
        self.middlewares.push(Arc::new(handler));
        self
    }

    #[cfg(feature = "static")]
    pub fn with_static(self, path: &str) -> Self {
        self.append(
            Route::new("<path:**>").insert_handler(Method::GET, Arc::new(static_handler(path))),
        )
    }

    #[cfg(feature = "static")]
    pub fn with_static_in_url(self, url: &str, path: &str) -> Self {
        self.append(Route::new(url).with_static(path))
    }
}

#[async_trait]
impl Handler for Route {
    async fn call(&self, req: Request) -> crate::error::SilentResult<Response> {
        let configs = req.configs();
        match self.handler.get(req.method()) {
            None => Err(SilentError::business_error(
                StatusCode::METHOD_NOT_ALLOWED,
                "method not allowed".to_string(),
            )),
            Some(handler) => {
                let mut pre_res = Response::empty();
                pre_res.configs = configs;
                let mut active_middlewares = vec![];
                for middleware in self.middlewares.iter().cloned() {
                    if middleware.match_req(&req).await {
                        active_middlewares.push(middleware);
                    }
                }
                let next = Next::build(handler.clone(), active_middlewares);
                pre_res.copy_from_response(next.call(req).await?);
                Ok(pre_res)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::{Next, Request, Response};

    use super::*;

    #[derive(Clone, Eq, PartialEq)]
    struct MiddlewareTest;
    #[async_trait::async_trait]
    impl MiddleWareHandler for MiddlewareTest {
        async fn handle(&self, req: Request, next: &Next) -> crate::error::SilentResult<Response> {
            next.call(req).await
        }
    }

    #[test]
    fn middleware_tree_test() {
        let route = Route::new("api")
            .hook(MiddlewareTest {})
            .append(Route::new("test"));
        // 在新的架构中，中间件不会自动传播到子路由
        // 每个路由层级独立管理自己的中间件
        assert_eq!(route.middlewares.len(), 1); // 父路由有1个中间件
        assert_eq!(route.children[0].middlewares.len(), 0); // 子路由没有中间件
    }

    #[test]
    fn long_path_append_test() {
        let route = Route::new("api/v1")
            .hook(MiddlewareTest {})
            .append(Route::new("test"));
        assert_eq!(route.children.len(), 1);
        assert_eq!(route.children[0].children.len(), 1);
    }
}
