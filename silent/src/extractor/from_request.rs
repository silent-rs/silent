use async_trait::async_trait;

use crate::core::path_param::PathParam as CorePathParam;
use crate::{Request, Response, SilentError, headers::HeaderMapExt};

use super::types::{
    Configs, CookieParam, Extension, Form, HeaderParam, Json, Method, Path, PathParam, Query,
    QueryParam, RemoteAddr, TypedHeader, Uri, Version,
};

#[async_trait]
pub trait FromRequest: Sized {
    type Rejection: Into<crate::Response> + Send + 'static;
    async fn from_request(req: &mut Request) -> Result<Self, Self::Rejection>;
}

#[async_trait]
impl<T> FromRequest for Path<T>
where
    for<'de> T: serde::Deserialize<'de> + Send + 'static,
{
    type Rejection = SilentError;

    async fn from_request(req: &mut Request) -> Result<Self, Self::Rejection> {
        use crate::core::serde::{from_str_map, from_str_val};
        let params = req.path_params();
        if params.is_empty() {
            return Err(SilentError::ParamsEmpty);
        }

        if params.len() == 1 {
            let value = params.values().next().unwrap();
            let single = path_param_to_string(value);
            let parsed: T = from_str_val(single.as_str())?;
            return Ok(Path(parsed));
        }

        let map_iter = params
            .iter()
            .map(|(k, v)| (k.as_str(), path_param_to_string(v)));
        let parsed: T = from_str_map(map_iter)?;
        Ok(Path(parsed))
    }
}

#[async_trait]
impl<T> FromRequest for Query<T>
where
    for<'de> T: serde::Deserialize<'de> + Send + 'static,
{
    type Rejection = SilentError;

    async fn from_request(req: &mut Request) -> Result<Self, Self::Rejection> {
        let value = req.params_parse::<T>()?;
        Ok(Query(value))
    }
}

#[async_trait]
impl<T> FromRequest for Json<T>
where
    for<'de> T: serde::Deserialize<'de> + Send + 'static,
{
    type Rejection = SilentError;

    async fn from_request(req: &mut Request) -> Result<Self, Self::Rejection> {
        let value = req.json_parse::<T>().await?;
        Ok(Json(value))
    }
}

#[async_trait]
impl<T> FromRequest for Form<T>
where
    for<'de> T: serde::Deserialize<'de> + serde::Serialize + Send + 'static,
{
    type Rejection = SilentError;

    async fn from_request(req: &mut Request) -> Result<Self, Self::Rejection> {
        let value = req.form_parse::<T>().await?;
        Ok(Form(value))
    }
}

#[async_trait]
impl<T> FromRequest for Configs<T>
where
    T: Send + Sync + Clone + 'static,
{
    type Rejection = SilentError;

    async fn from_request(req: &mut Request) -> Result<Self, Self::Rejection> {
        let cfg = req.get_config::<T>()?.clone();
        Ok(Configs(cfg))
    }
}

#[async_trait]
impl<T> FromRequest for Extension<T>
where
    T: Clone + Send + Sync + 'static,
{
    type Rejection = SilentError;

    async fn from_request(req: &mut Request) -> Result<Self, Self::Rejection> {
        let ext = req
            .extensions()
            .get::<T>()
            .cloned()
            .ok_or(SilentError::ParamsNotFound)?;
        Ok(Extension(ext))
    }
}

#[async_trait]
impl<H> FromRequest for TypedHeader<H>
where
    H: headers::Header + Send + 'static,
{
    type Rejection = SilentError;

    async fn from_request(req: &mut Request) -> Result<Self, Self::Rejection> {
        let h = req
            .headers()
            .typed_get::<H>()
            .ok_or(SilentError::ParamsNotFound)?;
        Ok(TypedHeader(h))
    }
}

#[async_trait]
impl FromRequest for Method {
    type Rejection = SilentError;
    async fn from_request(req: &mut Request) -> Result<Self, Self::Rejection> {
        Ok(Method(req.method().clone()))
    }
}

#[async_trait]
impl FromRequest for Uri {
    type Rejection = SilentError;
    async fn from_request(req: &mut Request) -> Result<Self, Self::Rejection> {
        Ok(Uri(req.uri().clone()))
    }
}

#[async_trait]
impl FromRequest for Version {
    type Rejection = SilentError;
    async fn from_request(req: &mut Request) -> Result<Self, Self::Rejection> {
        Ok(Version(req.version()))
    }
}

#[async_trait]
impl FromRequest for RemoteAddr {
    type Rejection = SilentError;
    async fn from_request(req: &mut Request) -> Result<Self, Self::Rejection> {
        Ok(RemoteAddr(req.remote()))
    }
}

#[async_trait]
impl<A> FromRequest for (A,)
where
    A: FromRequest + Send + 'static,
{
    type Rejection = Response;

    async fn from_request(req: &mut Request) -> Result<Self, Self::Rejection> {
        let a = match <A as FromRequest>::from_request(req).await {
            Ok(v) => v,
            Err(e) => return Err(e.into()),
        };
        Ok((a,))
    }
}

