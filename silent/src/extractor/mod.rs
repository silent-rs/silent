//! # 萃取器模块
//!
//! 萃取器是 Silent 框架中用于从 HTTP 请求中提取数据的核心机制。它允许您以类型安全的方式
//! 获取路径参数、查询参数、请求头、请求体等各种数据。
//!
//! ## 主要特性
//!
//! - **类型安全**：所有萃取器都使用 Rust 的类型系统确保数据安全
//! - **零成本抽象**：编译时类型检查，无运行时开销
//! - **灵活组合**：支持多个萃取器组合使用
//! - **丰富类型**：支持所有实现了 `serde::Deserialize` 或 `FromStr` 的类型
//!
//! ## 基本用法
//!
//! ### 1. 路径参数萃取
//!
//! ```rust
//! use silent::extractor::Path;
//! use silent::Result;
//!
//! async fn handler(Path(id): Path<i64>) -> Result<String> {
//!     Ok(format!("用户ID: {}", id))
//! }
//! ```
//!
//! ### 2. 查询参数萃取
//!
//! ```rust
//! use silent::extractor::Query;
//! use silent::Result;
//! use serde::Deserialize;
//!
//! #[derive(Deserialize)]
//! struct Page {
//!     page: u32,
//!     size: u32,
//! }
//!
//! async fn handler(Query(p): Query<Page>) -> Result<String> {
//!     Ok(format!("第 {} 页，每页 {} 条", p.page, p.size))
//! }
//! ```
//!
//! ### 3. 组合使用
//!
//! ```rust
//! use silent::extractor::{Path, Query, Json};
//! use silent::{Request, Result};
//! use serde::Deserialize;
//!
//! #[derive(Deserialize)]
//! struct Page {
//!     page: u32,
//!     size: u32,
//! }
//!
//! #[derive(Deserialize)]
//! struct Data {
//!     name: String,
//! }
//!
//! async fn handler(
//!     (Path(id), Query(p), Json(data)): (Path<i64>, Query<Page>, Json<Data>),
//! ) -> Result<String> {
//!     // 处理提取的数据
//!     Ok("成功".to_string())
//! }
//! ```
//!
//! ## 萃取器类型
//!
//! 本模块提供以下萃取器：
//!
//! - **Path<T>**：从 URL 路径中提取参数
//! - **Query<T>**：从查询字符串中提取参数
//! - **Json<T>**：从 JSON 请求体中提取数据
//! - **Form<T>**：从表单数据中提取参数
//! - **TypedHeader<T>**：提取并解析特定类型的请求头
//! - **Extension<T>**：从请求扩展中提取数据
//! - **Configs<T>**：从请求配置中提取数据
//! - **Method、Uri、Version**：提取请求的基础信息
//!
//! ## 自定义萃取器
//!
//! 您可以通过实现 `FromRequest` trait 来创建自定义萃取器：
//!
//! ```rust
//! use async_trait::async_trait;
//! use silent::extractor::FromRequest;
//! use silent::{Request, Result, SilentError};
//!
//! struct AuthToken(String);
//!
//! #[async_trait]
//! impl FromRequest for AuthToken {
//!     type Rejection = SilentError;
//!
//!     async fn from_request(req: &mut Request) -> std::result::Result<Self, Self::Rejection> {
//!         let token = req.headers()
//!             .get("authorization")
//!             .and_then(|v| v.to_str().ok())
//!             .map(|s| s.to_string())
//!             .ok_or(SilentError::ParamsNotFound)?;
//!
//!         Ok(AuthToken(token))
//!     }
//! }
//! ```
//!
//! ## 组合使用
//!
//! 萃取器可以组合使用：
//!
//! ```rust
//! use silent::Result;
//! use silent::extractor::{Path, Query, Json};
//! use serde::Deserialize;
//!
//! #[derive(Deserialize)]
//! struct Page {
//!     page: u32,
//!     size: u32,
//! }
//!
//! #[derive(Deserialize)]
//! struct Data {
//!     name: String,
//! }
//!
//! async fn handler(
//!     (Path(id), Query(p), Json(data)): (Path<i64>, Query<Page>, Json<Data>),
//! ) -> Result<String> {
//!     // 处理提取的数据
//!     Ok("成功".to_string())
//! }
//! ```
//!
//! ## 错误处理
//!
//! 萃取器在提取失败时会返回错误。您可以使用 `Option<T>` 或 `Result<T, E>` 来优雅处理：
//!
//! ```rust
//! use silent::{Result, Response};
//! use silent::extractor::{Path, Json};
//!
//! // 可选参数
//! async fn handler(opt_id: Option<Path<i64>>) -> Result<String> {
//!     match opt_id {
//!         Some(Path(id)) => Ok(format!("有ID: {}", id)),
//!         None => Ok("无ID".to_string()),
//!     }
//! }
//!
//! // 自定义错误处理
//! #[derive(serde::Deserialize, Debug)]
//! struct Data {
//!     name: String,
//! }
//!
//! async fn handler2(
//!     result: std::result::Result<Json<Data>, Response>,
//! ) -> Result<String> {
//!     match result {
//!         Ok(Json(data)) => Ok(format!("数据: {:?}", data)),
//!         Err(_) => Ok("请求无效".to_string()),
//!     }
//! }
//! ```
//!
//! # Examples
//!
//! 查看 `examples/extractors/` 目录获取更多示例。
//!
// pub use silent_macros::define_extractors;  // 暂时注释，将在后面正确设置

