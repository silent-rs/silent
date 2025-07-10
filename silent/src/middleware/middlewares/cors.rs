use crate::{Handler, MiddleWareHandler, Next, Request, Response, Result, SilentError};
use async_trait::async_trait;
use http::{Method, header};

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
            CorsOriginType::Any => "*".to_string(),
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
#[derive(Default, Debug)]
pub struct Cors {
    origin: Option<CorsOriginType>,
    methods: Option<CorsType>,
    headers: Option<CorsType>,
    credentials: Option<bool>,
    max_age: Option<u32>,
    expose: Option<CorsType>,
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
}

#[async_trait]
impl MiddleWareHandler for Cors {
    async fn handle(&self, req: Request, next: &Next) -> Result<Response> {
        let req_origin = req
            .headers()
            .get("origin")
            .map_or("", |v| v.to_str().unwrap_or(""))
            .to_string();
        let mut res = Response::empty();
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
        if let Some(ref methods) = self.methods {
            res.headers_mut().insert(
                "Access-Control-Allow-Methods",
                methods.get_value().parse().map_err(|e| {
                    SilentError::business_error(
                        http::StatusCode::INTERNAL_SERVER_ERROR,
                        format!("Cors: Failed to parse cors allow methods: {e}"),
                    )
                })?,
            );
        }
        if let Some(ref headers) = self.headers {
            res.headers_mut().insert(
                "Access-Control-Allow-Headers",
                headers.get_value().parse().map_err(|e| {
                    SilentError::business_error(
                        http::StatusCode::INTERNAL_SERVER_ERROR,
                        format!("Cors: Failed to parse cors allow headers: {e}"),
                    )
                })?,
            );
        }
        if let Some(ref credentials) = self.credentials {
            res.headers_mut().insert(
                "Access-Control-Allow-Credentials",
                credentials.to_string().parse().map_err(|e| {
                    SilentError::business_error(
                        http::StatusCode::INTERNAL_SERVER_ERROR,
                        format!("Cors: Failed to parse cors allow credentials: {e}"),
                    )
                })?,
            );
        }
        if let Some(ref max_age) = self.max_age {
            res.headers_mut().insert(
                "Access-Control-Max-Age",
                max_age.to_string().parse().map_err(|e| {
                    SilentError::business_error(
                        http::StatusCode::INTERNAL_SERVER_ERROR,
                        format!("Cors: Failed to parse cors max age: {e}"),
                    )
                })?,
            );
        }
        if let Some(ref expose) = self.expose {
            res.headers_mut().insert(
                "Access-Control-Expose-Headers",
                expose.get_value().parse().map_err(|e| {
                    SilentError::business_error(
                        http::StatusCode::INTERNAL_SERVER_ERROR,
                        format!("Cors: Failed to parse cors expose headers: {e}"),
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
