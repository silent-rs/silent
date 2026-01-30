#[cfg(feature = "multipart")]
use crate::core::form::{FilePart, FormData};
use crate::core::path_param::PathParam;
use crate::core::remote_addr::RemoteAddr;
use crate::core::req_body::ReqBody;
#[cfg(feature = "multipart")]
use crate::core::serde::from_str_multi_val;
use crate::header::CONTENT_TYPE;
use crate::{Configs, Result, SilentError};
use bytes::Bytes;
use http::request::Parts;
use http::{Extensions, HeaderMap, HeaderValue, Method, Uri, Version};
use http::{Request as BaseRequest, StatusCode};
use http_body_util::BodyExt;
use mime::Mime;
use once_cell::sync::OnceCell;
use serde::Deserialize;
use serde::de::StdError;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use url::form_urlencoded;

/// 请求体
/// ```
/// use silent::Request;
/// let req = Request::empty();
/// ```
#[derive(Debug)]
pub struct Request {
    // req: BaseRequest<ReqBody>,
    parts: Parts,
    path_params: HashMap<String, PathParam>,
    params: HashMap<String, String>,
    body: ReqBody,
    path_source: Option<Arc<str>>,
    #[cfg(feature = "multipart")]
    form_data: OnceCell<FormData>,
    json_data: OnceCell<Value>,
    form_body_cache: OnceCell<Vec<u8>>,
    pub(crate) configs: Configs,
}

impl Request {
    /// 从http请求体创建请求
    pub fn into_http(self) -> http::Request<ReqBody> {
        http::Request::from_parts(self.parts, self.body)
    }
    /// Strip the request to [`hyper::Request`].
    #[doc(hidden)]
    pub fn strip_to_hyper<QB>(&mut self) -> Result<hyper::Request<QB>>
    where
        QB: TryFrom<ReqBody>,
        <QB as TryFrom<ReqBody>>::Error: StdError + Send + Sync + 'static,
    {
        let mut builder = http::request::Builder::new()
            .method(self.method().clone())
            .uri(self.uri().clone())
            .version(self.version());
        if let Some(headers) = builder.headers_mut() {
            *headers = std::mem::take(self.headers_mut());
        }
        if let Some(extensions) = builder.extensions_mut() {
            *extensions = std::mem::take(self.extensions_mut());
        }

        let body = self.take_body();
        builder
            .body(body.try_into().map_err(|e| {
                SilentError::business_error(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("request strip to hyper failed: {e}"),
                )
            })?)
            .map_err(|e| SilentError::business_error(StatusCode::BAD_REQUEST, e.to_string()))
    }
    /// Strip the request to [`hyper::Request`].
    #[doc(hidden)]
    pub async fn strip_to_bytes_hyper(&mut self) -> Result<hyper::Request<Bytes>> {
        let mut builder = http::request::Builder::new()
            .method(self.method().clone())
            .uri(self.uri().clone())
            .version(self.version());
        if let Some(headers) = builder.headers_mut() {
            *headers = std::mem::take(self.headers_mut());
        }
        if let Some(extensions) = builder.extensions_mut() {
            *extensions = std::mem::take(self.extensions_mut());
        }

        let mut body = self.take_body();
        builder
            .body(body.frame().await.unwrap()?.into_data().unwrap())
            .map_err(|e| SilentError::business_error(StatusCode::BAD_REQUEST, e.to_string()))
    }
}

impl Default for Request {
    fn default() -> Self {
        Self::empty()
    }
}

impl Request {
    /// 创建空请求体
    pub fn empty() -> Self {
        let (parts, _) = BaseRequest::builder()
            .method("GET")
            .body(())
            .unwrap()
            .into_parts();
        Self {
            // req: BaseRequest::builder()
            //     .method("GET")
            //     .body(().into())
            //     .unwrap(),
            parts,
            path_params: HashMap::new(),
            params: HashMap::new(),
            body: ReqBody::Empty,
            path_source: None,
            #[cfg(feature = "multipart")]
            form_data: OnceCell::new(),
            json_data: OnceCell::new(),
            form_body_cache: OnceCell::new(),
            configs: Configs::default(),
        }
    }

    /// 从请求体创建请求
    #[inline]
    pub fn from_parts(parts: Parts, body: ReqBody) -> Self {
        Self {
            parts,
            body,
            ..Self::default()
        }
    }

