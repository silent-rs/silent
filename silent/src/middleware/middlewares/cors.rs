use crate::{Handler, MiddleWareHandler, Next, Request, Response, Result, SilentError};
use async_trait::async_trait;
use http::{HeaderMap, Method, header};
use std::sync::OnceLock;

#[derive(Debug)]
pub enum CorsType {
    Any,
    AllowSome(Vec<String>),
}

impl CorsType {
    fn get_value(&self) -> String {
        match self {
            CorsType::Any => "*".to_string(),
            CorsType::AllowSome(value) => value.join(","),
        }
    }
}

impl From<Vec<&str>> for CorsType {
    fn from(value: Vec<&str>) -> Self {
        CorsType::AllowSome(value.iter().map(|s| s.to_string()).collect())
    }
}

impl From<Vec<Method>> for CorsType {
    fn from(value: Vec<Method>) -> Self {
        CorsType::AllowSome(value.iter().map(|s| s.to_string()).collect())
    }
}

impl From<Vec<header::HeaderName>> for CorsType {
    fn from(value: Vec<header::HeaderName>) -> Self {
        CorsType::AllowSome(value.iter().map(|s| s.to_string()).collect())
    }
}

#[derive(Debug)]
enum CorsOriginType {
    Any,
    AllowSome(Vec<String>),
}

impl CorsOriginType {
    fn get_value(&self, origin: &str) -> String {
        match self {
            CorsOriginType::Any => origin.to_string(),
            CorsOriginType::AllowSome(value) => {
                if let Some(v) = value.iter().find(|&v| v == origin) {
                    v.to_string()
                } else {
                    "".to_string()
                }
            }
        }
    }
}

impl From<CorsType> for CorsOriginType {
    fn from(value: CorsType) -> Self {
        match value {
            CorsType::Any => CorsOriginType::Any,
            CorsType::AllowSome(value) => CorsOriginType::AllowSome(value),
        }
    }
}

impl From<&str> for CorsType {
    fn from(value: &str) -> Self {
        if value == "*" {
            CorsType::Any
        } else {
            CorsType::AllowSome(value.split(',').map(|s| s.to_string()).collect())
        }
    }
}

/// cors 中间件
/// ```rust
/// use silent::prelude::*;
/// use silent::middlewares::{Cors, CorsType};
/// // set with CorsType
/// let _ = Cors::new()
///                .origin(CorsType::Any)
///                .methods(CorsType::AllowSome(vec![Method::POST.to_string()]))
///                .headers(CorsType::AllowSome(vec![header::AUTHORIZATION.to_string(), header::ACCEPT.to_string()]))
///                .credentials(true);
/// // set with Method or header
/// let _ = Cors::new()
///                .origin(CorsType::Any)
///                .methods(vec![Method::POST])
///                .headers(vec![header::AUTHORIZATION, header::ACCEPT])
///                .credentials(true);
/// // set with str
/// let _ = Cors::new()
///                .origin("*")
///                .methods("POST")
///                .headers("authorization,accept")
///                .credentials(true);
#[derive(Debug)]
pub struct Cors {
    origin: Option<CorsOriginType>,
    methods: Option<CorsType>,
    headers: Option<CorsType>,
    credentials: Option<bool>,
    max_age: Option<u32>,
    expose: Option<CorsType>,
    // 优化：延迟初始化的响应头缓存
    cached_headers: OnceLock<HeaderMap>,
}

impl Default for Cors {
    fn default() -> Self {
        Self {
            origin: None,
            methods: None,
            headers: None,
            credentials: None,
            max_age: None,
            expose: None,
            cached_headers: OnceLock::new(),
        }
    }
}

