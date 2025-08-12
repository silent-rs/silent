use super::Route;
use crate::extractor::{FromRequest, handler_from_extractor, handler_from_extractor_with_request};
use crate::{Handler, HandlerWrapper, Method, Request, Response, Result};
use std::collections::HashMap;
use std::future::Future;
use std::sync::Arc;

pub trait HandlerGetter {
    fn get_handler_mut(&mut self) -> &mut HashMap<Method, Arc<dyn Handler>>;
    fn insert_handler(self, method: Method, handler: Arc<dyn Handler>) -> Self;
    fn handler(self, method: Method, handler: Arc<dyn Handler>) -> Self;
}

pub trait HandlerAppend<F, T, Fut>: HandlerGetter
where
    Fut: Future<Output = Result<T>> + Send + 'static,
    F: Fn(Request) -> Fut + Send + Sync + 'static,
    T: Into<Response>,
{
    fn get(self, handler: F) -> Self;
    fn post(self, handler: F) -> Self;
    fn put(self, handler: F) -> Self;
    fn delete(self, handler: F) -> Self;
    fn patch(self, handler: F) -> Self;
    fn options(self, handler: F) -> Self;
    fn handler_append(&mut self, method: Method, handler: F) {
        let handler = Arc::new(HandlerWrapper::new(handler));
        let handler_map = self.get_handler_mut();
        handler_map.insert(method, handler);
    }
}

impl HandlerGetter for Route {
    fn get_handler_mut(&mut self) -> &mut HashMap<Method, Arc<dyn Handler>> {
        if self.path == self.create_path {
            &mut self.handler
        } else {
            let mut iter = self.create_path.splitn(2, '/');
            let _local_url = iter.next().unwrap_or("");
            let last_url = iter.next().unwrap_or("");
            let route = self
                .children
                .iter_mut()
                .find(|c| c.create_path == last_url)
                .unwrap();
            <Route as HandlerGetter>::get_handler_mut(route)
        }
    }
    fn insert_handler(mut self, method: Method, handler: Arc<dyn Handler>) -> Self {
        self.handler.insert(method, handler);
        self
    }

    fn handler(mut self, method: Method, handler: Arc<dyn Handler>) -> Self {
        self.get_handler_mut().insert(method, handler);
        self
    }
}

impl<F, T, Fut> HandlerAppend<F, T, Fut> for Route
where
    Fut: Future<Output = Result<T>> + Send + 'static,
    F: Fn(Request) -> Fut + Send + Sync + 'static,
    T: Into<Response>,
{
    fn get(mut self, handler: F) -> Self {
        self.handler_append(Method::GET, handler);
        self
    }

    fn post(mut self, handler: F) -> Self {
        self.handler_append(Method::POST, handler);
        self
    }

    fn put(mut self, handler: F) -> Self {
        self.handler_append(Method::PUT, handler);
        self
    }

    fn delete(mut self, handler: F) -> Self {
        self.handler_append(Method::DELETE, handler);
        self
    }

    fn patch(mut self, handler: F) -> Self {
        self.handler_append(Method::PATCH, handler);
        self
    }

    fn options(mut self, handler: F) -> Self {
        self.handler_append(Method::OPTIONS, handler);
        self
    }
}

/// 将不同形态的处理函数（基于 Request 或基于萃取器 Args）统一适配为 `Arc<dyn Handler>`
pub trait IntoRouteHandler<Args> {
    fn into_handler(self) -> std::sync::Arc<dyn Handler>;
}

impl<F, T, Fut> IntoRouteHandler<crate::Request> for F
where
    Fut: Future<Output = Result<T>> + Send + 'static,
    F: Fn(Request) -> Fut + Send + Sync + 'static,
    T: Into<Response>,
{
    fn into_handler(self) -> std::sync::Arc<dyn Handler> {
        std::sync::Arc::new(HandlerWrapper::new(self))
    }
}

impl<Args, F, Fut, T> IntoRouteHandler<Args> for F
where
    Args: FromRequest + Send + 'static,
    <Args as FromRequest>::Rejection: Into<Response> + Send + 'static,
    F: Fn(Args) -> Fut + Send + Sync + 'static,
    Fut: core::future::Future<Output = Result<T>> + Send + 'static,
    T: Into<Response> + Send + 'static,
{
    fn into_handler(self) -> std::sync::Arc<dyn Handler> {
        let adapted = handler_from_extractor::<Args, F, Fut, T>(self);
        std::sync::Arc::new(HandlerWrapper::new(adapted))
    }
}

impl<Args, F, Fut, T> IntoRouteHandler<(Request, Args)> for F
where
    Args: FromRequest + Send + 'static,
    <Args as FromRequest>::Rejection: Into<Response> + Send + 'static,
    F: Fn(Request, Args) -> Fut + Send + Sync + 'static,
    Fut: core::future::Future<Output = Result<T>> + Send + 'static,
    T: Into<Response> + Send + 'static,
{
    fn into_handler(self) -> std::sync::Arc<dyn Handler> {
        let adapted = handler_from_extractor_with_request::<Args, F, Fut, T>(self);
        std::sync::Arc::new(HandlerWrapper::new(adapted))
    }
}

