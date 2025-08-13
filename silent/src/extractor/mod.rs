use async_trait::async_trait;

use crate::{
    Method as HttpMethod, Request, Response, SilentError, core::path_param::PathParam,
    headers::HeaderMapExt,
};
use futures_util::future::BoxFuture;
use http::{Uri as HttpUri, Version as HttpVersion};
use std::sync::Arc;

/// 统一的请求萃取器 trait
#[async_trait]
pub trait FromRequest: Sized {
    type Rejection: Into<crate::Response> + Send + 'static;
    async fn from_request(req: &mut Request) -> Result<Self, Self::Rejection>;
}

/// Path 萃取器：支持从路径参数中解析到单值或结构体
/// - 单值：当仅有一个路径参数时，使用 from_str_val 解析到目标类型
/// - 结构体：当存在多个路径参数时，按字段名匹配填充
pub struct Path<T>(pub T);

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
            // 尝试用单值解析（支持数值、bool、字符串、枚举等）
            let single = path_param_to_string(value);
            let parsed: T = from_str_val(single.as_str())?;
            return Ok(Path(parsed));
        }

        // 多键参数，按键名组装 map 解析为结构体
        let map_iter = params
            .iter()
            .map(|(k, v)| (k.as_str(), path_param_to_string(v)));
        // from_str_map 需要值实现 Into<Cow<'_, str>>，这里传 String 即可
        let parsed: T = from_str_map(map_iter)?;
        Ok(Path(parsed))
    }
}

/// Query 萃取器：从 URL 查询参数解析为 T
pub struct Query<T>(pub T);

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

/// Json 萃取器：从 application/json 解析为 T（带缓存）
pub struct Json<T>(pub T);

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

/// Form 萃取器：从表单解析为 T
pub struct Form<T>(pub T);

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

/// Request 便捷扩展：通用萃取
#[async_trait]
pub trait RequestExt {
    async fn extract<T>(&mut self) -> Result<T, T::Rejection>
    where
        T: FromRequest + Send + 'static;
}

#[async_trait]
impl RequestExt for Request {
    async fn extract<T>(&mut self) -> Result<T, T::Rejection>
    where
        T: FromRequest + Send + 'static,
    {
        T::from_request(self).await
    }
}

// tuple extractors
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

/// 将使用萃取器参数的处理函数适配为接收 `Request` 的处理函数
/// 仅萃取器参数的处理函数：`F: Fn(Args) -> Fut`
pub fn handler_from_extractor<Args, F, Fut, T>(
    f: F,
) -> impl Fn(crate::Request) -> BoxFuture<'static, crate::Result<Response>> + Send + Sync + 'static
where
    Args: FromRequest + Send + 'static,
    <Args as FromRequest>::Rejection: Into<Response> + Send + 'static,
    F: Fn(Args) -> Fut + Send + Sync + 'static,
    Fut: core::future::Future<Output = crate::Result<T>> + Send + 'static,
    T: Into<Response> + Send + 'static,
{
    let f = Arc::new(f);
    move |mut req: Request| {
        let f = f.clone();
        Box::pin(async move {
            match <Args as FromRequest>::from_request(&mut req).await {
                Ok(args) => {
                    let res = f(args).await?;
                    Ok(res.into())
                }
                Err(rej) => Ok(rej.into()),
            }
        })
    }
}

/// 同时接收 Request 与萃取器参数：`F: Fn(Request, Args) -> Fut`
pub fn handler_from_extractor_with_request<Args, F, Fut, T>(
    f: F,
) -> impl Fn(crate::Request) -> BoxFuture<'static, crate::Result<Response>> + Send + Sync + 'static
where
    Args: FromRequest + Send + 'static,
    <Args as FromRequest>::Rejection: Into<Response> + Send + 'static,
    F: Fn(crate::Request, Args) -> Fut + Send + Sync + 'static,
    Fut: core::future::Future<Output = crate::Result<T>> + Send + 'static,
    T: Into<Response> + Send + 'static,
{
    let f = Arc::new(f);
    move |mut req: Request| {
        let f = f.clone();
        Box::pin(async move {
            match <Args as FromRequest>::from_request(&mut req).await {
                Ok(args) => {
                    let res = f(req, args).await?;
                    Ok(res.into())
                }
                Err(rej) => Ok(rej.into()),
            }
        })
    }
}