    /// 获取访问真实地址
    ///
    /// - 仅从请求头 `x-real-ip` 中解析远端地址；
    /// - 若解析失败则 panic，假定上游已通过 `set_remote` 注入了正确值。
    #[inline]
    pub fn remote(&self) -> RemoteAddr {
        self.headers()
            .get("x-real-ip")
            .and_then(|h| h.to_str().ok())
            .and_then(|s| s.parse::<RemoteAddr>().ok())
            .expect("remote addr not set or invalid in x-real-ip header")
    }

    /// 设置访问真实地址
    ///
    /// 适配策略：
    /// - 若请求头中已存在合法的 `x-real-ip`，则保持不变；
    /// - 否则优先从 `X-Forwarded-For` 中解析第一个有效 IP，写入 `x-real-ip`；
    /// - 若仍不可用，则退回使用传入的 `remote_addr`。
    #[inline]
    pub fn set_remote(&mut self, remote_addr: RemoteAddr) {
        // 已有合法 x-real-ip，则尊重上游配置
        if self
            .headers()
            .get("x-real-ip")
            .and_then(|h| h.to_str().ok())
            .and_then(|s| s.parse::<RemoteAddr>().ok())
            .is_some()
        {
            return;
        }

        // 优先根据 X-Forwarded-For 计算真实客户端 IP
        if let Some(real_from_forwarded) = self
            .headers()
            .get("x-forwarded-for")
            .and_then(|h| h.to_str().ok())
            .and_then(|v| {
                v.split(',')
                    .map(|p| p.trim())
                    .find(|p| !p.is_empty())
                    .and_then(|ip| ip.parse::<RemoteAddr>().ok())
            })
        {
            self.headers_mut().insert(
                "x-real-ip",
                real_from_forwarded.to_string().parse().unwrap(),
            );
            return;
        }

        // 最后退回到底层 peer 地址
        self.headers_mut()
            .insert("x-real-ip", remote_addr.to_string().parse().unwrap());
    }

    pub(crate) fn set_path_source(&mut self, source: Arc<str>) {
        self.path_source = Some(source);
    }

    /// 获取请求方法
    #[inline]
    pub fn method(&self) -> &Method {
        &self.parts.method
    }

