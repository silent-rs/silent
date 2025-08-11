use super::Route;
use crate::extractor::handler_from_extractor;
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
        let adapted = handler_from_extractor::<Args, _, _, T>(f);
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
        let adapted = handler_from_extractor::<Args, _, _, T>(f);
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
