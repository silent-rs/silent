#[cfg(feature = "cookie")]
use crate::cookie::middleware::CookieMiddleware;
use crate::route::Route;
use crate::route::handler_match::{Match, RouteMatched};
#[cfg(feature = "session")]
use crate::session::middleware::SessionMiddleware;
#[cfg(feature = "template")]
use crate::templates::TemplateMiddleware;
use crate::{
    Configs, Handler, HandlerWrapper, MiddleWareHandler, Next, Request, Response, SilentError,
};
#[cfg(feature = "session")]
use async_session::SessionStore;
use async_trait::async_trait;
use std::fmt;
use std::sync::Arc;

#[derive(Clone, Default)]
pub struct RootRoute {
    pub(crate) children: Vec<Route>,
    pub(crate) middlewares: Vec<Arc<dyn MiddleWareHandler>>,
    #[cfg(feature = "session")]
    pub(crate) session_set: bool,
    pub(crate) configs: Option<Configs>,
}

impl fmt::Debug for RootRoute {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let path = self
            .children
            .iter()
            .map(|route| format!("{route:?}"))
            .collect::<Vec<String>>()
            .join("\n");
        write!(f, "{path}")
    }
}

impl RootRoute {
    pub fn new() -> Self {
        Self {
            children: vec![],
            middlewares: vec![],
            #[cfg(feature = "session")]
            session_set: false,
            configs: None,
        }
    }

    pub fn push(&mut self, route: Route) {
        // 不再需要扩展中间件，因为我们移除了中间件传播机制
        self.children.push(route);
    }

    pub fn hook(&mut self, handler: impl MiddleWareHandler + 'static) {
        let handler = Arc::new(handler);
        self.middlewares.push(handler.clone());
        // 不再向子路由传播中间件
    }
    #[allow(dead_code)]
    pub(crate) fn hook_first(&mut self, handler: impl MiddleWareHandler + 'static) {
        let handler = Arc::new(handler);
        self.middlewares.insert(0, handler.clone());
    }

    pub(crate) fn set_configs(&mut self, configs: Option<Configs>) {
        self.configs = configs;
    }
}

struct LayeredHandler {
    inner: RouteMatched,
    middleware_layers: Vec<Vec<Arc<dyn MiddleWareHandler>>>,
}

#[async_trait]
impl Handler for LayeredHandler {
    async fn call(&self, req: Request) -> Result<Response, SilentError> {
        match self.inner.clone() {
            RouteMatched::Matched(route) => {
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

#[async_trait]
impl Handler for RootRoute {
    async fn call(&self, mut req: Request) -> Result<Response, SilentError> {
        tracing::debug!("{:?}", req);
        let configs = self.configs.clone().unwrap_or_default();
        req.configs = configs.clone();

        let (mut req, path) = req.split_url();

        // 使用新的中间件收集逻辑
        let (matched_route, middleware_layers) =
            self.handler_match_collect_middlewares(&mut req, &path);

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

        // 直接调用 LayeredHandler，不再额外包装
        handler.call(req).await
    }
}

impl RootRoute {
    #[cfg(feature = "session")]
    pub fn set_session_store<S: SessionStore>(&mut self, session: S) -> &mut Self {
        self.hook_first(SessionMiddleware::new(session));
        self.session_set = true;
        self
    }
    #[cfg(feature = "session")]
    pub fn check_session(&mut self) {
        if !self.session_set {
            self.hook_first(SessionMiddleware::default())
        }
    }
    #[cfg(feature = "cookie")]
    pub fn check_cookie(&mut self) {
        self.hook_first(CookieMiddleware::new())
    }

    #[cfg(feature = "template")]
    pub fn set_template_dir(&mut self, dir: impl Into<String>) -> &mut Self {
        self.hook(TemplateMiddleware::new(dir.into().as_str()));
        self
    }
}
