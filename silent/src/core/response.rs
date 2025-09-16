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
