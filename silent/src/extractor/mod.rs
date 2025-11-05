use futures_util::future::BoxFuture;
use std::sync::Arc;

use crate::{Request, Response};

pub use self::from_request::{FromRequest, cookie_param, header_param, path_param, query_param};
pub use self::types::*;

mod from_request;
mod types;

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
            crate::core::path_param::PathParam::from("bob".to_string()),
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

    #[tokio::test]
    async fn test_single_field_extractors() {
        // 测试 QueryParam
        let mut req = Request::empty();
        *req.uri_mut() = http::Uri::from_static("http://localhost/test?name=alice&age=25");
        let name = query_param::<String>(&mut req, "name").await.unwrap();
        assert_eq!(name, "alice");

        let age = query_param::<u32>(&mut req, "age").await.unwrap();
        assert_eq!(age, 25);

        // 测试 PathParam
        let mut req = Request::empty();
        req.set_path_params("id".to_owned(), crate::core::path_param::PathParam::Int(42));
        let id = path_param::<i32>(&mut req, "id").await.unwrap();
        assert_eq!(id, 42);

        // 测试 HeaderParam
        let mut req = Request::empty();
        req.headers_mut().insert(
            "content-type",
            http::HeaderValue::from_static("application/json"),
        );
        let content_type = header_param::<String>(&mut req, "content-type")
            .await
            .unwrap();
        assert_eq!(content_type, "application/json");

        // 测试 CookieParam
        let mut req = Request::empty();
        req.headers_mut().insert(
            "cookie",
            http::HeaderValue::from_static("session=abc123; user=alice"),
        );
        let session = cookie_param::<String>(&mut req, "session").await.unwrap();
        assert_eq!(session, "abc123");

        let user = cookie_param::<String>(&mut req, "user").await.unwrap();
        assert_eq!(user, "alice");
    }

    #[tokio::test]
    async fn test_single_field_extractors_not_found() {
        // 测试 QueryParam 不存在的情况
        let mut req = Request::empty();
        *req.uri_mut() = http::Uri::from_static("http://localhost/test");
        let result = query_param::<String>(&mut req, "missing").await;
        assert!(result.is_err());

        // 测试 PathParam 不存在的情况
        let mut req = Request::empty();
        let result = path_param::<String>(&mut req, "missing").await;
        assert!(result.is_err());

        // 测试 HeaderParam 不存在的情况
        let mut req = Request::empty();
        let result = header_param::<String>(&mut req, "missing").await;
        assert!(result.is_err());

        // 测试 CookieParam 不存在的情况
        let mut req = Request::empty();
        req.headers_mut()
            .insert("cookie", http::HeaderValue::from_static("session=abc123"));
        let result = cookie_param::<String>(&mut req, "missing").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_single_field_extractors_type_conversion() {
        // 测试类型转换：String -> i32
        let mut req = Request::empty();
        *req.uri_mut() = http::Uri::from_static("http://localhost/test?count=123");
        let count = query_param::<i32>(&mut req, "count").await.unwrap();
        assert_eq!(count, 123);

        // 测试类型转换：String -> bool
        let mut req = Request::empty();
        *req.uri_mut() = http::Uri::from_static("http://localhost/test?active=true");
        let active = query_param::<bool>(&mut req, "active").await.unwrap();
        assert!(active);

        // 测试类型转换：String -> f64
        let mut req = Request::empty();
        *req.uri_mut() = http::Uri::from_static("http://localhost/test?price=99.99");
        let price = query_param::<f64>(&mut req, "price").await.unwrap();
        assert_eq!(price, 99.99);

        // 测试类型转换：String -> u64
        let mut req = Request::empty();
        *req.uri_mut() = http::Uri::from_static("http://localhost/test?size=999");
        let size = query_param::<u64>(&mut req, "size").await.unwrap();
        assert_eq!(size, 999);
    }
}
