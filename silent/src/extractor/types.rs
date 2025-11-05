use async_trait::async_trait;

use crate::Request;

use http::{Uri as HttpUri, Version as HttpVersion};

/// Path 萃取器：支持从路径参数中解析到单值或结构体
/// - 单值：当仅有一个路径参数时，使用 from_str_val 解析到目标类型
/// - 结构体：当存在多个路径参数时，按字段名匹配填充
pub struct Path<T>(pub T);

/// Query 萃取器：从 URL 查询参数解析为 T
pub struct Query<T>(pub T);

/// Json 萃取器：从 application/json 解析为 T（带缓存）
pub struct Json<T>(pub T);

/// Form 萃取器：从表单解析为 T
pub struct Form<T>(pub T);

#[allow(dead_code)]
pub struct Configs<T>(pub T);

/// 从 Extensions 中提取扩展
pub struct Extension<T>(pub T);

/// 头部类型化提取（等价 axum 的 TypedHeader）
pub struct TypedHeader<H>(pub H);

#[allow(dead_code)]
pub struct Method(pub crate::Method);
pub struct Uri(pub HttpUri);
pub struct Version(pub HttpVersion);
pub struct RemoteAddr(pub crate::core::socket_addr::SocketAddr);

/// Request 便捷扩展：通用萃取
#[async_trait]
pub trait RequestExt {
    async fn extract<T>(&mut self) -> Result<T, T::Rejection>
    where
        T: super::FromRequest + Send + 'static;
}

#[async_trait]
impl RequestExt for Request {
    async fn extract<T>(&mut self) -> Result<T, T::Rejection>
    where
        T: super::FromRequest + Send + 'static,
    {
        T::from_request(self).await
    }
}

/// QueryParam 萃取器：按名称提取单个查询参数
#[allow(dead_code)]
pub struct QueryParam<T> {
    param_name: &'static str,
    value: T,
}

/// PathParam 萃取器：按名称提取单个路径参数
#[allow(dead_code)]
pub struct PathParam<T> {
    param_name: &'static str,
    value: T,
}

/// HeaderParam 萃取器：按名称提取单个请求头
#[allow(dead_code)]
pub struct HeaderParam<T> {
    param_name: &'static str,
    value: T,
}

/// CookieParam 萃取器：按名称提取单个 Cookie
#[allow(dead_code)]
pub struct CookieParam<T> {
    param_name: &'static str,
    value: T,
}