impl Cors {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn origin<T>(mut self, origin: T) -> Self
    where
        T: Into<CorsType>,
    {
        self.origin = Some(origin.into().into());
        self
    }
    pub fn methods<T>(mut self, methods: T) -> Self
    where
        T: Into<CorsType>,
    {
        self.methods = Some(methods.into());
        self
    }
    pub fn headers<T>(mut self, headers: T) -> Self
    where
        T: Into<CorsType>,
    {
        self.headers = Some(headers.into());
        self
    }
    pub fn credentials(mut self, credentials: bool) -> Self {
        self.credentials = Some(credentials);
        self
    }
    pub fn max_age(mut self, max_age: u32) -> Self {
        self.max_age = Some(max_age);
        self
    }
    pub fn expose<T>(mut self, expose: T) -> Self
    where
        T: Into<CorsType>,
    {
        self.expose = Some(expose.into());
        self
    }

    // 优化：获取或构建静态响应头缓存
    fn get_cached_headers(&self) -> &HeaderMap {
        self.cached_headers.get_or_init(|| {
            let mut headers = HeaderMap::new();

            if let Some(ref methods) = self.methods
                && let Ok(value) = methods.get_value().parse()
            {
                headers.insert("Access-Control-Allow-Methods", value);
            }
            if let Some(ref cors_headers) = self.headers
                && let Ok(value) = cors_headers.get_value().parse()
            {
                headers.insert("Access-Control-Allow-Headers", value);
            }
            if let Some(ref credentials) = self.credentials
                && let Ok(value) = credentials.to_string().parse()
            {
                headers.insert("Access-Control-Allow-Credentials", value);
            }
            if let Some(ref max_age) = self.max_age
                && let Ok(value) = max_age.to_string().parse()
            {
                headers.insert("Access-Control-Max-Age", value);
            }
            if let Some(ref expose) = self.expose
                && let Ok(value) = expose.get_value().parse()
            {
                headers.insert("Access-Control-Expose-Headers", value);
            }

            headers
        })
    }
}

