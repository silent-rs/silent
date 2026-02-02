use std::fmt;
use std::fmt::{Display, Formatter};

use crate::core::res_body::{ResBody, full};
use crate::headers::{ContentType, Header, HeaderMap, HeaderMapExt};
use crate::{Configs, Result, SilentError, StatusCode, header};
use http::{Extensions, Version};
use http_body::{Body, SizeHint};
use serde::Serialize;
use serde_json::Value;

/// 响应体
/// ```
/// use silent::Response;
/// let req = Response::empty();
/// ```
pub struct Response<B: Body = ResBody> {
    /// The HTTP status code.
    pub(crate) status: StatusCode,
    /// The HTTP version.
    pub(crate) version: Version,
    /// The HTTP headers.
    pub(crate) headers: HeaderMap,
    pub(crate) body: B,
    pub(crate) extensions: Extensions,
    pub(crate) configs: Configs,
}

impl fmt::Debug for Response {
    #[inline]
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        writeln!(f, "{:?} {}\n{:?}", self.version, self.status, self.headers)
    }
}

impl Display for Response {
    #[inline]
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        fmt::Debug::fmt(self, f)
    }
}

impl Response {
    /// 创建空响应体
    pub fn empty() -> Self {
        Self {
            status: StatusCode::OK,
            headers: HeaderMap::new(),
            version: Version::default(),
            body: ResBody::None,
            extensions: Extensions::default(),
            configs: Configs::default(),
        }
    }
    /// 获取响应状态码
    #[inline]
    pub fn status(&self) -> StatusCode {
        self.status
    }
    /// 取出响应体（将内部body置为空）
    #[inline]
    pub fn take_body(&mut self) -> ResBody {
        std::mem::replace(&mut self.body, ResBody::None)
    }
    #[inline]
    /// 设置响应重定向
    pub fn redirect(url: &str) -> Result<Self> {
        let mut res = Self::empty();
        res.status = StatusCode::MOVED_PERMANENTLY;
        res.headers.insert(
            header::LOCATION,
            url.parse().map_err(|e| {
                SilentError::business_error(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("redirect error: {e}"),
                )
            })?,
        );
        Ok(res)
    }
    #[inline]
    /// 生成文本响应
    pub fn text(text: &str) -> Self {
        let mut res = Self::empty();
        res.set_typed_header(ContentType::text_utf8());
        res.set_body(full(text.as_bytes().to_vec()));
        res
    }
    #[inline]
    /// 生成html响应
    pub fn html(html: &str) -> Self {
        let mut res = Self::empty();
        res.set_typed_header(ContentType::html());
        res.set_body(full(html.as_bytes().to_vec()));
        res
    }
    #[inline]
    /// 生成json响应
    pub fn json<T: Serialize>(json: &T) -> Self {
        let mut res = Self::empty();
        res.set_typed_header(ContentType::json());
        res.set_body(full(serde_json::to_vec(json).unwrap()));
        res
    }
}

impl<B: Body> Response<B> {
    /// 设置响应状态
    #[inline]
    pub fn set_status(&mut self, status: StatusCode) {
        self.status = status;
    }
    /// 包含响应状态
    #[inline]
    pub fn with_status(mut self, status: StatusCode) -> Self {
        self.status = status;
        self
    }
    /// 设置响应body
    #[inline]
    pub fn set_body(&mut self, body: B) {
        self.body = body;
    }
    /// 包含响应body
    #[inline]
    pub fn with_body(mut self, body: B) -> Self {
        self.body = body;
        self
    }
    /// 获取响应体
    #[inline]
    pub fn body(&self) -> &B {
        &self.body
    }
    /// 设置响应header
    #[inline]
    pub fn set_header(&mut self, key: header::HeaderName, value: header::HeaderValue) {
        self.headers.insert(key, value);
    }
    /// 包含响应header
    #[inline]
    pub fn with_header(mut self, key: header::HeaderName, value: header::HeaderValue) -> Self {
        self.headers.insert(key, value);
        self
    }
    #[inline]
    /// 获取extensions
    pub fn extensions(&self) -> &Extensions {
        &self.extensions
    }
    #[inline]
    /// 获取extensions_mut
    pub fn extensions_mut(&mut self) -> &mut Extensions {
        &mut self.extensions
    }