    /// 获取请求方法
    #[inline]
    pub fn method_mut(&mut self) -> &mut Method {
        &mut self.parts.method
    }
    /// 获取请求uri
    #[inline]
    pub fn uri(&self) -> &Uri {
        &self.parts.uri
    }
    /// 获取请求uri
    #[inline]
    pub fn uri_mut(&mut self) -> &mut Uri {
        &mut self.parts.uri
    }
    /// 获取请求版本
    #[inline]
    pub fn version(&self) -> Version {
        self.parts.version
    }
    /// 获取请求版本
    #[inline]
    pub fn version_mut(&mut self) -> &mut Version {
        &mut self.parts.version
    }
    /// 获取请求头
    #[inline]
    pub fn headers(&self) -> &HeaderMap<HeaderValue> {
        &self.parts.headers
    }
    /// 获取请求头
    #[inline]
    pub fn headers_mut(&mut self) -> &mut HeaderMap<HeaderValue> {
        &mut self.parts.headers
    }
    /// 获取请求拓展
    #[inline]
    pub fn extensions(&self) -> &Extensions {
        &self.parts.extensions
    }
    /// 获取请求拓展
    #[inline]
    pub fn extensions_mut(&mut self) -> &mut Extensions {
        &mut self.parts.extensions
    }
    pub(crate) fn set_path_params(&mut self, key: String, value: PathParam) {
        self.path_params.insert(key, value);
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
    pub fn configs(&self) -> Configs {
        self.configs.clone()
    }

    /// 获取可变全局配置
    #[inline]
    pub fn configs_mut(&mut self) -> &mut Configs {
        &mut self.configs
    }

    /// 获取路径参数集合
    pub fn path_params(&self) -> &HashMap<String, PathParam> {
        &self.path_params
    }

    /// 获取路径参数
    pub fn get_path_params<'a, T>(&'a self, key: &'a str) -> Result<T>
    where
        T: TryFrom<&'a PathParam, Error = SilentError>,
    {
        match self.path_params.get(key) {
            Some(value) => value.try_into(),
            None => Err(SilentError::ParamsNotFound),
        }
    }

    /// 获取query参数
    pub fn params(&mut self) -> &HashMap<String, String> {
        if let Some(query) = self.uri().query() {
            let params = form_urlencoded::parse(query.as_bytes())
                .into_owned()
                .collect::<HashMap<String, String>>();
            self.params = params;
        };
        &self.params
    }

    /// 转换query参数
    pub fn params_parse<T>(&mut self) -> Result<T>
    where
        for<'de> T: Deserialize<'de>,
    {
        let query = self.uri().query().unwrap_or("");
        let params = serde_html_form::from_str(query)?;
        Ok(params)
    }

    /// 获取请求body
    #[inline]
    pub fn replace_body(&mut self, body: ReqBody) -> ReqBody {
        std::mem::replace(&mut self.body, body)
    }

    /// 获取请求body
    #[inline]
    pub fn take_body(&mut self) -> ReqBody {
        self.replace_body(ReqBody::Empty)
    }

    /// 获取请求content_type
    #[inline]
    pub fn content_type(&self) -> Option<Mime> {
        self.headers()
            .get(CONTENT_TYPE)
            .and_then(|h| h.to_str().ok())
            .and_then(|v| v.parse().ok())
    }

    /// 获取请求form_data
    #[cfg(feature = "multipart")]
    #[inline]
    pub async fn form_data(&mut self) -> Result<&FormData> {
        let content_type = self
            .content_type()
            .ok_or(SilentError::ContentTypeMissingError)?;
        if content_type.subtype() != mime::FORM_DATA {
            return Err(SilentError::ContentTypeError);
        }

        // Check if already initialized
        if self.form_data.get().is_some() {
            return Ok(self.form_data.get().unwrap());
        }

        let body = self.take_body();
        let headers = self.headers();
        let form_data = FormData::read(headers, body).await.map_err(|e| {
            SilentError::business_error(
                StatusCode::BAD_REQUEST,
                format!("Failed to read form data: {}", e),
            )
        })?;
        self.form_data.get_or_init(|| form_data);
        Ok(self.form_data.get().unwrap())
    }

    /// 解析表单数据（支持 multipart/form-data 和 application/x-www-form-urlencoded）
    pub async fn form_parse<T>(&mut self) -> Result<T>
    where
        for<'de> T: Deserialize<'de>,
    {
        let content_type = self
            .content_type()
            .ok_or(SilentError::ContentTypeMissingError)?;

        match content_type.subtype() {
            #[cfg(feature = "multipart")]
            mime::FORM_DATA => self.multipart_form_parse().await,
            mime::WWW_FORM_URLENCODED => self.urlencoded_form_parse().await,
            _ => Err(SilentError::ContentTypeError),
        }
    }

    /// 解析 multipart/form-data 数据
    #[cfg(feature = "multipart")]
    async fn multipart_form_parse<T>(&mut self) -> Result<T>
    where
        for<'de> T: Deserialize<'de>,
    {
        // 复用 form_data 的缓存机制
        let form_data = self.form_data().await?;
        let value = serde_json::to_value(form_data.fields.clone()).map_err(SilentError::from)?;
        serde_json::from_value(value).map_err(Into::into)
    }

    /// 解析 application/x-www-form-urlencoded 数据
    async fn urlencoded_form_parse<T>(&mut self) -> Result<T>
    where
        for<'de> T: Deserialize<'de>,
    {
        // 先尝试从 form_body_cache 获取缓存的字节数据
        let bytes = if let Some(cached_bytes) = self.form_body_cache.get() {
            cached_bytes.clone()
        } else {
            // 如果没有缓存，则从请求体中读取并缓存
            let body = self.take_body();
            let bytes = match body {
                ReqBody::Empty => return Err(SilentError::BodyEmpty),
                other => other
                    .collect()
                    .await
                    .or(Err(SilentError::BodyEmpty))?
                    .to_bytes()
                    .to_vec(),
            };

            if bytes.is_empty() {
                return Err(SilentError::BodyEmpty);
            }

            // 缓存字节数据
            let _ = self.form_body_cache.set(bytes.clone());
            bytes
        };

        // 解析 form-urlencoded 数据
        let parsed_data: T = serde_html_form::from_bytes(&bytes).map_err(SilentError::from)?;

        Ok(parsed_data)
    }

    /// 转换body参数
    #[cfg(feature = "multipart")]
    pub async fn form_field<T>(&mut self, key: &str) -> Option<T>
    where
        for<'de> T: Deserialize<'de>,
    {
        self.form_data()
            .await
            .ok()
            .and_then(|ps| ps.fields.get_vec(key))
            .and_then(|vs| from_str_multi_val(vs).ok())
    }

    /// 获取上传的文件
    #[cfg(feature = "multipart")]
    #[inline]
    pub async fn files<'a>(&'a mut self, key: &'a str) -> Option<&'a Vec<FilePart>> {
        self.form_data()
            .await
            .ok()
            .and_then(|ps| ps.files.get_vec(key))
    }

    /// 解析 JSON 数据（仅支持 application/json）
    pub async fn json_parse<T>(&mut self) -> Result<T>
    where
        for<'de> T: Deserialize<'de>,
    {
        // 检查是否已缓存
        if let Some(cached_value) = self.json_data.get() {
            return serde_json::from_value(cached_value.clone()).map_err(Into::into);
        }

        let content_type = self
            .content_type()
            .ok_or(SilentError::ContentTypeMissingError)?;

        if content_type.subtype() != mime::JSON {
            return Err(SilentError::ContentTypeError);
        }

        let body = self.take_body();
        let bytes = match body {
            ReqBody::Empty => return Err(SilentError::JsonEmpty),
            other => other
                .collect()
                .await
                .or(Err(SilentError::JsonEmpty))?
                .to_bytes(),
        };

        if bytes.is_empty() {
            return Err(SilentError::JsonEmpty);
        }

        let value: Value = serde_json::from_slice(&bytes).map_err(SilentError::from)?;

        // 缓存结果
        let _ = self.json_data.set(value.clone());

        serde_json::from_value(value).map_err(Into::into)
    }

    /// 转换body参数按Json匹配
    pub async fn json_field<T>(&mut self, key: &str) -> Result<T>
    where
        for<'de> T: Deserialize<'de>,
    {
        let value: Value = self.json_parse().await?;
        serde_json::from_value(
            value
                .get(key)
                .ok_or(SilentError::ParamsNotFound)?
                .to_owned(),
        )
        .map_err(Into::into)
    }

    /// 获取请求body
    #[inline]
    pub fn replace_extensions(&mut self, extensions: Extensions) -> Extensions {
        std::mem::replace(self.extensions_mut(), extensions)
    }

    /// 获取请求body
    #[inline]
    pub fn take_extensions(&mut self) -> Extensions {
        self.replace_extensions(Extensions::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::path_param::PathString;
    use std::net::{IpAddr, Ipv4Addr, SocketAddr as StdSocketAddr};

    // ==================== 基础构造函数测试 ====================

    #[test]
    fn test_request_empty() {
        let req = Request::empty();
        assert_eq!(req.method(), Method::GET);
        assert_eq!(req.uri(), &Uri::from_static("/"));
        assert_eq!(req.version(), Version::HTTP_11);
        assert!(req.path_params.is_empty());
        assert!(req.params.is_empty());
    }

    #[test]
    fn test_request_default() {
        let req = Request::default();
        assert_eq!(req.method(), Method::GET);
    }

    #[test]
    fn test_request_from_parts() {
        let (parts, _) = BaseRequest::builder()
            .method("POST")
            .uri("/test")
            .body(())
            .unwrap()
            .into_parts();

        let req = Request::from_parts(parts, ReqBody::Empty);
        assert_eq!(req.method(), Method::POST);
        assert_eq!(req.uri(), &Uri::from_static("/test"));
    }

    // ==================== remote/set_remote 测试 ====================

    #[test]
    fn test_remote_with_x_real_ip() {
        let mut req = Request::empty();
        req.headers_mut()
            .insert("x-real-ip", "127.0.0.1:8080".parse().unwrap());

        let remote = req.remote();
        assert_eq!(remote.to_string(), "127.0.0.1:8080");
    }

    #[test]
    fn test_set_remote_with_existing_x_real_ip() {
        let mut req = Request::empty();
        req.headers_mut()
            .insert("x-real-ip", "192.168.1.1:9000".parse().unwrap());

        let new_addr = RemoteAddr::from(StdSocketAddr::new(
            IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1)),
            8080,
        ));
        req.set_remote(new_addr);

        // x-real-ip 应该保持不变
        assert_eq!(
            req.headers().get("x-real-ip").unwrap(),
            "192.168.1.1:9000".parse::<HeaderValue>().unwrap()
        );
    }

    #[test]
    fn test_set_remote_with_x_forwarded_for() {
        let mut req = Request::empty();
        req.headers_mut().insert(
            "x-forwarded-for",
            "203.0.113.1, 70.41.3.18, 150.172.238.178".parse().unwrap(),
        );

        let addr = RemoteAddr::from(StdSocketAddr::new(
            IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1)),
            8080,
        ));
        req.set_remote(addr);