impl Route {
    pub fn get<H, Args>(self, handler: H) -> Self
    where
        H: IntoRouteHandler<Args>,
    {
        let handler = handler.into_handler();
        <Route as HandlerGetter>::handler(self, Method::GET, handler)
    }

    pub fn post<H, Args>(self, handler: H) -> Self
    where
        H: IntoRouteHandler<Args>,
    {
        let handler = handler.into_handler();
        <Route as HandlerGetter>::handler(self, Method::POST, handler)
    }

    pub fn put<H, Args>(self, handler: H) -> Self
    where
        H: IntoRouteHandler<Args>,
    {
        let handler = handler.into_handler();
        <Route as HandlerGetter>::handler(self, Method::PUT, handler)
    }

    pub fn delete<H, Args>(self, handler: H) -> Self
    where
        H: IntoRouteHandler<Args>,
    {
        let handler = handler.into_handler();
        <Route as HandlerGetter>::handler(self, Method::DELETE, handler)
    }

    pub fn patch<H, Args>(self, handler: H) -> Self
    where
        H: IntoRouteHandler<Args>,
    {
        let handler = handler.into_handler();
        <Route as HandlerGetter>::handler(self, Method::PATCH, handler)
    }

    pub fn options<H, Args>(self, handler: H) -> Self
    where
        H: IntoRouteHandler<Args>,
    {
        let handler = handler.into_handler();
        <Route as HandlerGetter>::handler(self, Method::OPTIONS, handler)
    }
}

// 扩展：支持基于萃取器签名的处理函数
impl Route {
    pub fn get_ex<Args, F, Fut, T>(mut self, f: F) -> Self
    where
        Args: crate::extractor::FromRequest + Send + 'static,
        <Args as crate::extractor::FromRequest>::Rejection: Into<Response> + Send + 'static,
        F: Fn(Args) -> Fut + Send + Sync + 'static,
        Fut: core::future::Future<Output = Result<T>> + Send + 'static,
        T: Into<Response> + Send + 'static,
    {
        let adapted = handler_from_extractor::<Args, F, Fut, T>(f);
        self.handler_append(Method::GET, adapted);
        self
    }

    pub fn post_ex<Args, F, Fut, T>(mut self, f: F) -> Self
    where
        Args: crate::extractor::FromRequest + Send + 'static,
        <Args as crate::extractor::FromRequest>::Rejection: Into<Response> + Send + 'static,
        F: Fn(Args) -> Fut + Send + Sync + 'static,
        Fut: core::future::Future<Output = Result<T>> + Send + 'static,
        T: Into<Response> + Send + 'static,
    {
        let adapted = handler_from_extractor::<Args, F, Fut, T>(f);
        self.handler_append(Method::POST, adapted);
        self
    }

    pub fn put_ex<Args, F, Fut, T>(mut self, f: F) -> Self
    where
        Args: crate::extractor::FromRequest + Send + 'static,
        <Args as crate::extractor::FromRequest>::Rejection: Into<Response> + Send + 'static,
        F: Fn(Args) -> Fut + Send + Sync + 'static,
        Fut: core::future::Future<Output = Result<T>> + Send + 'static,
        T: Into<Response> + Send + 'static,
    {
        let adapted = handler_from_extractor::<Args, _, _, T>(f);
        self.handler_append(Method::PUT, adapted);
        self
    }

    pub fn delete_ex<Args, F, Fut, T>(mut self, f: F) -> Self
    where
        Args: crate::extractor::FromRequest + Send + 'static,
        <Args as crate::extractor::FromRequest>::Rejection: Into<Response> + Send + 'static,
        F: Fn(Args) -> Fut + Send + Sync + 'static,
        Fut: core::future::Future<Output = Result<T>> + Send + 'static,
        T: Into<Response> + Send + 'static,
    {
        let adapted = handler_from_extractor::<Args, _, _, T>(f);
        self.handler_append(Method::DELETE, adapted);
        self
    }

    pub fn patch_ex<Args, F, Fut, T>(mut self, f: F) -> Self
    where
        Args: crate::extractor::FromRequest + Send + 'static,
        <Args as crate::extractor::FromRequest>::Rejection: Into<Response> + Send + 'static,
        F: Fn(Args) -> Fut + Send + Sync + 'static,
        Fut: core::future::Future<Output = Result<T>> + Send + 'static,
        T: Into<Response> + Send + 'static,
    {
        let adapted = handler_from_extractor::<Args, _, _, T>(f);
        self.handler_append(Method::PATCH, adapted);
        self
    }

    pub fn options_ex<Args, F, Fut, T>(mut self, f: F) -> Self
    where
        Args: crate::extractor::FromRequest + Send + 'static,
        <Args as crate::extractor::FromRequest>::Rejection: Into<Response> + Send + 'static,
        F: Fn(Args) -> Fut + Send + Sync + 'static,
        Fut: core::future::Future<Output = Result<T>> + Send + 'static,
        T: Into<Response> + Send + 'static,
    {
        let adapted = handler_from_extractor::<Args, _, _, T>(f);
        self.handler_append(Method::OPTIONS, adapted);
        self
    }
}
