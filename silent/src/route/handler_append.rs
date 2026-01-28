use super::Route;
use crate::error::SilentResult;
use crate::extractor::{FromRequest, handler_from_extractor, handler_from_extractor_with_request};
use crate::handler::HandlerFn;
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

trait RouteDispatch: Sized {
    fn into_arc_handler<F, Fut>(handler: F) -> std::sync::Arc<dyn Handler>
    where
        F: Fn(Request) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Self> + Send + 'static;
}

impl RouteDispatch for Response {
    fn into_arc_handler<F, Fut>(handler: F) -> std::sync::Arc<dyn Handler>
    where
        F: Fn(Request) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Self> + Send + 'static,
    {
        HandlerFn::new(handler).arc()
    }
}

impl<T> RouteDispatch for SilentResult<T>
where
    T: Into<Response> + Send + 'static,
{
    fn into_arc_handler<F, Fut>(handler: F) -> std::sync::Arc<dyn Handler>
    where
        F: Fn(Request) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Self> + Send + 'static,
    {
        std::sync::Arc::new(HandlerWrapper::new(handler))
    }
}

impl<F, Fut> IntoRouteHandler<crate::Request> for F
where
    F: Fn(Request) -> Fut + Send + Sync + 'static,
    Fut: Future + Send + 'static,
    Fut::Output: RouteDispatch,
{
    fn into_handler(self) -> std::sync::Arc<dyn Handler> {
        <Fut::Output as RouteDispatch>::into_arc_handler(self)
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Response;
    use std::sync::Arc;

    // ==================== HandlerGetter trait 测试 ====================

    #[test]
    fn test_get_handler_mut_current_route() {
        let mut route = Route::new("test");
        route.create_path = "test".to_string();

        let handler_map = route.get_handler_mut();
        assert!(handler_map.is_empty());
    }

    #[test]
    fn test_get_handler_mut_child_route() {
        let mut route = Route::new("api");
        route.create_path = "api/test".to_string();

        // 创建一个子路由
        let child_route = Route::new("test");
        route.children.push(child_route);

        // 测试子路由的情况
        let handler_map = route.get_handler_mut();
        assert!(handler_map.is_empty());
    }

    #[test]
    fn test_insert_handler() {
        let route = Route::new("test");
        let handler = Arc::new(HandlerWrapper::new(|_req: Request| async {
            Ok(Response::text("test"))
        }));

        let route = route.insert_handler(Method::GET, handler);
        assert!(route.handler.contains_key(&Method::GET));
    }

    #[test]
    fn test_handler_method() {
        let route = Route::new("test");
        let handler = Arc::new(HandlerWrapper::new(|_req: Request| async {
            Ok(Response::text("test"))
        }));

        let route = route.handler(Method::POST, handler);
        assert!(route.handler.contains_key(&Method::POST));
    }

    // ==================== HandlerAppend trait 测试 ====================

    #[test]
    fn test_get_method() {
        let route = Route::new("test").get(|_req: Request| async { Ok("test") });

        assert!(route.handler.contains_key(&Method::GET));
    }

    #[test]
    fn test_post_method() {
        let route = Route::new("test").post(|_req: Request| async { Ok("test") });

        assert!(route.handler.contains_key(&Method::POST));
    }

    #[test]
    fn test_put_method() {
        let route = Route::new("test").put(|_req: Request| async { Ok("test") });

        assert!(route.handler.contains_key(&Method::PUT));
    }

    #[test]
    fn test_delete_method() {
        let route = Route::new("test").delete(|_req: Request| async { Ok("test") });

        assert!(route.handler.contains_key(&Method::DELETE));
    }

    #[test]
    fn test_patch_method() {
        let route = Route::new("test").patch(|_req: Request| async { Ok("test") });

        assert!(route.handler.contains_key(&Method::PATCH));
    }

    #[test]
    fn test_options_method() {
        let route = Route::new("test").options(|_req: Request| async { Ok("test") });

        assert!(route.handler.contains_key(&Method::OPTIONS));
    }

    #[test]
    fn test_multiple_methods() {
        let route = Route::new("test")
            .get(|_req: Request| async { Ok("get") })
            .post(|_req: Request| async { Ok("post") })
            .put(|_req: Request| async { Ok("put") });

        assert!(route.handler.contains_key(&Method::GET));
        assert!(route.handler.contains_key(&Method::POST));
        assert!(route.handler.contains_key(&Method::PUT));
    }

    #[test]
    fn test_handler_append_method() {
        let mut route = Route::new("test");

        route.handler_append(Method::GET, |_req: Request| async { Ok("test") });

        assert!(route.handler.contains_key(&Method::GET));
    }

    // ==================== Route Dispatch trait 测试 ====================

    #[test]
    fn test_response_into_arc_handler() {
        let handler = |_req: Request| async { Response::text("test") };
        let arc_handler = Response::into_arc_handler(handler);

        // 验证返回的是 Arc<dyn Handler>
        let _ = Arc::into_raw(arc_handler);
    }

    #[test]
    fn test_silent_result_into_arc_handler() {
        let handler = |_req: Request| async { Ok(Response::text("test")) };
        let arc_handler = <SilentResult<Response>>::into_arc_handler(handler);

        // 验证返回的是 Arc<dyn Handler>
        let _ = Arc::into_raw(arc_handler);
    }

    // ==================== IntoRouteHandler trait 测试 ====================

    #[test]
    fn test_into_handler_with_request() {
        let handler: fn(Request) -> _ = |_req: Request| async { Ok(Response::text("test")) };
        let arc_handler = handler.into_handler();

        // 验证返回的是 Arc<dyn Handler>
        let _ = Arc::into_raw(arc_handler);
    }

    #[test]
    fn test_into_handler_with_response_output() {
        let handler: fn(Request) -> _ = |_req: Request| async { Response::text("test") };
        let arc_handler = handler.into_handler();

        // 验证返回的是 Arc<dyn Handler>
        let _ = Arc::into_raw(arc_handler);
    }

    // ==================== Route 方法测试（使用 IntoRouteHandler）====================

    #[test]
    fn test_route_get_with_into_handler() {
        let route = Route::new("test").get(|_req: Request| async { Ok(Response::text("get")) });

        assert!(route.handler.contains_key(&Method::GET));
    }

    #[test]
    fn test_route_post_with_into_handler() {
        let route = Route::new("test").post(|_req: Request| async { Ok(Response::text("post")) });

        assert!(route.handler.contains_key(&Method::POST));
    }

    #[test]
    fn test_route_put_with_into_handler() {
        let route = Route::new("test").put(|_req: Request| async { Ok(Response::text("put")) });

        assert!(route.handler.contains_key(&Method::PUT));
    }

    #[test]
    fn test_route_delete_with_into_handler() {
        let route =
            Route::new("test").delete(|_req: Request| async { Ok(Response::text("delete")) });

        assert!(route.handler.contains_key(&Method::DELETE));
    }

    #[test]
    fn test_route_patch_with_into_handler() {
        let route = Route::new("test").patch(|_req: Request| async { Ok(Response::text("patch")) });

        assert!(route.handler.contains_key(&Method::PATCH));
    }

    #[test]
    fn test_route_options_with_into_handler() {
        let route =
            Route::new("test").options(|_req: Request| async { Ok(Response::text("options")) });

        assert!(route.handler.contains_key(&Method::OPTIONS));
    }

    #[test]
    fn test_route_with_response_output() {
        let route =
            Route::new("test").get(|_req: Request| async { Response::text("direct response") });

        assert!(route.handler.contains_key(&Method::GET));
    }

    // ==================== Extractor 方法测试 ====================

    // 注意：get_ex/post_ex 等方法需要 Args: FromRequest 的类型（如 Path、Query 等）
    // 由于在测试环境中创建这些萃取器类型比较复杂，这些方法主要在实际使用中测试
    // 我们通过编译验证来确保这些方法的正确性

    // ==================== Route trait 方法测试（使用 IntoRouteHandler）====================

    #[test]
    fn test_route_trait_get_with_fn_pointer() {
        // 测试 Route trait 的 get 方法（函数指针）
        async fn handler(_req: Request) -> Result<Response> {
            Ok(Response::text("test"))
        }

        let route = Route::new("test").get(handler);
        assert!(route.handler.contains_key(&Method::GET));
    }

    #[test]
    fn test_into_route_handler_trait_request() {
        // 测试 IntoRouteHandler<Request> trait 实现
        let handler: fn(Request) -> _ = |_req: Request| async { Ok(Response::text("test")) };
        let _arc_handler = handler.into_handler();

        // 验证可以创建 Arc<dyn Handler>
    }

    #[test]
    fn test_handler_wrapper_arc_conversion() {
        // 测试 HandlerWrapper 的 Arc 转换
        let wrapper = HandlerWrapper::new(|_req: Request| async { Ok("test") });
        let _arc: std::sync::Arc<dyn Handler> = std::sync::Arc::new(wrapper);
    }

    #[test]
    fn test_handler_fn_arc_conversion() {
        // 测试 HandlerFn 的 arc() 方法
        let handler_fn = HandlerFn::new(|_req: Request| async { Response::text("test") });
        let _arc = handler_fn.arc();
    }

    #[test]
    fn test_multiple_arcs_from_same_handler() {
        // 测试从同一个处理函数创建多个 Arc
        let route = Route::new("test")
            .get(|_req: Request| async { Ok("get") })
            .post(|_req: Request| async { Ok("post") });

        // 两个处理器应该可以独立克隆
        if let Some(get_handler) = route.handler.get(&Method::GET) {
            let _clone1 = get_handler.clone();
        }

        if let Some(post_handler) = route.handler.get(&Method::POST) {
            let _clone2 = post_handler.clone();
        }

        assert_eq!(route.handler.len(), 2);
    }

    #[test]
    fn test_handler_storage_type() {
        // 测试处理器存储类型
        let route = Route::new("test").get(|_req: Request| async { Ok("test") });

        if let Some(handler) = route.handler.get(&Method::GET) {
            // 验证可以转换为 Arc<dyn Handler>
            let _arc_handler: std::sync::Arc<dyn Handler> = handler.clone();
        }
    }

    #[test]
    fn test_route_trait_post_with_closure() {
        // 测试 Route trait 的 post 方法（闭包）
        let route = Route::new("test").post(|_req: Request| async { Ok("post") });
        assert!(route.handler.contains_key(&Method::POST));
    }

    #[test]
    fn test_route_trait_put() {
        // 测试 Route trait 的 put 方法
        let route = Route::new("test").put(|_req: Request| async { Ok("put") });
        assert!(route.handler.contains_key(&Method::PUT));
    }

    #[test]
    fn test_route_trait_delete() {
        // 测试 Route trait 的 delete 方法
        let route = Route::new("test").delete(|_req: Request| async { Ok("delete") });
        assert!(route.handler.contains_key(&Method::DELETE));
    }

    #[test]
    fn test_route_trait_patch() {
        // 测试 Route trait 的 patch 方法
        let route = Route::new("test").patch(|_req: Request| async { Ok("patch") });
        assert!(route.handler.contains_key(&Method::PATCH));
    }

    #[test]
    fn test_route_trait_options() {
        // 测试 Route trait 的 options 方法
        let route = Route::new("test").options(|_req: Request| async { Ok("options") });
        assert!(route.handler.contains_key(&Method::OPTIONS));
    }

    #[test]
    fn test_route_trait_methods_chain() {
        // 测试 Route trait 方法的链式调用
        let route = Route::new("test")
            .get(|_req: Request| async { Ok("get") })
            .post(|_req: Request| async { Ok("post") })
            .put(|_req: Request| async { Ok("put") })
            .delete(|_req: Request| async { Ok("delete") })
            .patch(|_req: Request| async { Ok("patch") })
            .options(|_req: Request| async { Ok("options") });

        assert_eq!(route.handler.len(), 6);
    }

    // ==================== 混合使用 HandlerAppend 和 Route trait ====================

    #[test]
    fn test_mixed_handler_append_and_route_trait() {
        // 测试混合使用 HandlerAppend trait 和 Route trait 的方法
        let route = Route::new("test")
            // HandlerAppend trait 方法
            .get(|_req: Request| async { Ok("handler append get") })
            .post(|_req: Request| async { Ok("handler append post") })
            // Route trait 方法
            .put(|_req: Request| async { Ok("route trait put") })
            .delete(|_req: Request| async { Ok("route trait delete") })
            // 更多方法
            .patch(|_req: Request| async { Ok("patch") })
            .options(|_req: Request| async { Ok("options") });

        assert_eq!(route.handler.len(), 6);
    }

    // ==================== 复杂场景测试 ====================

    #[test]
    fn test_handler_with_different_response_types() {
        // 测试不同响应类型的处理函数
        // 使用 HandlerAppend trait 的方法
        let route1 =
            Route::new("test1").get(|_req: Request| async { Ok(Response::text("ok response")) });

        let route2 =
            Route::new("test2").post(|_req: Request| async { Response::text("direct response") });

        let route3 = Route::new("test3").put(|_req: Request| async { Ok("string") });

        assert!(route1.handler.contains_key(&Method::GET));
        assert!(route2.handler.contains_key(&Method::POST));
        assert!(route3.handler.contains_key(&Method::PUT));
    }

    #[test]
    fn test_handler_clone_behavior() {
        // 测试处理器的克隆行为
        let route = Route::new("test")
            .get(|_req: Request| async { Ok("test") })
            .post(|_req: Request| async { Ok("test") });

        // 克隆处理器
        if let Some(get_handler) = route.handler.get(&Method::GET) {
            let _cloned = get_handler.clone();
        }

        if let Some(post_handler) = route.handler.get(&Method::POST) {
            let _cloned = post_handler.clone();
        }

        assert_eq!(route.handler.len(), 2);
    }

    // ==================== 边界条件测试 ====================

    #[test]
    fn test_handler_overwrite() {
        let route = Route::new("test")
            .get(|_req: Request| async { Ok("first") })
            .get(|_req: Request| async { Ok("second") });

        // 后面的 handler 应该覆盖前面的
        assert!(route.handler.contains_key(&Method::GET));
        assert_eq!(route.handler.len(), 1);
    }

    #[test]
    fn test_empty_route_handler() {
        let route = Route::new("test");
        assert!(route.handler.is_empty());
    }

    #[test]
    fn test_chain_methods() {
        let route = Route::new("test")
            .get(|_req: Request| async { Ok("get") })
            .post(|_req: Request| async { Ok("post") })
            .put(|_req: Request| async { Ok("put") })
            .delete(|_req: Request| async { Ok("delete") })
            .patch(|_req: Request| async { Ok("patch") })
            .options(|_req: Request| async { Ok("options") });

        assert_eq!(route.handler.len(), 6);
        assert!(route.handler.contains_key(&Method::GET));
        assert!(route.handler.contains_key(&Method::POST));
        assert!(route.handler.contains_key(&Method::PUT));
        assert!(route.handler.contains_key(&Method::DELETE));
        assert!(route.handler.contains_key(&Method::PATCH));
        assert!(route.handler.contains_key(&Method::OPTIONS));
    }

    #[test]
    fn test_handler_append_custom_method() {
        let mut route = Route::new("test");

        route.handler_append(Method::GET, |_req: Request| async { Ok("custom get") });

        assert!(route.handler.contains_key(&Method::GET));
    }

    // ==================== 类型验证测试 ====================

    #[test]
    fn test_handler_return_types() {
        // 测试不同的返回类型
        let route1 =
            Route::new("test1").get(|_req: Request| async { Ok(Response::text("string")) });

        let route2 =
            Route::new("test2").post(|_req: Request| async { Response::text("direct response") });

        let route3 = Route::new("test3").put(|_req: Request| async { Ok("text value") });

        assert!(route1.handler.contains_key(&Method::GET));
        assert!(route2.handler.contains_key(&Method::POST));
        assert!(route3.handler.contains_key(&Method::PUT));
    }

    #[test]
    fn test_handler_arc_storage() {
        let route = Route::new("test").get(|_req: Request| async { Ok(Response::text("test")) });

        // 验证 handler 被存储为 Arc
        if let Some(handler) = route.handler.get(&Method::GET) {
            // 检查是否可以克隆 Arc（验证它是 Arc）
            let _ = handler.clone();
        } else {
            panic!("Handler not found");
        }
    }
}