#[async_trait]
impl<A, B> FromRequest for (A, B)
where
    A: FromRequest + Send + 'static,
    B: FromRequest + Send + 'static,
{
    type Rejection = Response;

    async fn from_request(req: &mut Request) -> Result<Self, Self::Rejection> {
        let a = match <A as FromRequest>::from_request(req).await {
            Ok(v) => v,
            Err(e) => return Err(e.into()),
        };
        let b = match <B as FromRequest>::from_request(req).await {
            Ok(v) => v,
            Err(e) => return Err(e.into()),
        };
        Ok((a, b))
    }
}

#[async_trait]
impl<A, B, C> FromRequest for (A, B, C)
where
    A: FromRequest + Send + 'static,
    B: FromRequest + Send + 'static,
    C: FromRequest + Send + 'static,
{
    type Rejection = Response;

    async fn from_request(req: &mut Request) -> Result<Self, Self::Rejection> {
        let a = match <A as FromRequest>::from_request(req).await {
            Ok(v) => v,
            Err(e) => return Err(e.into()),
        };
        let b = match <B as FromRequest>::from_request(req).await {
            Ok(v) => v,
            Err(e) => return Err(e.into()),
        };
        let c = match <C as FromRequest>::from_request(req).await {
            Ok(v) => v,
            Err(e) => return Err(e.into()),
        };
        Ok((a, b, c))
    }
}

#[async_trait]
impl<A, B, C, D> FromRequest for (A, B, C, D)
where
    A: FromRequest + Send + 'static,
    B: FromRequest + Send + 'static,
    C: FromRequest + Send + 'static,
    D: FromRequest + Send + 'static,
{
    type Rejection = Response;

    async fn from_request(req: &mut Request) -> Result<Self, Self::Rejection> {
        let a = match <A as FromRequest>::from_request(req).await {
            Ok(v) => v,
            Err(e) => return Err(e.into()),
        };
        let b = match <B as FromRequest>::from_request(req).await {
            Ok(v) => v,
            Err(e) => return Err(e.into()),
        };
        let c = match <C as FromRequest>::from_request(req).await {
            Ok(v) => v,
            Err(e) => return Err(e.into()),
        };
        let d = match <D as FromRequest>::from_request(req).await {
            Ok(v) => v,
            Err(e) => return Err(e.into()),
        };
        Ok((a, b, c, d))
    }
}

#[async_trait]
impl<T> FromRequest for Option<T>
where
    T: FromRequest + Send + 'static,
{
    type Rejection = Response;

    async fn from_request(req: &mut Request) -> Result<Self, Self::Rejection> {
        match T::from_request(req).await {
            Ok(v) => Ok(Some(v)),
            Err(_e) => Ok(None),
        }
    }
}

#[async_trait]
impl<T> FromRequest for Result<T, Response>
where
    T: FromRequest + Send + 'static,
{
    type Rejection = Response;

    async fn from_request(req: &mut Request) -> Result<Self, Self::Rejection> {
        match T::from_request(req).await {
            Ok(v) => Ok(Ok(v)),
            Err(e) => Ok(Err(e.into())),
        }
    }
}

#[inline]
fn path_param_to_string(param: &CorePathParam) -> String {
    match param {
        CorePathParam::Str(s) | CorePathParam::Path(s) => s.as_str().to_string(),
        CorePathParam::Int(v) => v.to_string(),
        CorePathParam::Int32(v) => v.to_string(),
        CorePathParam::Int64(v) => v.to_string(),
        CorePathParam::UInt32(v) => v.to_string(),
        CorePathParam::UInt64(v) => v.to_string(),
        CorePathParam::Uuid(u) => u.to_string(),
    }
}

// ===== 单个字段萃取器实现 =====

impl QueryParam<()> {
    /// 创建查询参数萃取器上下文
    fn extract(req: &mut Request, param_name: &'static str) -> Result<String, SilentError> {
        let query = req.uri().query().unwrap_or("");
        let params: std::collections::HashMap<String, String> = serde_html_form::from_str(query)?;
        params
            .get(param_name)
            .ok_or_else(|| SilentError::ParamsNotFound)
            .cloned()
    }
}

impl PathParam<()> {
    /// 创建路径参数萃取器上下文
    fn extract(req: &mut Request, param_name: &'static str) -> Result<String, SilentError> {
        let params = req.path_params();
        let value = params
            .get(param_name)
            .ok_or_else(|| SilentError::ParamsNotFound)?;
        Ok(path_param_to_string(value))
    }
}

impl HeaderParam<()> {
    /// 创建请求头萃取器上下文
    fn extract(req: &mut Request, param_name: &'static str) -> Result<String, SilentError> {
        let headers = req.headers();
        let value = headers
            .get(param_name)
            .ok_or_else(|| SilentError::ParamsNotFound)?
            .to_str()
            .map_err(|_| SilentError::ParamsNotFound)?;
        Ok(value.to_string())
    }
}

