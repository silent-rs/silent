use async_trait::async_trait;
use http::StatusCode;
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
use crate::route::handler_match::{Match, RouteMatched};
use crate::{HandlerWrapper, Method, Next, Request, Response, SilentError};

pub(crate) mod handler_append;
mod handler_match;
mod route_service;

// LayeredHandler 从 root.rs 移过来
struct LayeredHandler {
    inner: RouteMatched,
    middleware_layers: Vec<Vec<Arc<dyn MiddleWareHandler>>>,
}

#[async_trait]
impl Handler for LayeredHandler {
    async fn call(&self, req: Request) -> Result<Response, SilentError> {
        match self.inner.clone() {
            RouteMatched::Matched(route) => {
                println!("🔍 LayeredHandler - 路由匹配成功，路径: '{}', 处理器数量: {}", route.path, route.handler.len());
                // 将所有层级的中间件扁平化，按顺序执行
                let mut flattened_middlewares = vec![];
                for layer in &self.middleware_layers {
                    for middleware in layer {
                        // 检查中间件是否匹配当前请求
                        if middleware.match_req(&req).await {
                            flattened_middlewares.push(middleware.clone());
                        }
                    }
                }

                let next = Next::build(Arc::new(route), flattened_middlewares);
                next.call(req).await
            }
            RouteMatched::Unmatched => {
                println!("🔍 LayeredHandler - 路由匹配失败，返回404");
                let handler = |_req| async move { Err::<(), SilentError>(SilentError::NotFound) };

                // 对于未匹配的路由，仍然执行根级中间件（如果需要的话）
                let mut root_middlewares = vec![];
                if let Some(first_layer) = self.middleware_layers.first() {
                    for middleware in first_layer {
                        if middleware.match_req(&req).await {
                            root_middlewares.push(middleware.clone());
                        }
                    }
                }

                let next = Next::build(Arc::new(HandlerWrapper::new(handler)), root_middlewares);
                next.call(req).await
            }
        }
    }
}

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

    /// 添加子路由（原 RootRoute::push 功能）
    pub fn push(&mut self, route: Route) {
        self.children.push(route);
    }

    /// 添加中间件到当前路由首位（原 RootRoute::hook_first 功能）
    pub fn hook_first(&mut self, handler: impl MiddleWareHandler + 'static) {
        let handler = Arc::new(handler);
        self.middlewares.insert(0, handler);
    }

    /// 设置配置（任何路由都可以使用）
    pub fn set_configs(&mut self, configs: Option<crate::Configs>) {
        self.configs = configs;
    }

    /// 获取配置
    pub fn get_configs(&self) -> Option<&crate::Configs> {
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

    /// 作为服务入口点处理请求（包含路径匹配和中间件层级管理）
    async fn handle_as_service_entry(
        &self,
        mut req: Request,
    ) -> crate::error::SilentResult<Response> {
        println!("🔍 handle_as_service_entry - 开始处理请求");
        tracing::debug!("{:?}", req);
        let configs = self.configs.clone().unwrap_or_default();
        req.configs = configs.clone();

        let (mut req, path) = req.split_url();


        // 使用新的中间件收集逻辑
        let (matched_route, middleware_layers) =
            self.handler_match_collect_middlewares(&mut req, &path);
        println!("🔍 handle_as_service_entry - 路由匹配完成，中间件层数: {}", middleware_layers.len());

        match &matched_route {
            RouteMatched::Matched(route) => {
                println!("🔍 handle_as_service_entry - 路由匹配成功，路径: '{}', 处理器数量: {}, 有configs: {}",
                        route.path, route.handler.len(), route.configs.is_some());
            }
            RouteMatched::Unmatched => {
                println!("🔍 handle_as_service_entry - 路由匹配失败");
            }
        }

        // 收集根级中间件
        let mut root_middlewares = vec![];
        for middleware in self.middlewares.iter().cloned() {
            if middleware.match_req(&req).await {
                root_middlewares.push(middleware);
            }
        }

        // 将根级中间件添加到第一层
        let mut all_middleware_layers = vec![];
        if !root_middlewares.is_empty() {
            all_middleware_layers.push(root_middlewares);
        }
        all_middleware_layers.extend(middleware_layers);

        let handler = LayeredHandler {
            inner: matched_route,
            middleware_layers: all_middleware_layers,
        };

        // 直接调用 LayeredHandler
        handler.call(req).await
    }
}

#[async_trait]
impl Handler for Route {
    async fn call(&self, req: Request) -> crate::error::SilentResult<Response> {
        // 统一的路由处理逻辑
        println!("🔍 Route::call - 开始处理，路径: '{}', 有configs: {}", self.path, self.configs.is_some());

        // 如果当前路由有配置，说明是服务入口点，需要处理路径匹配和中间件层级
        if self.configs.is_some() {
            println!("🔍 Route::call - 进入服务入口处理");
            return self.handle_as_service_entry(req).await;
        }

        // 普通路由的直接处理逻辑
        let configs = req.configs();
        println!("🔍 Route::call - 路径: '{}', 方法: {:?}, 处理器数量: {}",
                self.path, req.method(), self.handler.len());
        println!("🔍 Route::call - 处理器键: {:?}", self.handler.keys().collect::<Vec<_>>());
        match self.handler.get(req.method()) {
            None => {
                println!("❌ 未找到方法 {:?} 的处理器", req.method());
                Err(SilentError::business_error(
                    StatusCode::METHOD_NOT_ALLOWED,
                    "method not allowed".to_string(),
                ))
            },
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
