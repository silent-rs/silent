use async_trait::async_trait;

use crate::core::path_param::PathParam as CorePathParam;
use crate::{Request, Response, SilentError, headers::HeaderMapExt};

use super::types::{
    Configs, Extension, Form, Json, Method, Path, Query, RemoteAddr, TypedHeader, Uri, Version,
};

/// `FromRequest` 是萃取器的核心 trait，用于从 HTTP 请求中提取特定类型的数据。
///
/// 通过实现这个 trait，您可以创建自定义的萃取器，从请求中提取任何需要的数据。
/// 所有内置萃取器（Path、Query、Json 等）都实现了这个 trait。
///
/// ## 基本用法
///
/// 要实现一个自定义萃取器，您需要：
/// 1. 定义您的数据类型
/// 2. 实现 `FromRequest` trait
/// 3. 在处理函数中使用萃取器
///
/// ## 示例：创建 JWT 令牌萃取器
///
/// ```rust
/// use async_trait::async_trait;
/// use silent::extractor::FromRequest;
/// use silent::{Request, Result, SilentError};
///
/// struct JwtToken(String);
///
/// #[async_trait]
/// impl FromRequest for JwtToken {
///     type Rejection = SilentError;
///
///     async fn from_request(req: &mut Request) -> std::result::Result<Self, Self::Rejection> {
///         let token = req.headers()
///             .get("authorization")
///             .and_then(|v| v.to_str().ok())
///             .and_then(|s| s.strip_prefix("Bearer "))
///             .map(|s| s.to_string())
///             .ok_or(SilentError::ParamsNotFound)?;
///
///         Ok(JwtToken(token))
///     }
/// }
///
/// // 使用自定义萃取器
/// async fn protected_handler(token: JwtToken) -> Result<String> {
///     Ok(format!("访问受保护的资源，Token: {}", token.0))
/// }
/// ```
///
/// ## 错误处理
///
/// `FromRequest` 的 `Rejection` 类型决定了萃取失败时的错误类型。常用的错误类型：
/// - `SilentError`：框架内置错误，包含 `ParamsNotFound`、`ParamsEmpty` 等
/// - `Response`：直接返回 HTTP 响应
///
/// ## 组合使用
///
/// 多个萃取器可以组合使用：
///
/// ```rust
/// use silent::Result;
/// use silent::extractor::{Path, Query, Json};
/// use serde::Deserialize;
///
/// #[derive(Deserialize)]
/// struct Page {
///     page: u32,
///     size: u32,
/// }
///
/// #[derive(Deserialize)]
/// struct Data {
///     name: String,
/// }
///
/// async fn handler(
///     (Path(id), Query(p), Json(data)): (Path<i64>, Query<Page>, Json<Data>),
/// ) -> Result<String> {
///     // 处理提取的数据
///     Ok("成功".to_string())
/// }
/// ```
///
/// ## 可选参数
///
/// 使用 `Option<T>` 可以处理可选参数：
///
/// ```rust
/// use silent::Result;
/// use silent::extractor::Path;
///
/// async fn handler(opt_id: Option<Path<i64>>) -> Result<String> {
///     match opt_id {
///         Some(Path(id)) => Ok(format!("ID: {}", id)),
///         None => Ok("无ID".to_string()),
///     }
/// }
/// ```
#[async_trait]
pub trait FromRequest: Sized {
    /// 萃取失败时的错误类型
    ///
    /// 这个类型必须能够转换为 HTTP 响应（实现了 `Into<Response>`）
    type Rejection: Into<crate::Response> + Send + 'static;

    /// 从请求中提取数据
    ///
    /// # 参数
    ///
    /// * `req` - 可变的请求引用，可以从中提取数据
    ///
    /// # 返回值
    ///
    /// 返回 `Result<Self, Self::Rejection>`：
    /// - 成功时返回 `Ok(extracted_value)`
    /// - 失败时返回 `Err(error)`
    ///
    /// # 示例
    ///
    /// 参见上面 `FromRequest` trait 的完整示例。
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