#[async_trait]
impl MiddleWareHandler for Cors {
    async fn handle(&self, req: Request, next: &Next) -> Result<Response> {
        let req_origin = req
            .headers()
            .get("origin")
            .map_or("", |v| v.to_str().unwrap_or(""))
            .to_string();

        // 如果没有 origin 一般为同源请求，直接返回
        if req_origin.is_empty() {
            return next.call(req).await;
        }

        // 优化：复用预构建的响应头模板
        let mut res = Response::empty();

        // 复制缓存的静态头部 (优化：避免重复构建)
        let cached_headers = self.get_cached_headers();
        res.headers_mut().extend(cached_headers.clone());

        // 只处理动态的 Origin 头部
        if let Some(ref origin) = self.origin {
            let origin = origin.get_value(&req_origin);
            if origin.is_empty() {
                return Err(SilentError::business_error(
                    http::StatusCode::FORBIDDEN,
                    format!("Cors: Origin \"{req_origin}\" is not allowed"),
                ));
            }
            res.headers_mut().insert(
                "Access-Control-Allow-Origin",
                origin.parse().map_err(|e| {
                    SilentError::business_error(
                        http::StatusCode::INTERNAL_SERVER_ERROR,
                        format!("Cors: Failed to parse cors allow origin: {e}"),
                    )
                })?,
            );
        }

        if req.method() == Method::OPTIONS {
            return Ok(res);
        }
        match next.call(req).await {
            Ok(result) => {
                res.copy_from_response(result);
                Ok(res)
            }
            Err(e) => {
                res.copy_from_response(e.into());
                Ok(res)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::prelude::Route;

    // ==================== CorsType 测试 ====================

    #[test]
    fn test_cors_type_any_get_value() {
        let cors_type = CorsType::Any;
        assert_eq!(cors_type.get_value(), "*");
    }

    #[test]
    fn test_cors_type_allow_some_get_value() {
        let cors_type = CorsType::AllowSome(vec!["GET".to_string(), "POST".to_string()]);
        assert_eq!(cors_type.get_value(), "GET,POST");
    }

    #[test]
    fn test_cors_type_allow_some_empty() {
        let cors_type = CorsType::AllowSome(vec![]);
        assert_eq!(cors_type.get_value(), "");
    }

    #[test]
    fn test_cors_type_from_vec_str() {
        let cors_type: CorsType = vec!["GET", "POST"].into();
        match cors_type {
            CorsType::AllowSome(ref v) => {
                assert_eq!(v, &["GET".to_string(), "POST".to_string()]);
            }
            _ => panic!("Expected AllowSome"),
        }
    }

    #[test]
    fn test_cors_type_from_vec_method() {
        let methods = vec![Method::GET, Method::POST];
        let cors_type: CorsType = methods.into();
        match cors_type {
            CorsType::AllowSome(ref v) => {
                assert_eq!(v, &["GET".to_string(), "POST".to_string()]);
            }
            _ => panic!("Expected AllowSome"),
        }
    }

    #[test]
    fn test_cors_type_from_vec_header_name() {
        let headers = vec![header::AUTHORIZATION, header::ACCEPT];
        let cors_type: CorsType = headers.into();
        match cors_type {
            CorsType::AllowSome(ref v) => {
                assert_eq!(v, &["authorization".to_string(), "accept".to_string()]);
            }
            _ => panic!("Expected AllowSome"),
        }
    }

    #[test]
    fn test_cors_type_from_str_any() {
        let cors_type: CorsType = "*".into();
        assert!(matches!(cors_type, CorsType::Any));
    }

    #[test]
    fn test_cors_type_from_str_multiple() {
        let cors_type: CorsType = "GET,POST,PUT".into();
        match cors_type {
            CorsType::AllowSome(ref v) => {
                assert_eq!(
                    v,
                    &["GET".to_string(), "POST".to_string(), "PUT".to_string()]
                );
            }
            _ => panic!("Expected AllowSome"),
        }
    }

    // ==================== CorsOriginType 测试 ====================

    #[test]
    fn test_cors_origin_type_any_get_value() {
        let origin_type = CorsOriginType::Any;
        assert_eq!(
            origin_type.get_value("http://example.com"),
            "http://example.com"
        );
    }

    #[test]
    fn test_cors_origin_type_allow_some_match() {
        let origin_type = CorsOriginType::AllowSome(vec![
            "http://example.com".to_string(),
            "http://localhost:8080".to_string(),
        ]);
        assert_eq!(
            origin_type.get_value("http://example.com"),
            "http://example.com"
        );
    }

    #[test]
    fn test_cors_origin_type_allow_some_no_match() {
        let origin_type = CorsOriginType::AllowSome(vec!["http://example.com".to_string()]);
        assert_eq!(origin_type.get_value("http://evil.com"), "");
    }

    #[test]
    fn test_cors_origin_type_from_cors_type_any() {
        let cors_type = CorsType::Any;
        let origin_type: CorsOriginType = cors_type.into();
        assert!(matches!(origin_type, CorsOriginType::Any));
    }

    #[test]
    fn test_cors_origin_type_from_cors_type_allow_some() {
        let cors_type = CorsType::AllowSome(vec!["http://example.com".to_string()]);
        let origin_type: CorsOriginType = cors_type.into();
        match origin_type {
            CorsOriginType::AllowSome(ref v) => {
                assert_eq!(v, &["http://example.com".to_string()]);
            }
            _ => panic!("Expected AllowSome"),
        }
    }

    // ==================== Cors 结构体构造测试 ====================

    #[test]
    fn test_cors_new() {
        let cors = Cors::new();
        assert!(cors.origin.is_none());
        assert!(cors.methods.is_none());
        assert!(cors.headers.is_none());
        assert!(cors.credentials.is_none());
        assert!(cors.max_age.is_none());
        assert!(cors.expose.is_none());
    }

    #[test]
    fn test_cors_default() {
        let cors = Cors::default();
        assert!(cors.origin.is_none());
        assert!(cors.methods.is_none());
        assert!(cors.headers.is_none());
    }

    #[test]
    fn test_cors_origin_any() {
        let cors = Cors::new().origin(CorsType::Any);
        assert!(matches!(cors.origin, Some(CorsOriginType::Any)));
    }

    #[test]
    fn test_cors_origin_str() {
        let cors = Cors::new().origin("http://example.com");
        match cors.origin {
            Some(CorsOriginType::AllowSome(ref v)) => {
                assert_eq!(v, &["http://example.com".to_string()]);
            }
            _ => panic!("Expected AllowSome"),
        }
    }

    #[test]
    fn test_cors_methods() {
        let cors = Cors::new().methods(vec![Method::GET, Method::POST]);
        match cors.methods {
            Some(CorsType::AllowSome(ref v)) => {
                assert_eq!(v, &["GET".to_string(), "POST".to_string()]);
            }
            _ => panic!("Expected AllowSome"),
        }
    }

    #[test]
    fn test_cors_headers() {
        let cors = Cors::new().headers(vec![header::AUTHORIZATION, header::ACCEPT]);
        match cors.headers {
            Some(CorsType::AllowSome(ref v)) => {
                assert_eq!(v, &["authorization".to_string(), "accept".to_string()]);
            }
            _ => panic!("Expected AllowSome"),
        }
    }

    #[test]
    fn test_cors_credentials() {
        let cors = Cors::new().credentials(true);
        assert_eq!(cors.credentials, Some(true));

        let cors = Cors::new().credentials(false);
        assert_eq!(cors.credentials, Some(false));
    }

    #[test]
    fn test_cors_max_age() {
        let cors = Cors::new().max_age(3600);
        assert_eq!(cors.max_age, Some(3600));
    }

    #[test]
    fn test_cors_expose() {
        let cors = Cors::new().expose("Content-Length,X-Custom-Header");
        match cors.expose {
            Some(CorsType::AllowSome(ref v)) => {
                assert_eq!(
                    v,
                    &["Content-Length".to_string(), "X-Custom-Header".to_string()]
                );
            }
            _ => panic!("Expected AllowSome"),
        }
    }

    #[test]
    fn test_cors_builder_chain() {
        let cors = Cors::new()
            .origin(CorsType::Any)
            .methods(vec![Method::GET])
            .headers(vec![header::ACCEPT])
            .credentials(true)
            .max_age(3600)
            .expose("Content-Length");

        assert!(matches!(cors.origin, Some(CorsOriginType::Any)));
        assert!(cors.methods.is_some());
        assert!(cors.headers.is_some());
        assert_eq!(cors.credentials, Some(true));
        assert_eq!(cors.max_age, Some(3600));
        assert!(cors.expose.is_some());
    }

    // ==================== get_cached_headers 测试 ====================

    #[test]
    fn test_get_cached_headers_with_methods() {
        let cors = Cors::new().methods(vec![Method::GET, Method::POST]);
        let headers = cors.get_cached_headers();

        assert_eq!(
            headers.get("Access-Control-Allow-Methods"),
            Some(&"GET,POST".parse().unwrap())
        );
    }

    #[test]
    fn test_get_cached_headers_with_headers() {
        let cors = Cors::new().headers("authorization,accept");
        let headers = cors.get_cached_headers();

        assert_eq!(
            headers.get("Access-Control-Allow-Headers"),
            Some(&"authorization,accept".parse().unwrap())
        );
    }

    #[test]
    fn test_get_cached_headers_with_credentials() {
        let cors = Cors::new().credentials(true);
        let headers = cors.get_cached_headers();

        assert_eq!(
            headers.get("Access-Control-Allow-Credentials"),
            Some(&"true".parse().unwrap())
        );
    }

    #[test]
    fn test_get_cached_headers_with_max_age() {
        let cors = Cors::new().max_age(3600);
        let headers = cors.get_cached_headers();

        assert_eq!(
            headers.get("Access-Control-Max-Age"),
            Some(&"3600".parse().unwrap())
        );
    }

    #[test]
    fn test_get_cached_headers_with_expose() {
        let cors = Cors::new().expose("Content-Length");
        let headers = cors.get_cached_headers();

        assert_eq!(
            headers.get("Access-Control-Expose-Headers"),
            Some(&"Content-Length".parse().unwrap())
        );
    }

    #[test]
    fn test_get_cached_headers_combined() {
        let cors = Cors::new()
            .methods("GET,POST")
            .headers("authorization")
            .credentials(true)
            .max_age(3600);
        let headers = cors.get_cached_headers();

        assert!(headers.contains_key("Access-Control-Allow-Methods"));
        assert!(headers.contains_key("Access-Control-Allow-Headers"));
        assert!(headers.contains_key("Access-Control-Allow-Credentials"));
        assert!(headers.contains_key("Access-Control-Max-Age"));
    }

    // ==================== 集成测试 ====================

    #[tokio::test]
    async fn test_cors_integration() {
        let route = Route::new("/")
            .hook(Cors::new().origin(CorsType::Any))
            .get(|_req: Request| async { Ok("hello world") });
        let route = Route::new_root().append(route);
        let mut req = Request::empty();
        *req.method_mut() = Method::OPTIONS;
        *req.uri_mut() = "http://localhost:8080/".parse().unwrap();
        req.headers_mut()
            .insert("origin", "http://localhost:8080".parse().unwrap());
        req.headers_mut()
            .insert("access-control-request-method", "GET".parse().unwrap());
        req.headers_mut().insert(
            "access-control-request-headers",
            "content-type".parse().unwrap(),
        );
        let res = route.call(req).await.unwrap();
        assert_eq!(res.status, http::StatusCode::OK);
    }

    #[tokio::test]
    async fn test_cors_with_post_request() {
        let route = Route::new("/")
            .hook(
                Cors::new()
                    .origin("http://localhost:8080")
                    .methods(vec![Method::GET, Method::POST])
                    .credentials(true),
            )
            .post(|_req: Request| async { Ok("posted") });
        let route = Route::new_root().append(route);

        let mut req = Request::empty();
        *req.method_mut() = Method::POST;
        *req.uri_mut() = "http://localhost:8080/".parse().unwrap();
        req.headers_mut()
            .insert("origin", "http://localhost:8080".parse().unwrap());

        let res = route.call(req).await.unwrap();
        assert_eq!(res.status, http::StatusCode::OK);
        assert!(res.headers().contains_key("Access-Control-Allow-Origin"));
        assert!(
            res.headers()
                .contains_key("Access-Control-Allow-Credentials")
        );
    }

    // ==================== 边界条件测试 ====================

    #[test]
    fn test_cors_type_empty_methods() {
        let cors_type = CorsType::AllowSome(vec![]);
        assert_eq!(cors_type.get_value(), "");
    }

    #[test]
    fn test_cors_origin_empty_list() {
        let origin_type = CorsOriginType::AllowSome(vec![]);
        assert_eq!(origin_type.get_value("http://example.com"), "");
    }

    #[tokio::test]
    async fn test_handle_without_origin_header() {
        // 测试同源请求（没有 origin header）
        let route = Route::new("/")
            .hook(Cors::new().origin(CorsType::Any))
            .get(|_req: Request| async { Ok("hello") });
        let route = Route::new_root().append(route);

        let mut req = Request::empty();
        *req.method_mut() = Method::GET;
        *req.uri_mut() = "http://localhost:8080/".parse().unwrap();
        // 不添加 origin header

        let res = route.call(req).await.unwrap();
        // 没有 origin 应该正常返回（同源请求）
        assert_eq!(res.status, http::StatusCode::OK);
    }

    #[tokio::test]
    async fn test_handle_empty_string_origin() {
        // 测试空字符串 origin
        let route = Route::new("/")
            .hook(Cors::new().origin("http://example.com"))
            .get(|_req: Request| async { Ok("hello") });
        let route = Route::new_root().append(route);

        let mut req = Request::empty();
        *req.method_mut() = Method::GET;
        *req.uri_mut() = "http://localhost:8080/".parse().unwrap();
        req.headers_mut().insert("origin", "".parse().unwrap());

        let res = route.call(req).await.unwrap();
        // 空 origin 应该被视为同源请求
        assert_eq!(res.status, http::StatusCode::OK);
    }
}
