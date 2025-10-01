use async_trait::async_trait;

use crate::{Request, Response, SilentError, core::path_param::PathParam, headers::HeaderMapExt};

use super::types::{
    Configs, Extension, Form, Json, Method, Path, Query, RemoteAddr, TypedHeader, Uri, Version,
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
fn path_param_to_string(param: &PathParam) -> String {
    match param {
        PathParam::Str(s) | PathParam::Path(s) => s.as_str().to_string(),
        PathParam::Int(v) => v.to_string(),
        PathParam::Int32(v) => v.to_string(),
        PathParam::Int64(v) => v.to_string(),
        PathParam::UInt32(v) => v.to_string(),
        PathParam::UInt64(v) => v.to_string(),
        PathParam::Uuid(u) => u.to_string(),
    }
}
