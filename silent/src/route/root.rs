#[cfg(feature = "cookie")]
use crate::cookie::middleware::CookieMiddleware;
use crate::middlewares::RequestTimeLogger;
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
        self.middlewares.extend(route.root_middlewares.clone());
        self.children.push(route);
    }

    pub fn hook(&mut self, handler: impl MiddleWareHandler + 'static) {
        let handler = Arc::new(handler);
        self.middlewares.push(handler.clone());
        self.children
            .iter_mut()
            .for_each(|r| r.middleware_hook(handler.clone()));
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

struct RootHandler {
    inner: RouteMatched,
    middlewares: Vec<Arc<dyn MiddleWareHandler>>,
}

#[async_trait]
impl Handler for RootHandler {
    async fn call(&self, req: Request) -> Result<Response, SilentError> {
        match self.inner.clone() {
            RouteMatched::Matched(route) => {
                let next = Next::build(Arc::new(route), self.middlewares.clone());
                next.call(req).await
            }
            RouteMatched::Unmatched => {
                let handler = |_req| async move { Err::<(), SilentError>(SilentError::NotFound) };
                let next = Next::build(
                    Arc::new(HandlerWrapper::new(handler)),
                    self.middlewares.clone(),
                );
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

        let mut root_middlewares = vec![];
        for middleware in self.middlewares.iter().cloned() {
            if middleware.match_req(&req).await {
                root_middlewares.push(middleware);
            }
        }
        let (mut req, path) = req.split_url();
        let handler = RootHandler {
            inner: self.handler_match(&mut req, &path),
            middlewares: root_middlewares,
        };
        let next = Next::build(Arc::new(handler), vec![Arc::new(RequestTimeLogger::new())]);
        next.call(req).await
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
