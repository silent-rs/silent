use async_trait::async_trait;
// RootRoute 已被 Route 替代，不再导出
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
use crate::{Method, Request, Response};

pub(crate) mod handler_append;
mod handler_match;
mod route_service;
mod route_tree;
pub(crate) use route_tree::RouteTree;
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
    // 配置管理字段（有此字段表示是服务入口点）
    configs: Option<crate::Configs>,
    #[cfg(feature = "session")]
    session_set: bool,
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
    /// 创建服务入口路由（原根路由功能）
    /// 通过设置 configs 字段来标识这是一个服务入口点
    pub fn new_root() -> Self {
        Route {
            path: String::new(),
            handler: HashMap::new(),
            children: Vec::new(),
            middlewares: Vec::new(),
            special_match: false,
            create_path: String::new(),
            configs: Some(crate::Configs::new()), // 服务入口点需要配置管理
            #[cfg(feature = "session")]
            session_set: false,
        }
    }

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
            configs: None,
            #[cfg(feature = "session")]
            session_set: false,
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
    pub fn extend<R: RouterAdapt>(&mut self, routes: Vec<R>) {
        let routes: Vec<Route> = routes.into_iter().map(|r| r.into_router()).collect();

        let real_route = self.get_append_real_route(&self.create_path.clone());
        real_route.children.extend(routes);
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

    pub fn push<R: RouterAdapt>(&mut self, route: R) {
        let route = route.into_router();
        let real_route = self.get_append_real_route(&self.create_path.clone());
        real_route.children.push(route);
    }

    pub fn hook_first(&mut self, handler: impl MiddleWareHandler + 'static) {
        let handler = Arc::new(handler);
        self.middlewares.insert(0, handler);
    }

    /// 设置配置（任何路由都可以使用）
    pub(crate) fn set_configs(&mut self, configs: Option<crate::Configs>) {
        self.configs = configs;
    }

    /// 获取配置
    pub(crate) fn get_configs(&self) -> Option<&crate::Configs> {
        self.configs.as_ref()
    }

    #[cfg(feature = "session")]
    pub fn set_session_store<S: async_session::SessionStore>(&mut self, session: S) -> &mut Self {
        self.hook_first(crate::session::middleware::SessionMiddleware::new(session));
        self.session_set = true;
        self
    }

    #[cfg(feature = "session")]
    pub fn check_session(&mut self) {
        if !self.session_set {
            self.hook_first(crate::session::middleware::SessionMiddleware::default())
        }
    }

    #[cfg(feature = "cookie")]
    pub fn check_cookie(&mut self) {
        self.hook_first(crate::cookie::middleware::CookieMiddleware::new())
    }

    #[cfg(feature = "template")]
    pub fn set_template_dir(&mut self, dir: impl Into<String>) -> &mut Self {
        let handler = crate::templates::TemplateMiddleware::new(dir.into().as_str());
        self.middlewares.push(Arc::new(handler));
        self
    }
}

// 路由执行实现

#[async_trait]
impl Handler for Route {
    async fn call(&self, mut req: Request) -> crate::error::SilentResult<Response> {
        if self.configs.is_some() {
            req.configs_mut()
                .insert(self.get_configs().unwrap().clone());
        }
        let (req, last_path) = req.split_url();
        // Route 结构已不再在服务路径上使用，保持向后兼容：
        // 直接把自身转换为 RouteTree 并调用，避免重复维护两套匹配逻辑
        let tree = self.clone().convert_to_route_tree();
        tree.call_with_path(req, last_path).await
    }
}

// RouteTree 已移动到 route_tree.rs

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