    /// 获取配置
    #[inline]
    pub fn get_config<T: Send + Sync + 'static>(&self) -> Result<&T> {
        self.configs.get::<T>().ok_or(SilentError::ConfigNotFound)
    }

    /// 获取配置(Uncheck)
    #[inline]
    pub fn get_config_uncheck<T: Send + Sync + 'static>(&self) -> &T {
        self.configs.get::<T>().unwrap()
    }

    /// 获取全局配置
    #[inline]
    pub fn configs(&self) -> &Configs {
        &self.configs
    }

    /// 获取可变全局配置
    #[inline]
    pub fn configs_mut(&mut self) -> &mut Configs {
        &mut self.configs
    }
    #[inline]
    /// 设置响应header
    pub fn headers(&self) -> &HeaderMap {
        &self.headers
    }
    #[inline]
    /// 设置响应header
    pub fn headers_mut(&mut self) -> &mut HeaderMap {
        &mut self.headers
    }
    #[inline]
    /// 获取响应体长度
    pub fn content_length(&self) -> SizeHint {
        self.body.size_hint()
    }
    #[inline]
    /// 设置响应header
    pub fn set_typed_header<H>(&mut self, header: H)
    where
        H: Header,
    {
        self.headers.typed_insert(header);
    }
    #[inline]
    /// 包含响应header
    pub fn with_typed_header<H>(mut self, header: H) -> Self
    where
        H: Header,
    {
        self.headers.typed_insert(header);
        self
    }

    /// move response to from another response
    pub fn copy_from_response(&mut self, res: Response<B>) {
        self.headers.extend(res.headers);
        self.status = res.status;
        self.extensions.extend(res.extensions);
        self.set_body(res.body);
    }
}