impl CookieParam<()> {
    /// 创建 Cookie 萃取器上下文
    fn extract(req: &mut Request, param_name: &'static str) -> Result<String, SilentError> {
        let headers = req.headers();
        let cookie_header = headers
            .get("cookie")
            .ok_or_else(|| SilentError::ParamsNotFound)?
            .to_str()
            .map_err(|_| SilentError::ParamsNotFound)?;

        // 解析 Cookie 字符串
        let mut cookies = std::collections::HashMap::new();
        for part in cookie_header.split(';') {
            let mut kv = part.trim().split('=');
            if let (Some(k), Some(v)) = (kv.next(), kv.next()) {
                cookies.insert(k.to_string(), v.to_string());
            }
        }

        cookies
            .get(param_name)
            .ok_or_else(|| SilentError::ParamsNotFound)
            .cloned()
    }
}

// 实现 QueryParam<T> 的 FromRequest
#[async_trait]
impl<T> FromRequest for QueryParam<T>
where
    for<'de> T: serde::Deserialize<'de> + Send + 'static,
{
    type Rejection = SilentError;

    async fn from_request(_req: &mut Request) -> Result<Self, Self::Rejection> {
        // 注意：QueryParam 需要参数名称才能工作，但 FromRequest 不能接收额外参数
        // 这个实现只是占位符，实际使用需要通过特殊的宏或函数
        unimplemented!(
            "QueryParam<T> requires param_name. Use QueryParam::<T>::from_request_with_name(req, param_name) instead"
        )
    }
}

// 为 QueryParam<T> 实现辅助方法
impl<T> QueryParam<T> {
    /// 从请求中提取指定名称的查询参数
    pub async fn from_request_with_name(
        req: &mut Request,
        param_name: &'static str,
    ) -> Result<T, SilentError>
    where
        for<'de> T: serde::Deserialize<'de>,
    {
        let value = QueryParam::extract(req, param_name)?;
        let parsed: T = crate::core::serde::from_str_val(&value)?;
        Ok(parsed)
    }
}

// 为 PathParam<T> 实现辅助方法
impl<T> PathParam<T> {
    /// 从请求中提取指定名称的路径参数
    pub async fn from_request_with_name(
        req: &mut Request,
        param_name: &'static str,
    ) -> Result<T, SilentError>
    where
        for<'de> T: serde::Deserialize<'de>,
    {
        let value = PathParam::extract(req, param_name)?;
        let parsed: T = crate::core::serde::from_str_val(&value)?;
        Ok(parsed)
    }
}

// 为 HeaderParam<T> 实现辅助方法
impl<T> HeaderParam<T> {
    /// 从请求中提取指定名称的请求头
    pub async fn from_request_with_name(
        req: &mut Request,
        param_name: &'static str,
    ) -> Result<T, SilentError>
    where
        for<'de> T: serde::Deserialize<'de>,
    {
        let value = HeaderParam::extract(req, param_name)?;
        let parsed: T = crate::core::serde::from_str_val(&value)?;
        Ok(parsed)
    }
}

// 为 CookieParam<T> 实现辅助方法
impl<T> CookieParam<T> {
    /// 从请求中提取指定名称的 Cookie
    pub async fn from_request_with_name(
        req: &mut Request,
        param_name: &'static str,
    ) -> Result<T, SilentError>
    where
        for<'de> T: serde::Deserialize<'de>,
    {
        let value = CookieParam::extract(req, param_name)?;
        let parsed: T = crate::core::serde::from_str_val(&value)?;
        Ok(parsed)
    }
}

/// 便捷函数：创建 QueryParam 萃取器
pub async fn query_param<T>(req: &mut Request, param_name: &'static str) -> Result<T, SilentError>
where
    for<'de> T: serde::Deserialize<'de> + Send + 'static,
{
    QueryParam::<T>::from_request_with_name(req, param_name).await
}

/// 便捷函数：创建 PathParam 萃取器
pub async fn path_param<T>(req: &mut Request, param_name: &'static str) -> Result<T, SilentError>
where
    for<'de> T: serde::Deserialize<'de> + Send + 'static,
{
    PathParam::<T>::from_request_with_name(req, param_name).await
}

/// 便捷函数：创建 HeaderParam 萃取器
pub async fn header_param<T>(req: &mut Request, param_name: &'static str) -> Result<T, SilentError>
where
    for<'de> T: serde::Deserialize<'de> + Send + 'static,
{
    HeaderParam::<T>::from_request_with_name(req, param_name).await
}

/// 便捷函数：创建 CookieParam 萃取器
pub async fn cookie_param<T>(req: &mut Request, param_name: &'static str) -> Result<T, SilentError>
where
    for<'de> T: serde::Deserialize<'de> + Send + 'static,
{
    CookieParam::<T>::from_request_with_name(req, param_name).await
}
