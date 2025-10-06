use http_body::Body;

/// 通用协议适配抽象
///
/// `Protocol` trait 将外部协议消息与框架内部处理流程解耦，允许按需指定
/// 内部使用的请求/响应类型，从而支持 HTTP、MQTT 等不同协议场景。
pub trait Protocol {
    /// 外部协议的请求载体（如 hyper::Request、MQTT 报文等）。
    type Incoming;
    /// 外部协议的响应载体。
    type Outgoing;
    /// 框架内部使用的响应体实现。
    type Body: Body;
    /// 框架内部处理的请求类型（如 `silent::Request` 或自定义上下文）。
    type InternalRequest;
    /// 框架内部处理的响应类型。
    type InternalResponse;

    /// 将外部协议请求转换为框架内部请求类型。
    fn into_internal(message: Self::Incoming) -> Self::InternalRequest;

    /// 将框架内部响应转换为外部协议响应。
    fn from_internal(response: Self::InternalResponse) -> Self::Outgoing;
}

#[cfg(feature = "server")]
pub mod hyper_http;