#[inline]
fn path_param_to_string(param: &PathParam) -> String {
    match param {
        PathParam::String(s) => s.clone(),
        PathParam::Path(s) => s.clone(),
        PathParam::Int(v) => v.to_string(),
        PathParam::Int32(v) => v.to_string(),
        PathParam::Int64(v) => v.to_string(),
        PathParam::UInt32(v) => v.to_string(),
        PathParam::UInt64(v) => v.to_string(),
        PathParam::Uuid(u) => u.to_string(),
    }
}

#[allow(dead_code)]
pub struct Configs<T>(pub T);

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

/// 从 Extensions 中提取扩展
pub struct Extension<T>(pub T);

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

/// 头部类型化提取（等价 axum 的 TypedHeader）
pub struct TypedHeader<H>(pub H);

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

#[allow(dead_code)]
pub struct Method(pub HttpMethod);
pub struct Uri(pub HttpUri);
pub struct Version(pub HttpVersion);
pub struct RemoteAddr(pub crate::core::socket_addr::SocketAddr);

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Request, Response};
    use headers::UserAgent;
    use serde::Deserialize;

    #[derive(Deserialize)]
    struct Page {
        page: u32,
        size: u32,
    }

    #[tokio::test]
    async fn test_path_single_and_struct() {
        // single value
        let mut req = Request::empty();
        req.set_path_params(
            "id".to_owned(),
            crate::core::path_param::PathParam::Int64(42),
        );
        let Path(id): Path<i64> = Path::from_request(&mut req).await.unwrap();
        assert_eq!(id, 42);

        // struct
        let mut req = Request::empty();
        req.set_path_params(
            "id".to_owned(),
            crate::core::path_param::PathParam::Int64(7),
        );
        req.set_path_params(
            "name".to_owned(),
            crate::core::path_param::PathParam::String("bob".into()),
        );
        #[derive(Deserialize)]
        struct U {
            id: i64,
            name: String,
        }
        let Path(u): Path<U> = Path::from_request(&mut req).await.unwrap();
        assert_eq!(u.id, 7);
        assert_eq!(u.name, "bob");
    }

    #[tokio::test]
    async fn test_query_and_json_and_form() {
        // query
        let mut req = Request::empty();
        *req.uri_mut() = http::Uri::from_static("http://localhost/test?page=1&size=20");
        let Query(p): Query<Page> = Query::from_request(&mut req).await.unwrap();
        assert_eq!(p.page, 1);
        assert_eq!(p.size, 20);

        // json
        #[derive(Deserialize, serde::Serialize)]
        struct U {
            name: String,
        }
        let mut req = Request::empty();
        req.headers_mut().insert(
            "content-type",
            http::HeaderValue::from_static("application/json"),
        );
        req.replace_body(crate::core::req_body::ReqBody::Once(
            serde_json::to_vec(&U {
                name: "alice".into(),
            })
            .unwrap()
            .into(),
        ));
        let Json(u): Json<U> = Json::from_request(&mut req).await.unwrap();
        assert_eq!(u.name, "alice");
    }

    #[tokio::test]
    async fn test_tuple_and_option_result() {
        // tuple
        let mut req = Request::empty();
        req.set_path_params("id".to_owned(), crate::core::path_param::PathParam::Int(1));
        *req.uri_mut() = http::Uri::from_static("http://localhost/test?page=2&size=3");
        let (_a, _b): (Path<i32>, Query<Page>) =
            <(Path<i32>, Query<Page>) as FromRequest>::from_request(&mut req)
                .await
                .unwrap();

        // option not found
        let mut req = Request::empty();
        let o: Option<Path<i32>> = Option::<Path<i32>>::from_request(&mut req).await.unwrap();
        assert!(o.is_none());

        // result error mapping
        let mut req = Request::empty();
        let r: Result<Path<i32>, Response> =
            <Result<Path<i32>, Response> as FromRequest>::from_request(&mut req)
                .await
                .unwrap();
        assert!(r.is_err());
    }

    #[tokio::test]
    async fn test_form_and_typed_header_and_method_uri_version_remote() {
        // form-url-encoded
        #[derive(Deserialize, serde::Serialize)]
        struct U {
            name: String,
            age: u32,
        }
        let mut req = Request::empty();
        req.headers_mut().insert(
            "content-type",
            http::HeaderValue::from_static("application/x-www-form-urlencoded"),
        );
        req.replace_body(crate::core::req_body::ReqBody::Once(
            "name=Alice&age=25".as_bytes().to_vec().into(),
        ));
        let Form(u): Form<U> = Form::from_request(&mut req).await.unwrap();
        assert_eq!(u.name, "Alice");
        assert_eq!(u.age, 25);

        // typed header
        let mut req = Request::empty();
        req.headers_mut()
            .insert("user-agent", http::HeaderValue::from_static("curl/8.0"));
        let TypedHeader(ua): TypedHeader<UserAgent> =
            TypedHeader::from_request(&mut req).await.unwrap();
        assert_eq!(ua.as_str(), "curl/8.0");

        // method/uri/version
        let mut req = Request::empty();
        *req.method_mut() = http::Method::POST;
        *req.uri_mut() = http::Uri::from_static("http://localhost:8080/path?q=1");
        let Method(m): Method = Method::from_request(&mut req).await.unwrap();
        let Uri(u): Uri = Uri::from_request(&mut req).await.unwrap();
        let Version(v): Version = Version::from_request(&mut req).await.unwrap();
        assert_eq!(m, http::Method::POST);
        assert_eq!(u.path(), "/path");
        assert!(matches!(
            v,
            http::Version::HTTP_11 | http::Version::HTTP_10 | http::Version::HTTP_2
        ));

        // remote addr
        let mut req = Request::empty();
        req.set_remote("127.0.0.1:9090".parse().unwrap());
        let RemoteAddr(addr): RemoteAddr = RemoteAddr::from_request(&mut req).await.unwrap();
        assert_eq!(addr.to_string(), "127.0.0.1:9090");
    }

    #[tokio::test]
    async fn test_configs_and_extension_and_request_ext() {
        // configs
        #[derive(Clone)]
        struct CfgData(u32);
        let mut req = Request::empty();
        req.configs_mut().insert(CfgData(9));
        let Configs(CfgData(v)): Configs<CfgData> = Configs::from_request(&mut req).await.unwrap();
        assert_eq!(v, 9);

        // extensions
        #[derive(Clone)]
        struct Ext(&'static str);
        let mut req = Request::empty();
        req.extensions_mut().insert(Ext("hello"));
        let Extension(Ext(s)): Extension<Ext> = Extension::from_request(&mut req).await.unwrap();
        assert_eq!(s, "hello");

        // RequestExt::extract
        let mut req = Request::empty();
        req.set_path_params("id".to_owned(), crate::core::path_param::PathParam::Int(5));
        let Path(id): Path<i32> = RequestExt::extract(&mut req).await.unwrap();
        assert_eq!(id, 5);
    }

    #[tokio::test]
    async fn test_tuple_triple_quad_and_result_ok() {
        #[allow(dead_code)]
        #[derive(Deserialize)]
        struct Q {
            page: u32,
        }
        // triple
        let mut req = Request::empty();
        req.set_path_params("id".to_owned(), crate::core::path_param::PathParam::Int(1));
        *req.uri_mut() = http::Uri::from_static("http://localhost/test?page=3");
        req.headers_mut()
            .insert("user-agent", http::HeaderValue::from_static("ua"));
        let (_a, _b, _c): (Path<i32>, Query<Q>, TypedHeader<UserAgent>) =
            <(Path<i32>, Query<Q>, TypedHeader<UserAgent>) as FromRequest>::from_request(&mut req)
                .await
                .unwrap();

        // quad
        let mut req = Request::empty();
        req.set_path_params("id".to_owned(), crate::core::path_param::PathParam::Int(1));
        *req.uri_mut() = http::Uri::from_static("http://localhost/test?page=3");
        req.headers_mut()
            .insert("user-agent", http::HeaderValue::from_static("ua"));
        let (_a, _b, _c, _d): (Path<i32>, Query<Q>, TypedHeader<UserAgent>, Method) =
            <(Path<i32>, Query<Q>, TypedHeader<UserAgent>, Method) as FromRequest>::from_request(
                &mut req,
            )
            .await
            .unwrap();

        // Result<Json<_>, Response> success
        #[derive(Deserialize, serde::Serialize)]
        struct U {
            name: String,
        }
        let mut req = Request::empty();
        req.headers_mut().insert(
            "content-type",
            http::HeaderValue::from_static("application/json"),
        );
        req.replace_body(crate::core::req_body::ReqBody::Once(
            serde_json::to_vec(&U { name: "ok".into() }).unwrap().into(),
        ));
        let r: Result<Json<U>, Response> =
            <Result<Json<U>, Response> as FromRequest>::from_request(&mut req)
                .await
                .unwrap();
        assert!(matches!(r, Ok(Json(U { name })) if name == "ok"));
    }
}
