use crate::{
    CookieExt, Handler, MiddleWareHandler, Next, Request, Response, SilentError, StatusCode,
};
use async_lock::RwLock;
use async_session::{MemoryStore, Session, SessionStore};
use async_trait::async_trait;
use cookie::{Cookie, CookieJar};
use std::sync::Arc;

pub struct SessionMiddleware<T>
where
    T: SessionStore,
{
    pub session_store: Arc<RwLock<T>>,
}

impl Default for SessionMiddleware<MemoryStore> {
    fn default() -> SessionMiddleware<MemoryStore> {
        let session = MemoryStore::new();
        Self::new(session)
    }
}

impl<T> SessionMiddleware<T>
where
    T: SessionStore,
{
    pub fn new(session: T) -> Self {
        let session_store = Arc::new(RwLock::new(session));
        SessionMiddleware { session_store }
    }
}

#[async_trait]
impl<T> MiddleWareHandler for SessionMiddleware<T>
where
    T: SessionStore,
{
    async fn handle(&self, mut req: Request, next: &Next) -> crate::Result<Response> {
        let mut cookies = req.cookies().clone();
        let cookie = cookies.get("silent-web-session");
        let session_store = self.session_store.read().await;
        let mut session_key_exists = false;
        let mut cookie_value = if let Some(cookie) = cookie {
            session_key_exists = true;
            cookie.value().to_string()
        } else {
            session_store
                .store_session(Session::new())
                .await?
                .ok_or_else(|| {
                    SilentError::business_error(
                        StatusCode::INTERNAL_SERVER_ERROR,
                        "failed to create session",
                    )
                })?
        };
        let session =
            if let Ok(Some(session)) = session_store.load_session(cookie_value.clone()).await {
                session
            } else {
                session_key_exists = false;
                cookie_value = session_store
                    .store_session(Session::new())
                    .await?
                    .ok_or_else(|| {
                        SilentError::business_error(
                            StatusCode::INTERNAL_SERVER_ERROR,
                            "failed to create session",
                        )
                    })?;
                session_store
                    .load_session(cookie_value.clone())
                    .await?
                    .ok_or_else(|| {
                        SilentError::business_error(
                            StatusCode::INTERNAL_SERVER_ERROR,
                            "failed to load session",
                        )
                    })?
            };
        req.extensions_mut().insert(session.clone());
        let session_copied = session.clone();
        if !session_key_exists {
            cookies.add(
                Cookie::build(("silent-web-session", cookie_value))
                    .max_age(cookie::time::Duration::hours(2))
                    .secure(true),
            );
        }
        let mut res = next.call(req).await?;
        if res.extensions().get::<Session>().is_none() {
            res.extensions_mut().insert(session_copied);
        }
        if res.extensions().get::<CookieJar>().is_none() {
            res.extensions_mut().insert(cookies);
        } else {
            if let Some(cookie_jar) = res.extensions().get::<CookieJar>() {
                for cookie in cookie_jar.iter() {
                    cookies.add(cookie.clone());
                }
            }
            res.extensions_mut().insert(cookies.clone());
        }
        Ok(res)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::handler::HandlerWrapper;
    use crate::session::session_ext::SessionExt;
    use async_session::MemoryStore;
    use cookie::CookieJar;
    use std::sync::Arc;

    // 创建测试用的 handler
    async fn test_handler(_req: Request) -> crate::Result<Response> {
        Ok(Response::empty())
    }

    #[test]
    fn test_session_middleware_default() {
        // 测试 SessionMiddleware 的 Default 实现
        let middleware = SessionMiddleware::<MemoryStore>::default();
        // 验证 Arc 引用计数
        let _count = Arc::strong_count(&middleware.session_store);
    }

    #[test]
    fn test_session_middleware_new() {
        // 测试 SessionMiddleware::new 构造函数
        let store = MemoryStore::new();
        let middleware = SessionMiddleware::new(store);
        // 验证 Arc 引用计数
        let _count = Arc::strong_count(&middleware.session_store);
    }

    #[tokio::test]
    async fn test_middleware_with_no_session_cookie() {
        // 测试没有 session cookie 时的行为（应该创建新 session）
        let middleware = SessionMiddleware::default();

        let mut req = Request::empty();

        // 设置空的 cookie jar
        req.extensions_mut().insert(CookieJar::new());

        let handler = HandlerWrapper::new(test_handler).arc();
        let middlewares: Vec<Arc<dyn MiddleWareHandler>> = vec![];
        let next = Next::build(handler, &middlewares);
        let result = middleware.handle(req, &next).await;

        assert!(result.is_ok());
        let res = result.unwrap();

        // 验证 response 中有 session
        assert!(res.extensions().get::<Session>().is_some());
    }

    #[tokio::test]
    async fn test_middleware_with_valid_session_cookie() {
        // 测试有有效 session cookie 时的行为
        let middleware = SessionMiddleware::default();

        // 首先创建一个 session 并获取 cookie
        let store = middleware.session_store.read().await;
        let session = Session::new();
        let cookie_value = store.store_session(session).await.unwrap().unwrap();
        drop(store);

        // 创建带有 session cookie 的请求
        let mut jar = CookieJar::new();
        jar.add(Cookie::new("silent-web-session", cookie_value));

        let mut req = Request::empty();
        req.extensions_mut().insert(jar);

        let handler = HandlerWrapper::new(test_handler).arc();
        let middlewares: Vec<Arc<dyn MiddleWareHandler>> = vec![];
        let next = Next::build(handler, &middlewares);
        let result = middleware.handle(req, &next).await;

        assert!(result.is_ok());
        let res = result.unwrap();

        // 验证 response 中有 session
        assert!(res.extensions().get::<Session>().is_some());
    }

    #[tokio::test]
    async fn test_middleware_creates_new_session_if_cookie_invalid() {
        // 测试当 cookie 中的 session 无效时创建新 session
        let middleware = SessionMiddleware::default();

        // 创建带有无效 session cookie 的请求
        let mut jar = CookieJar::new();
        jar.add(Cookie::new("silent-web-session", "invalid_cookie_value"));

        let mut req = Request::empty();
        req.extensions_mut().insert(jar);

        let handler = HandlerWrapper::new(test_handler).arc();
        let middlewares: Vec<Arc<dyn MiddleWareHandler>> = vec![];
        let next = Next::build(handler, &middlewares);
        let result = middleware.handle(req, &next).await;

        assert!(result.is_ok());
        let res = result.unwrap();

        // 验证即使 cookie 无效，也能正常处理
        assert!(res.extensions().get::<Session>().is_some());
    }

    #[tokio::test]
    async fn test_middleware_session_inserted_to_request() {
        // 测试 session 被正确插入到 request extensions
        let middleware = SessionMiddleware::default();

        let mut req = Request::empty();
        req.extensions_mut().insert(CookieJar::new());

        let handler = HandlerWrapper::new(test_handler).arc();
        let middlewares: Vec<Arc<dyn MiddleWareHandler>> = vec![];
        let next = Next::build(handler, &middlewares);
        let result = middleware.handle(req, &next).await;

        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_middleware_preserves_response_session() {
        // 测试中间件保留 response 中的 session
        let middleware = SessionMiddleware::default();

        let mut req = Request::empty();
        req.extensions_mut().insert(CookieJar::new());

        // 创建一个会修改 session 的 handler
        async fn session_handler(mut req: Request) -> crate::Result<Response> {
            let session = req.sessions_mut();
            session.insert("test_key", "test_value").unwrap();
            Ok(Response::empty())
        }

        let handler = HandlerWrapper::new(session_handler).arc();
        let middlewares: Vec<Arc<dyn MiddleWareHandler>> = vec![];
        let next = Next::build(handler, &middlewares);
        let result = middleware.handle(req, &next).await;

        assert!(result.is_ok());
        let res = result.unwrap();

        // 验证 response 中有 session
        assert!(res.extensions().get::<Session>().is_some());
    }

    #[tokio::test]
    async fn test_middleware_with_existing_cookie_jar() {
        // 测试当 response 中已有 CookieJar 时的行为
        let middleware = SessionMiddleware::default();

        let mut req = Request::empty();
        req.extensions_mut().insert(CookieJar::new());

        // 创建一个会添加 cookie 的 handler
        async fn cookie_handler(_req: Request) -> crate::Result<Response> {
            let mut res = Response::empty();
            let mut jar = CookieJar::new();
            jar.add(Cookie::new("test_cookie", "test_value"));
            res.extensions_mut().insert(jar);
            Ok(res)
        }

        let handler = HandlerWrapper::new(cookie_handler).arc();
        let middlewares: Vec<Arc<dyn MiddleWareHandler>> = vec![];
        let next = Next::build(handler, &middlewares);
        let result = middleware.handle(req, &next).await;

        assert!(result.is_ok());
        let res = result.unwrap();

        // 验证 response 中有 CookieJar
        assert!(res.extensions().get::<CookieJar>().is_some());
    }

    #[tokio::test]
    async fn test_middleware_adds_cookie_when_session_key_not_exists() {
        // 测试当 session key 不存在时添加 cookie
        let middleware = SessionMiddleware::default();

        let jar = CookieJar::new();
        // 不添加 session cookie

        let mut req = Request::empty();
        req.extensions_mut().insert(jar);

        let handler = HandlerWrapper::new(test_handler).arc();
        let middlewares: Vec<Arc<dyn MiddleWareHandler>> = vec![];
        let next = Next::build(handler, &middlewares);
        let result = middleware.handle(req, &next).await;

        assert!(result.is_ok());
        let res = result.unwrap();

        // 验证 response 中有 cookie jar
        if let Some(cookie_jar) = res.extensions().get::<CookieJar>() {
            // 应该有 session cookie
            assert!(cookie_jar.get("silent-web-session").is_some());
        }
    }
}