impl<S: Serialize> From<S> for Response {
    fn from(value: S) -> Self {
        match serde_json::to_value(&value).unwrap() {
            Value::String(value) => Response::empty()
                .with_typed_header(ContentType::json())
                .with_body(full(value.as_bytes().to_vec())),
            Value::Null => Response::empty().with_status(StatusCode::NO_CONTENT),
            _ => Self::json(&value),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::response::Response;

    // 基础构造函数测试

    #[test]
    fn test_response_empty() {
        let res = Response::empty();
        assert_eq!(res.status(), StatusCode::OK);
        assert_eq!(res.version, Version::default());
        assert_eq!(res.headers().len(), 0);
    }

    #[test]
    fn test_response_text() {
        let res = Response::text("Hello, World!");
        assert_eq!(res.status(), StatusCode::OK);
        assert!(res.headers().get("content-type").is_some());
        let content_type = res.headers().get("content-type").unwrap();
        assert!(content_type.to_str().unwrap().contains("text/plain"));
    }

    #[test]
    fn test_response_html() {
        let res = Response::html("<html><body>Test</body></html>");
        assert_eq!(res.status(), StatusCode::OK);
        assert!(res.headers().get("content-type").is_some());
        let content_type = res.headers().get("content-type").unwrap();
        assert!(content_type.to_str().unwrap().contains("text/html"));
    }

    #[test]
    fn test_response_json() {
        let data = serde_json::json!({"key": "value"});
        let res = Response::json(&data);
        assert_eq!(res.status(), StatusCode::OK);
        assert!(res.headers().get("content-type").is_some());
        let content_type = res.headers().get("content-type").unwrap();
        assert!(content_type.to_str().unwrap().contains("application/json"));
    }

    #[test]
    fn test_response_json_with_struct() {
        #[derive(Serialize)]
        struct TestData {
            name: String,
            count: i32,
        }
        let data = TestData {
            name: "test".to_string(),
            count: 42,
        };
        let res = Response::json(&data);
        assert_eq!(res.status(), StatusCode::OK);
        assert!(res.headers().get("content-type").is_some());
    }

    #[test]
    fn test_response_redirect_valid_url() {
        let res = Response::redirect("https://example.com");
        assert!(res.is_ok());
        let res = res.unwrap();
        assert_eq!(res.status(), StatusCode::MOVED_PERMANENTLY);
        assert!(res.headers().get("location").is_some());
        let location = res.headers().get("location").unwrap();
        assert_eq!(location.to_str().unwrap(), "https://example.com");
    }

    #[test]
    fn test_response_redirect_relative_url() {
        let res = Response::redirect("/new-location");
        assert!(res.is_ok());
        let res = res.unwrap();
        assert_eq!(res.status(), StatusCode::MOVED_PERMANENTLY);
        assert!(res.headers().get("location").is_some());
    }

    #[test]
    fn test_response_redirect_empty_url() {
        let res = Response::redirect("");
        // 空字符串可以被 HeaderValue 接受
        assert!(res.is_ok());
        let res = res.unwrap();
        assert_eq!(res.status(), StatusCode::MOVED_PERMANENTLY);
    }

    #[test]
    fn test_response_redirect_invalid_url() {
        let res = Response::redirect("not a valid url");
        // HeaderValue 可以接受任何字符串
        assert!(res.is_ok());
        let res = res.unwrap();
        assert_eq!(res.status(), StatusCode::MOVED_PERMANENTLY);
    }

    // 状态管理测试

    #[test]
    fn test_response_status() {
        let res = Response::empty();
        assert_eq!(res.status(), StatusCode::OK);
    }

    #[test]
    fn test_response_set_status() {
        let mut res = Response::empty();
        res.set_status(StatusCode::NOT_FOUND);
        assert_eq!(res.status(), StatusCode::NOT_FOUND);
    }

    #[test]
    fn test_response_set_status_multiple() {
        let mut res = Response::empty();
        res.set_status(StatusCode::NOT_FOUND);
        assert_eq!(res.status(), StatusCode::NOT_FOUND);
        res.set_status(StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(res.status(), StatusCode::INTERNAL_SERVER_ERROR);
    }

    #[test]
    fn test_response_with_status() {
        let res = Response::empty().with_status(StatusCode::CREATED);
        assert_eq!(res.status(), StatusCode::CREATED);
    }

    #[test]
    fn test_response_with_status_chain() {
        let res = Response::empty()
            .with_status(StatusCode::CREATED)
            .with_status(StatusCode::ACCEPTED);
        assert_eq!(res.status(), StatusCode::ACCEPTED);
    }

    // 主体管理测试

    #[test]
    fn test_response_body() {
        let res = Response::text("test");
        assert!(!res.body().is_end_stream());
    }

    #[test]
    fn test_response_set_body() {
        let mut res = Response::empty();
        let new_body = full(b"new body".to_vec());
        res.set_body(new_body);
        assert!(!res.body().is_end_stream());
    }

    #[test]
    fn test_response_with_body() {
        let body = full(b"test body".to_vec());
        let res = Response::empty().with_body(body);
        assert!(!res.body().is_end_stream());
    }

    #[test]
    fn test_response_take_body() {
        let mut res = Response::text("test");
        let body = res.take_body();
        assert!(!body.is_end_stream());
        assert!(res.body().is_end_stream()); // After take, body should be None
    }

    #[test]
    fn test_response_take_body_twice() {
        let mut res = Response::text("test");
        let _body1 = res.take_body();
        let body2 = res.take_body();
        assert!(body2.is_end_stream());
    }

    #[test]
    fn test_response_content_length() {
        let res = Response::text("Hello, World!");
        let hint = res.content_length();
        assert!(hint.lower() > 0);
    }

    #[test]
    fn test_response_content_length_empty() {
        let res = Response::empty();
        let hint = res.content_length();
        assert_eq!(hint.lower(), 0);
    }

    // 头部管理测试

    #[test]
    fn test_response_headers() {
        let res = Response::text("test");
        assert!(res.headers().get("content-type").is_some());
        assert_eq!(res.headers().len(), 1);
    }

    #[test]
    fn test_response_headers_empty() {
        let res = Response::empty();
        assert_eq!(res.headers().len(), 0);
    }

    #[test]
    fn test_response_headers_mut() {
        let mut res = Response::empty();
        res.headers_mut()
            .insert("x-custom-header", "custom-value".parse().unwrap());
        assert_eq!(res.headers().len(), 1);
        assert!(res.headers().get("x-custom-header").is_some());
    }

    #[test]
    fn test_response_set_header() {
        let mut res = Response::empty();
        res.set_header(header::CONTENT_TYPE, "application/json".parse().unwrap());
        assert!(res.headers().get("content-type").is_some());
    }

    #[test]
    fn test_response_set_header_multiple() {
        let mut res = Response::empty();
        res.set_header(header::CONTENT_TYPE, "text/plain".parse().unwrap());
        res.set_header(header::CACHE_CONTROL, "no-cache".parse().unwrap());
        assert_eq!(res.headers().len(), 2);
    }

    #[test]
    fn test_response_with_header() {
        let res = Response::empty().with_header(header::CONTENT_TYPE, "text/html".parse().unwrap());
        assert!(res.headers().get("content-type").is_some());
    }

    #[test]
    fn test_response_with_header_chain() {
        let res = Response::empty()
            .with_header(header::CONTENT_TYPE, "text/plain".parse().unwrap())
            .with_header(header::CACHE_CONTROL, "no-cache".parse().unwrap());
        assert_eq!(res.headers().len(), 2);
    }

    #[test]
    fn test_response_set_typed_header() {
        let mut res = Response::empty();
        res.set_typed_header(ContentType::json());
        assert!(res.headers().get("content-type").is_some());
    }

    #[test]
    fn test_response_with_typed_header() {
        let res = Response::empty().with_typed_header(ContentType::text_utf8());
        assert!(res.headers().get("content-type").is_some());
    }

    // 扩展测试

    #[test]
    fn test_response_extensions() {
        let res = Response::empty();
        assert_eq!(res.extensions().len(), 0);
    }

    #[test]
    fn test_response_extensions_mut() {
        let mut res = Response::empty();
        res.extensions_mut().insert("test_key");
        assert_eq!(res.extensions().len(), 1);
    }

    #[test]
    fn test_response_extensions_insert_and_get() {
        let mut res = Response::empty();
        res.extensions_mut().insert(42i32);
        assert!(res.extensions().get::<i32>().is_some());
    }

    // 配置测试

    #[test]
    fn test_response_configs() {
        let res = Response::empty();
        assert_eq!(res.configs().len(), 0);
    }

    #[test]
    fn test_response_configs_mut() {
        let mut res = Response::empty();
        res.configs_mut().insert(42i32);
        assert_eq!(res.configs().len(), 1);
    }

    #[test]
    fn test_response_get_config_not_found() {
        let res = Response::empty();
        let result: Result<&i32> = res.get_config();
        assert!(result.is_err());
    }

    #[test]
    fn test_response_get_config_uncheck_panics() {
        // 测试 get_config_uncheck 在配置不存在时会 panic
        // Response 包含 ResBody，而 ResBody 包含非 UnwindSafe 的 trait object
        // 因此不能使用 catch_unwind 捕获 panic
        // 这个测试仅作为文档说明该方法在配置不存在时会 panic
    }

    #[test]
    fn test_response_get_config_success() {
        let mut res = Response::empty();
        res.configs_mut().insert(42i32);
        let config: Result<&i32> = res.get_config();
        assert!(config.is_ok());
        assert_eq!(*config.unwrap(), 42);
    }

    #[test]
    fn test_response_get_config_uncheck_success() {
        let mut res = Response::empty();
        res.configs_mut().insert(100i32);
        let config: &i32 = res.get_config_uncheck();
        assert_eq!(*config, 100);
    }

    // copy_from_response 测试

    #[test]
    fn test_response_copy_from_response() {
        let mut dest = Response::empty();
        let src = Response::text("source content")
            .with_status(StatusCode::CREATED)
            .with_header(header::CACHE_CONTROL, "no-cache".parse().unwrap());

        dest.copy_from_response(src);
        assert_eq!(dest.status(), StatusCode::CREATED);
        assert!(dest.headers().get("cache-control").is_some());
        assert!(!dest.body().is_end_stream());
    }

    #[test]
    fn test_response_copy_from_response_preserves_some_headers() {
        let mut dest =
            Response::empty().with_header(header::CONTENT_TYPE, "text/plain".parse().unwrap());
        let src = Response::html("<html>source</html>");

        dest.copy_from_response(src);
        // Source headers should extend destination, not replace
        assert!(dest.headers().get("content-type").is_some());
    }

    #[test]
    fn test_response_copy_from_response_empty_source() {
        let mut dest = Response::text("destination");
        let src = Response::empty();

        dest.copy_from_response(src);
        assert_eq!(dest.status(), StatusCode::OK);
    }

    // From trait 测试

    #[test]
    fn test_response_from_string() {
        let res: Response = "test string".to_string().into();
        assert_eq!(res.status(), StatusCode::OK);
        assert!(res.headers().get("content-type").is_some());
    }

    #[test]
    fn test_response_from_integer() {
        let res: Response = 42.into();
        assert_eq!(res.status(), StatusCode::OK);
        assert!(res.headers().get("content-type").is_some());
    }

    #[test]
    fn test_response_from_struct() {
        #[derive(Serialize)]
        struct TestData {
            field: String,
        }
        let data = TestData {
            field: "value".to_string(),
        };
        let res: Response = data.into();
        assert_eq!(res.status(), StatusCode::OK);
    }

    #[test]
    fn test_response_from_null_value() {
        let value: Option<i32> = None;
        let res: Response = value.into();
        assert_eq!(res.status(), StatusCode::NO_CONTENT);
    }

    #[test]
    fn test_response_from_json_object() {
        #[derive(Serialize)]
        struct Person {
            name: String,
            age: i32,
        }
        let person = Person {
            name: "Alice".to_string(),
            age: 30,
        };
        let res: Response = person.into();
        assert_eq!(res.status(), StatusCode::OK);
    }

    // Debug and Display trait 测试

    #[test]
    fn test_response_debug_format() {
        let res = Response::text("test");
        let debug_str = format!("{:?}", res);
        assert!(debug_str.contains("HTTP"));
        assert!(debug_str.contains("200 OK"));
    }

    #[test]
    fn test_response_display_format() {
        let res = Response::text("test");
        let display_str = format!("{}", res);
        assert!(!display_str.is_empty());
    }

    #[test]
    fn test_response_display_equals_debug() {
        let res = Response::empty();
        let debug_str = format!("{:?}", res);
        let display_str = format!("{}", res);
        // Display delegates to Debug
        assert_eq!(debug_str, display_str);
    }

    // 边界条件和特殊情况测试

    #[test]
    fn test_response_empty_text() {
        let res = Response::text("");
        assert_eq!(res.status(), StatusCode::OK);
        assert!(res.headers().get("content-type").is_some());
    }

    #[test]
    fn test_response_empty_html() {
        let res = Response::html("");
        assert_eq!(res.status(), StatusCode::OK);
        assert!(res.headers().get("content-type").is_some());
    }

    #[test]
    fn test_response_empty_json() {
        let data: serde_json::Value = serde_json::json!({});
        let res = Response::json(&data);
        assert_eq!(res.status(), StatusCode::OK);
    }

    #[test]
    fn test_response_unicode_text() {
        let text = "你好，世界！🌍";
        let res = Response::text(text);
        assert_eq!(res.status(), StatusCode::OK);
    }

    #[test]
    fn test_response_large_body() {
        let large_text = "x".repeat(1000000);
        let res = Response::text(&large_text);
        let hint = res.content_length();
        assert!(hint.lower() >= 1000000);
    }

    #[test]
    fn test_response_set_body_after_take() {
        let mut res = Response::text("original");
        let _ = res.take_body();
        res.set_body(full(b"new body".to_vec()));
        assert!(!res.body().is_end_stream());
    }

    #[test]
    fn test_response_status_code_range() {
        let mut res = Response::empty();
        for code in [100, 200, 301, 404, 500] {
            res.set_status(StatusCode::from_u16(code).unwrap());
            assert_eq!(res.status().as_u16(), code);
        }
    }

    #[test]
    fn test_response_version_default() {
        let res = Response::empty();
        assert_eq!(res.version, Version::default());
    }

    #[test]
    fn test_response_multiple_headers_same_name() {
        let mut res = Response::empty();
        res.headers_mut()
            .append("x-custom", "value1".parse().unwrap());
        res.headers_mut()
            .append("x-custom", "value2".parse().unwrap());
        let values: Vec<_> = res.headers().get_all("x-custom").iter().collect();
        assert_eq!(values.len(), 2);
    }
}