use futures_util::future::BoxFuture;
use std::sync::Arc;

use crate::{Request, Response};

pub use self::from_request::FromRequest;
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
        #[serde(default)]
        page: u32,
        #[serde(default)]
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
    async fn test_path_param_edge_cases() {
        // 测试各种数字类型
        let mut req = Request::empty();
        req.set_path_params(
            "val".to_owned(),
            crate::core::path_param::PathParam::Int(-42),
        );
        let Path(val): Path<i32> = Path::from_request(&mut req).await.unwrap();
        assert_eq!(val, -42);

        let mut req = Request::empty();
        req.set_path_params(
            "val".to_owned(),
            crate::core::path_param::PathParam::UInt32(123),
        );
        let Path(val): Path<u32> = Path::from_request(&mut req).await.unwrap();
        assert_eq!(val, 123);

        let mut req = Request::empty();
        req.set_path_params(
            "val".to_owned(),
            crate::core::path_param::PathParam::Int64(i64::MIN),
        );
        let Path(val): Path<i64> = Path::from_request(&mut req).await.unwrap();
        assert_eq!(val, i64::MIN);

        let mut req = Request::empty();
        req.set_path_params(
            "val".to_owned(),
            crate::core::path_param::PathParam::UInt64(u64::MAX),
        );
        let Path(val): Path<u64> = Path::from_request(&mut req).await.unwrap();
        assert_eq!(val, u64::MAX);

        // 测试字符串和路径类型
        let mut req = Request::empty();
        req.set_path_params(
            "val".to_owned(),
            crate::core::path_param::PathParam::from("test-string".to_string()),
        );
        let Path(val): Path<String> = Path::from_request(&mut req).await.unwrap();
        assert_eq!(val, "test-string");

        let mut req = Request::empty();
        req.set_path_params(
            "val".to_owned(),
            crate::core::path_param::PathParam::Path(crate::core::path_param::PathString::Owned(
                "path/to/file".to_string(),
            )),
        );
        let Path(val): Path<String> = Path::from_request(&mut req).await.unwrap();
        assert_eq!(val, "path/to/file");
    }

    #[tokio::test]
    async fn test_query_param_edge_cases() {
        // 测试空查询字符串
        let mut req = Request::empty();
        *req.uri_mut() = http::Uri::from_static("http://localhost/test");
        let Query(params): Query<Page> = Query::from_request(&mut req).await.unwrap();
        assert_eq!(params.page, 0);
        assert_eq!(params.size, 0);

        // 测试包含特殊字符的查询参数
        *req.uri_mut() =
            http::Uri::from_static("http://localhost/test?name=hello%20world&value=123");
        #[derive(serde::Deserialize)]
        struct SpecialParams {
            name: String,
            value: i32,
        }
        let Query(params): Query<SpecialParams> = Query::from_request(&mut req).await.unwrap();
        assert_eq!(params.name, "hello world");
        assert_eq!(params.value, 123);

        // 测试枚举类型
        *req.uri_mut() = http::Uri::from_static("http://localhost/test?status=active");
        #[derive(serde::Deserialize)]
        enum Status {
            #[serde(rename = "active")]
            Active,
            #[serde(rename = "inactive")]
            Inactive,
        }
        #[derive(serde::Deserialize)]
        struct EnumParam {
            status: Status,
        }
        let Query(params): Query<EnumParam> = Query::from_request(&mut req).await.unwrap();
        assert!(matches!(params.status, Status::Active));
    }

    #[tokio::test]
    async fn test_json_and_form_error_cases() {
        // 测试无效 JSON
        let mut req = Request::empty();
        req.headers_mut().insert(
            "content-type",
            http::HeaderValue::from_static("application/json"),
        );
        req.replace_body(crate::core::req_body::ReqBody::Once(
            b"{invalid json}".to_vec().into(),
        ));
        let result = Json::<serde_json::Value>::from_request(&mut req).await;
        assert!(result.is_err());

        // 测试缺失 content-type 的 JSON
        let mut req = Request::empty();
        req.replace_body(crate::core::req_body::ReqBody::Once(b"{}".to_vec().into()));
        let result = Json::<serde_json::Value>::from_request(&mut req).await;
        assert!(result.is_err());

        // 测试无效的表单数据
        let mut req = Request::empty();
        req.headers_mut().insert(
            "content-type",
            http::HeaderValue::from_static("application/x-www-form-urlencoded"),
        );
        req.replace_body(crate::core::req_body::ReqBody::Once(
            b"{invalid}".to_vec().into(),
        ));
        #[derive(serde::Deserialize, serde::Serialize)]
        struct FormData {
            key: String,
        }
        let result = Form::<FormData>::from_request(&mut req).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_complex_struct_parsing() {
        // 测试嵌套结构体
        let mut req = Request::empty();
        req.headers_mut().insert(
            "content-type",
            http::HeaderValue::from_static("application/json"),
        );
        let nested_data = serde_json::json!({
            "user": {
                "name": "Alice",
                "age": 30
            },
            "settings": {
                "theme": "dark",
                "notifications": true
            }
        });
        req.replace_body(crate::core::req_body::ReqBody::Once(
            serde_json::to_vec(&nested_data).unwrap().into(),
        ));

        #[derive(serde::Deserialize)]
        struct User {
            name: String,
            age: u32,
        }

        #[derive(serde::Deserialize)]
        struct Settings {
            theme: String,
            notifications: bool,
        }

        #[derive(serde::Deserialize)]
        struct ComplexData {
            user: User,
            settings: Settings,
        }

        let Json(data): Json<ComplexData> = Json::from_request(&mut req).await.unwrap();
        assert_eq!(data.user.name, "Alice");
        assert_eq!(data.user.age, 30);
        assert_eq!(data.settings.theme, "dark");
        assert!(data.settings.notifications);
    }

    #[tokio::test]
    async fn test_option_extractor_variations() {
        // 测试可选的查询参数
        let mut req = Request::empty();
        *req.uri_mut() = http::Uri::from_static("http://localhost/test?page=1");
        #[derive(serde::Deserialize)]
        struct OptionalParams {
            page: Option<u32>,
            size: Option<u32>,
        }
        let Query(params): Query<OptionalParams> = Query::from_request(&mut req).await.unwrap();
        assert_eq!(params.page, Some(1));
        assert_eq!(params.size, None);

        // 测试可选的路径参数
        let mut req = Request::empty();
        let result = Option::<Path<i32>>::from_request(&mut req).await.unwrap();
        assert!(result.is_none());

        // 测试可选的 JSON（缺失的 body）
        let mut req = Request::empty();
        let result = Option::<Json<serde_json::Value>>::from_request(&mut req)
            .await
            .unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_result_extractor_variations() {
        // 测试 Result<T, Response> 在成功情况下
        let mut req = Request::empty();
        req.set_path_params("id".to_owned(), crate::core::path_param::PathParam::Int(42));
        let result: Result<Path<i32>, Response> =
            <Result<Path<i32>, Response> as FromRequest>::from_request(&mut req)
                .await
                .unwrap();
        assert!(matches!(result, Ok(Path(42))));

        // 测试 Result<T, Response> 在失败情况下
        let mut req = Request::empty();
        let result: Result<Path<i32>, Response> =
            <Result<Path<i32>, Response> as FromRequest>::from_request(&mut req)
                .await
                .unwrap();
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_multiple_typed_headers() {
        // 测试多个 TypedHeader 组合
        let mut req = Request::empty();
        req.headers_mut()
            .insert("user-agent", http::HeaderValue::from_static("Mozilla/5.0"));
        req.headers_mut().insert(
            "content-type",
            http::HeaderValue::from_static("application/json"),
        );

        let (TypedHeader(ua), TypedHeader(content_type)): (
            TypedHeader<UserAgent>,
            TypedHeader<headers::ContentType>,
        ) = <(
            TypedHeader<UserAgent>,
            TypedHeader<headers::ContentType>,
        ) as FromRequest>::from_request(&mut req)
            .await
            .unwrap();

        assert!(ua.as_str().contains("Mozilla"));
        assert!(content_type.to_string().starts_with("application/json"));
    }

    #[tokio::test]
    async fn test_deeply_nested_tuples() {
        // 测试四元组
        let mut req = Request::empty();
        req.set_path_params("id".to_owned(), crate::core::path_param::PathParam::Int(1));
        *req.uri_mut() = http::Uri::from_static("http://localhost/test?page=3");
        req.headers_mut()
            .insert("user-agent", http::HeaderValue::from_static("ua"));
        req.headers_mut()
            .insert("content-type", http::HeaderValue::from_static("text/html"));
        req.configs_mut().insert(ConfigsData(99));

        #[derive(serde::Deserialize)]
        struct Q {
            page: u32,
        }

        #[derive(Clone)]
        struct ConfigsData(u32);

        type FourTupleResult = Result<
            (
                Path<i32>,
                Query<Q>,
                TypedHeader<UserAgent>,
                Configs<ConfigsData>,
            ),
            Response,
        >;

        let result: FourTupleResult = <(
            Path<i32>,
            Query<Q>,
            TypedHeader<UserAgent>,
            Configs<ConfigsData>,
        ) as FromRequest>::from_request(&mut req)
        .await;

        assert!(result.is_ok());
        let (Path(id), Query(q), TypedHeader(ua), Configs(cfg)) = result.unwrap();
        assert_eq!(id, 1);
        assert_eq!(q.page, 3);
        assert!(ua.as_str().contains("ua"));
        assert_eq!(cfg.0, 99);
    }

    #[tokio::test]
    async fn test_mixed_extractors_with_request() {
        // 测试混合使用萃取器和 Request
        let mut req = Request::empty();
        req.set_path_params(
            "id".to_owned(),
            crate::core::path_param::PathParam::Int(123),
        );
        *req.uri_mut() = http::Uri::from_static("http://localhost:8080/test?page=1&size=10");
        req.headers_mut()
            .insert("x-test", http::HeaderValue::from_static("value"));

        // 这测试了多个萃取器同时工作的情况
        let (Path(path_id), Query(query_params), Method(method)): (Path<i32>, Query<Page>, Method) =
            <(Path<i32>, Query<Page>, Method) as FromRequest>::from_request(&mut req)
                .await
                .unwrap();

        assert_eq!(path_id, 123);
        assert_eq!(query_params.page, 1);
        assert_eq!(query_params.size, 10);
        assert_eq!(method, http::Method::GET);
    }

    #[tokio::test]
    async fn test_extension_with_different_types() {
        // 测试多种类型的 Extension
        #[derive(Clone)]
        struct UserId(String);
        #[derive(Clone)]
        struct Permission(u32);

        let mut req = Request::empty();
        req.extensions_mut().insert(UserId("user-123".to_string()));
        req.extensions_mut().insert(Permission(777));

        let Extension(user_id): Extension<UserId> =
            Extension::from_request(&mut req).await.unwrap();
        let Extension(permission): Extension<Permission> =
            Extension::from_request(&mut req).await.unwrap();

        assert_eq!(user_id.0, "user-123");
        assert_eq!(permission.0, 777);

        // 测试不存在的扩展
        #[derive(Clone)]
        struct NonExistent;
        let result = Extension::<NonExistent>::from_request(&mut req).await;
        assert!(result.is_err());
    }
}