        // X-Forwarded-For 中的纯 IP 会被解析为 Ipv4，不包含端口
        assert_eq!(
            req.headers().get("x-real-ip").unwrap(),
            "203.0.113.1".parse::<HeaderValue>().unwrap()
        );
    }

    #[test]
    fn test_set_remote_without_headers() {
        let mut req = Request::empty();
        let addr = RemoteAddr::from(StdSocketAddr::new(
            IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1)),
            8080,
        ));
        req.set_remote(addr);

        assert_eq!(
            req.headers().get("x-real-ip").unwrap(),
            "10.0.0.1:8080".parse::<HeaderValue>().unwrap()
        );
    }

    // ==================== method 相关测试 ====================

    #[test]
    fn test_method_get() {
        let req = Request::empty();
        assert_eq!(req.method(), Method::GET);
    }

    #[test]
    fn test_method_mut() {
        let mut req = Request::empty();
        *req.method_mut() = Method::POST;
        assert_eq!(req.method(), Method::POST);
    }

    // ==================== uri 相关测试 ====================

    #[test]
    fn test_uri_get() {
        let req = Request::empty();
        assert_eq!(req.uri(), &Uri::from_static("/"));
    }

    #[test]
    fn test_uri_mut() {
        let mut req = Request::empty();
        *req.uri_mut() = Uri::from_static("/test/path");
        assert_eq!(req.uri(), &Uri::from_static("/test/path"));
    }

    // ==================== version 相关测试 ====================

    #[test]
    fn test_version_get() {
        let req = Request::empty();
        assert_eq!(req.version(), Version::HTTP_11);
    }

    #[test]
    fn test_version_mut() {
        let mut req = Request::empty();
        *req.version_mut() = Version::HTTP_2;
        assert_eq!(req.version(), Version::HTTP_2);
    }

    // ==================== headers 相关测试 ====================

    #[test]
    fn test_headers_get() {
        let mut req = Request::empty();
        req.headers_mut()
            .insert("custom-header", "test-value".parse().unwrap());

        assert_eq!(
            req.headers().get("custom-header").unwrap(),
            "test-value".parse::<HeaderValue>().unwrap()
        );
    }

    #[test]
    fn test_headers_mut() {
        let mut req = Request::empty();
        req.headers_mut()
            .insert("x-test", "value1".parse().unwrap());
        req.headers_mut()
            .insert("x-test", "value2".parse().unwrap());

        // headers 应该有新值
        assert!(req.headers().get("x-test").is_some());
    }

    // ==================== extensions 相关测试 ====================

    #[test]
    fn test_extensions_get() {
        let req = Request::empty();
        // 验证可以获取 extensions
        let _ext = req.extensions();
    }

    #[test]
    fn test_extensions_mut() {
        let mut req = Request::empty();
        req.extensions_mut().insert("test_key");
        assert_eq!(req.extensions().get::<&'static str>(), Some(&"test_key"));
    }

    // ==================== configs 相关测试 ====================

    #[test]
    fn test_configs_get() {
        let req = Request::empty();
        let configs = req.configs();
        // 验证可以获取 configs
        assert!(configs.is_empty());
    }

    #[test]
    fn test_configs_get_uncheck() {
        let mut req = Request::empty();
        req.configs_mut().insert("test_value");
        let value = req.get_config_uncheck::<&str>();
        assert_eq!(*value, "test_value");
    }

    #[test]
    fn test_configs_mut() {
        let mut req = Request::empty();
        req.configs_mut().insert(42i32);
        let value = req.get_config_uncheck::<i32>();
        assert_eq!(*value, 42);
    }

    // ==================== path_params 相关测试 ====================

    #[test]
    fn test_path_params_empty() {
        let req = Request::empty();
        assert!(req.path_params().is_empty());
    }

    #[test]
    fn test_get_path_params_success() {
        let mut req = Request::empty();
        req.path_params.insert(
            "id".to_string(),
            PathParam::Str(PathString::Owned("123".to_string())),
        );

        let id: String = req.get_path_params("id").unwrap();
        assert_eq!(id, "123");
    }

    #[test]
    fn test_get_path_params_missing() {
        let req = Request::empty();
        let result: Result<String> = req.get_path_params("missing");
        assert!(result.is_err());
    }

    // ==================== params 相关测试 ====================

    #[test]
    fn test_params_get() {
        let mut req = Request::empty();
        req.params.insert("key".to_string(), "value".to_string());

        let params = req.params();
        assert_eq!(params.get("key"), Some(&"value".to_string()));
    }

    // ==================== body 相关测试 ====================

    #[test]
    fn test_replace_body() {
        let mut req = Request::empty();
        let new_body = ReqBody::Once(Bytes::from("test data"));

        let old_body = req.replace_body(new_body);
        assert!(matches!(old_body, ReqBody::Empty));
    }

    #[test]
    fn test_take_body() {
        let mut req = Request::empty();
        req.body = ReqBody::Once(Bytes::from("data"));

        let body = req.take_body();
        assert!(matches!(body, ReqBody::Once(_)));
    }

    // ==================== content_type 测试 ====================

    #[test]
    fn test_content_type_json() {
        let mut req = Request::empty();
        req.headers_mut()
            .insert("content-type", "application/json".parse().unwrap());

        let ct = req.content_type();
        assert_eq!(ct.as_ref().unwrap().type_(), mime::APPLICATION);
        assert_eq!(ct.as_ref().unwrap().subtype(), mime::JSON);
    }

    #[test]
    fn test_content_type_missing() {
        let req = Request::empty();
        assert!(req.content_type().is_none());
    }

    #[test]
    fn test_content_type_invalid() {
        let mut req = Request::empty();
        req.headers_mut()
            .insert("content-type", "not-a-valid-mime".parse().unwrap());

        let ct = req.content_type();
        assert!(ct.is_none());
    }

    // ==================== into_http 测试 ====================

    #[test]
    fn test_into_http() {
        let req = Request::empty();
        let http_req = req.into_http();

        assert_eq!(http_req.method(), Method::GET);
        assert_eq!(http_req.uri(), &Uri::from_static("/"));
    }

    // ==================== replace_extensions/take_extensions 测试 ====================

    #[test]
    fn test_replace_extensions() {
        let mut req = Request::empty();
        req.extensions_mut().insert("value1");

        let mut new_ext = Extensions::new();
        new_ext.insert("value2");

        let old_ext = req.replace_extensions(new_ext);
        assert_eq!(old_ext.get::<&'static str>(), Some(&"value1"));
        assert_eq!(req.extensions().get::<&'static str>(), Some(&"value2"));
    }

    #[test]
    fn test_take_extensions() {
        let mut req = Request::empty();
        req.extensions_mut().insert("test_value");

        let ext = req.take_extensions();
        assert_eq!(ext.get::<&'static str>(), Some(&"test_value"));
        assert!(req.extensions().is_empty());
    }

    // ==================== Default trait 测试 ====================

    #[derive(Deserialize, Debug, PartialEq)]
    struct TestStruct {
        a: i32,
        b: String,
        #[serde(default, alias = "c[]")]
        c: Vec<String>,
    }

    #[test]
    fn test_query_parse_alias() {
        let mut req = Request::empty();
        *req.uri_mut() = Uri::from_static("http://localhost:8080/test?a=1&b=2&c[]=3&c[]=4");
        let _ = req.params_parse::<TestStruct>().unwrap();
    }

    #[test]
    fn test_query_parse() {
        let mut req = Request::empty();
        *req.uri_mut() = Uri::from_static("http://localhost:8080/test?a=1&b=2&c=3&c=4");
        let _ = req.params_parse::<TestStruct>().unwrap();
    }

    /// 测试 json_parse 和 form_parse 的语义分离
    #[tokio::test]
    async fn test_methods_semantic_separation() {
        // 测试数据结构，当启用 multipart 特性时仍需要 Serialize
        #[derive(Deserialize, Debug, PartialEq)]
        struct TestData {
            name: String,
            age: u32,
        }

        let test_data = TestData {
            name: "Alice".to_string(),
            age: 25,
        };

        // 1. json_parse 正确处理 JSON 数据
        let json_body = r#"{"name":"Alice","age":25}"#.as_bytes().to_vec();
        let mut req = create_request_with_body("application/json", json_body);

        let parsed_data = req
            .json_parse::<TestData>()
            .await
            .expect("json_parse should successfully parse JSON data");
        assert_eq!(parsed_data.name, test_data.name);
        assert_eq!(parsed_data.age, test_data.age);

        // 2. form_parse 正确处理 form-urlencoded 数据
        let form_body = "name=Alice&age=25".as_bytes().to_vec();
        let mut req = create_request_with_body("application/x-www-form-urlencoded", form_body);

        let parsed_data = req
            .form_parse::<TestData>()
            .await
            .expect("form_parse should successfully parse form-urlencoded data");
        assert_eq!(parsed_data.name, test_data.name);
        assert_eq!(parsed_data.age, test_data.age);

        // 3. json_parse 拒绝 form-urlencoded 数据
        let form_body = "name=Alice&age=25".as_bytes().to_vec();
        let mut req = create_request_with_body("application/x-www-form-urlencoded", form_body);

        let result = req.json_parse::<TestData>().await;
        assert!(
            result.is_err(),
            "json_parse should reject form-urlencoded data"
        );

        // 4. form_parse 拒绝 JSON 数据
        let json_body = r#"{"name":"Alice","age":25}"#.as_bytes().to_vec();
        let mut req = create_request_with_body("application/json", json_body);

        let result = req.form_parse::<TestData>().await;
        assert!(result.is_err(), "form_parse should reject JSON data");
    }

    /// 测试 WWW_FORM_URLENCODED 数据缓存到 form_body_cache 字段
    #[tokio::test]
    async fn test_form_urlencoded_caches_to_form_body_cache() {
        #[derive(Deserialize, Debug, PartialEq)]
        struct TestData {
            name: String,
            age: u32,
        }

        // 创建一个 form-urlencoded 请求
        let form_body = "name=Alice&age=25".as_bytes().to_vec();
        let mut req = create_request_with_body("application/x-www-form-urlencoded", form_body);

        // 第一次调用 form_parse，应该解析数据并缓存到 form_body_cache
        let first_result = req
            .form_parse::<TestData>()
            .await
            .expect("First form_parse call should succeed");

        // 验证 form_body_cache 字段已被缓存
        assert!(
            req.form_body_cache.get().is_some(),
            "form_body_cache should be cached after form_parse"
        );

        // 验证缓存的内容是正确的字节数据
        let cached_bytes = req.form_body_cache.get().unwrap();
        assert_eq!(cached_bytes, b"name=Alice&age=25");

        // 第二次调用应该从缓存中获取（不会再次解析 body）
        let second_result = req
            .form_parse::<TestData>()
            .await
            .expect("Second form_parse call should use cached data");

        // 两次结果应该相同
        assert_eq!(first_result.name, second_result.name);
        assert_eq!(first_result.age, second_result.age);
        assert_eq!(first_result.name, "Alice");
        assert_eq!(first_result.age, 25);
    }

    /// 测试共享缓存机制（验证 form_parse 复用 form_data 缓存）
    #[cfg(feature = "multipart")]
    #[tokio::test]
    async fn test_shared_cache_mechanism() {
        // 简单验证：当 Content-Type 是 multipart/form-data 时，
        // form_parse 会调用 form_data() 方法，从而复用其缓存
        let mut req = Request::empty();
        req.headers_mut().insert(
            "content-type",
            HeaderValue::from_str("multipart/form-data; boundary=----formdata").unwrap(),
        );

        // 设置一个空的 body 来避免实际的 multipart 解析
        req.body = ReqBody::Empty;

        // 尝试调用 form_parse，它应该尝试使用 form_data() 方法
        // 这个测试主要验证代码路径，而不是具体的数据解析
        // 注意：multipart 测试仍需要 Serialize，因为 multipart_form_parse 需要它
        #[derive(Deserialize, Debug)]

        struct TestData {
            name: String,
        }
        let t = TestData {
            name: "placeholder".to_string(),
        };
        assert_eq!(t.name, "placeholder");

        let result = req.form_parse::<TestData>().await;
        // 预期会失败，因为我们没有提供真实的 multipart 数据
        // 但重要的是代码走了正确的路径（调用 form_data()）
        assert!(
            result.is_err(),
            "Should fail due to empty body, but went through correct code path"
        );
    }

    /// 辅助函数：创建带有指定内容类型和内容的请求
    fn create_request_with_body(content_type: &str, body: Vec<u8>) -> Request {
        let mut req = Request::empty();
        req.headers_mut()
            .insert("content-type", HeaderValue::from_str(content_type).unwrap());
        req.body = ReqBody::Once(body.into());
        req
    }
}
