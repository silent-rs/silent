use crate::headers::ContentType;
use crate::{Response, StatusCode};
use serde::Serialize;
use serde_json::Value;
use std::backtrace::Backtrace;
use std::io;
use thiserror::Error;

/// BoxedError
pub type BoxedError = Box<dyn std::error::Error + Send + Sync>;

/// SilentError is the error type for the `silent` library.
#[derive(Error, Debug)]
pub enum SilentError {
    /// IO 错误
    #[error("io error")]
    IOError(#[from] io::Error),
    /// 反序列化 错误
    #[error("serde_json error `{0}`")]
    SerdeJsonError(#[from] serde_json::Error),
    /// 反序列化 错误
    #[error("serde de error `{0}`")]
    SerdeDeError(#[from] serde::de::value::Error),
    /// Hyper 错误
    #[error("the data for key `{0}` is not available")]
    HyperError(#[from] hyper::Error),
    #[cfg(feature = "multipart")]
    /// 上传文件读取 错误
    #[error("upload file read error `{0}`")]
    FileEmpty(#[from] multer::Error),
    /// Body为空 错误
    #[error("body is empty")]
    BodyEmpty,
    /// Json为空 错误
    #[error("json is empty")]
    JsonEmpty,
    /// Content-Type 错误
    #[error("content-type is error")]
    ContentTypeError,
    /// Content-Type 缺失错误
    #[error("content-type is missing")]
    ContentTypeMissingError,
    /// Params为空 错误
    #[error("params is empty")]
    ParamsEmpty,
    /// Params为空 错误
    #[error("params not found")]
    ParamsNotFound,
    /// 配置不存在 错误
    #[error("config not found")]
    ConfigNotFound,
    /// websocket错误
    #[error("websocket error: {0}")]
    WsError(String),
    /// anyhow错误
    #[error("{0}")]
    AnyhowError(#[from] anyhow::Error),
    /// 业务错误
    #[error("business error: {msg} ({code})")]
    BusinessError {
        /// 错误码
        code: StatusCode,
        /// 错误信息
        msg: String,
    },
    #[error("not found")]
    NotFound,
}

pub type SilentResult<T> = Result<T, SilentError>;

impl From<(StatusCode, String)> for SilentError {
    fn from(value: (StatusCode, String)) -> Self {
        Self::business_error(value.0, value.1)
    }
}

impl From<(u16, String)> for SilentError {
    fn from(value: (u16, String)) -> Self {
        let code = StatusCode::from_u16(value.0).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
        Self::business_error(code, value.1)
    }
}

impl From<String> for SilentError {
    fn from(value: String) -> Self {
        Self::business_error(StatusCode::INTERNAL_SERVER_ERROR, value)
    }
}

impl From<BoxedError> for SilentError {
    fn from(value: BoxedError) -> Self {
        Self::business_error(StatusCode::INTERNAL_SERVER_ERROR, value.to_string())
    }
}

impl SilentError {
    pub fn business_error_obj<S>(code: StatusCode, msg: S) -> Self
    where
        S: Serialize,
    {
        let msg = serde_json::to_string(&msg).unwrap_or_default();
        Self::BusinessError { code, msg }
    }
    pub fn business_error<T: Into<String>>(code: StatusCode, msg: T) -> Self {
        Self::BusinessError {
            code,
            msg: msg.into(),
        }
    }
    pub fn status(&self) -> StatusCode {
        match self {
            Self::BusinessError { code, .. } => *code,
            Self::SerdeDeError(_) => StatusCode::UNPROCESSABLE_ENTITY,
            Self::SerdeJsonError(_) => StatusCode::UNPROCESSABLE_ENTITY,
            Self::NotFound => StatusCode::NOT_FOUND,
            Self::ContentTypeError => StatusCode::BAD_REQUEST,
            Self::BodyEmpty => StatusCode::BAD_REQUEST,
            Self::JsonEmpty => StatusCode::BAD_REQUEST,
            _ => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
    pub fn message(&self) -> String {
        match self {
            Self::BusinessError { msg, .. } => msg.clone(),
            Self::SerdeDeError(e) => e.to_string(),
            Self::SerdeJsonError(e) => e.to_string(),
            _ => self.to_string(),
        }
    }
    pub fn trace(&self) -> Backtrace {
        Backtrace::capture()
    }
}

impl From<SilentError> for Response {
    fn from(value: SilentError) -> Self {
        let mut res = Response::empty();
        res.set_status(value.status());
        if serde_json::from_str::<Value>(&value.message()).is_ok() {
            res.set_typed_header(ContentType::json());
        }
        res.set_body(value.message().into());
        res
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Response;
    use http_body_util::BodyExt;
    use hyper::StatusCode;
    use serde::de::Error as SerdeDeError;
    use serde_json::Value;
    use std::error::Error as StdError;
    use std::io;
    use tracing::info;

    #[derive(Serialize)]
    struct ResBody {
        code: u16,
        msg: String,
        data: Value,
    }

    #[tokio::test]
    async fn test_silent_error() {
        let res_body = ResBody {
            code: 400,
            msg: "bad request".to_string(),
            data: Value::Null,
        };
        let err = SilentError::business_error_obj(StatusCode::BAD_REQUEST, res_body);
        let mut res: Response = err.into();
        assert_eq!(res.status, StatusCode::BAD_REQUEST);
        info!("{:#?}", res.headers);
        info!(
            "{:#?}",
            res.body.frame().await.unwrap().unwrap().data_ref().unwrap()
        );
    }

    // ==================== From trait 测试 ====================

    #[test]
    fn test_from_io_error() {
        let io_err = io::Error::new(io::ErrorKind::NotFound, "file not found");
        let silent_err: SilentError = io_err.into();
        assert!(matches!(silent_err, SilentError::IOError(_)));
        assert_eq!(silent_err.status(), StatusCode::INTERNAL_SERVER_ERROR);
    }

    #[test]
    fn test_from_serde_json_error() {
        let json_err = serde_json::from_str::<Value>("invalid json").unwrap_err();
        let silent_err: SilentError = json_err.into();
        assert!(matches!(silent_err, SilentError::SerdeJsonError(_)));
        assert_eq!(silent_err.status(), StatusCode::UNPROCESSABLE_ENTITY);
    }

    #[test]
    fn test_from_serde_de_error() {
        let de_err = serde::de::value::Error::custom("custom error");
        let silent_err: SilentError = de_err.into();
        assert!(matches!(silent_err, SilentError::SerdeDeError(_)));
        assert_eq!(silent_err.status(), StatusCode::UNPROCESSABLE_ENTITY);
    }

    #[test]
    fn test_from_hyper_error() {
        // 使用其他方式创建 Hyper 错误
        // 这里我们可以从一个已有的错误转换，或者使用 try_into
        // 由于 hyper::Error::new 是私有的，我们跳过这个测试
        // 或者使用其他方式触发 hyper 错误
    }

    #[cfg(feature = "multipart")]
    #[test]
    fn test_from_multer_error() {
        // 创建一个 multer 错误（如果可能）
        // 这个测试可能需要实际的 multer 错误实例
    }

    #[test]
    fn test_from_status_code_string_tuple() {
        let tuple = (StatusCode::BAD_REQUEST, "Invalid input".to_string());
        let silent_err: SilentError = tuple.into();
        assert!(matches!(silent_err, SilentError::BusinessError { .. }));
        assert_eq!(silent_err.status(), StatusCode::BAD_REQUEST);
        assert_eq!(silent_err.message(), "Invalid input");
    }

    #[test]
    fn test_from_u16_string_tuple() {
        let tuple = (404u16, "Not found".to_string());
        let silent_err: SilentError = tuple.into();
        assert!(matches!(silent_err, SilentError::BusinessError { .. }));
        assert_eq!(silent_err.status(), StatusCode::NOT_FOUND);
        assert_eq!(silent_err.message(), "Not found");
    }

    #[test]
    fn test_from_u16_string_tuple_invalid() {
        let tuple = (9999u16, "Invalid status".to_string());
        let silent_err: SilentError = tuple.into();
        assert_eq!(silent_err.status(), StatusCode::INTERNAL_SERVER_ERROR);
    }

    #[test]
    fn test_from_string() {
        let msg = "Internal server error".to_string();
        let silent_err: SilentError = msg.into();
        assert!(matches!(silent_err, SilentError::BusinessError { .. }));
        assert_eq!(silent_err.status(), StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(silent_err.message(), "Internal server error");
    }

    #[test]
    fn test_from_boxed_error() {
        let io_err = io::Error::new(io::ErrorKind::PermissionDenied, "access denied");
        let boxed_err: BoxedError = Box::new(io_err);
        let silent_err: SilentError = boxed_err.into();
        assert!(matches!(silent_err, SilentError::BusinessError { .. }));
        assert_eq!(silent_err.status(), StatusCode::INTERNAL_SERVER_ERROR);
    }

    #[test]
    fn test_from_anyhow_error() {
        let anyhow_err = anyhow::anyhow!("Something went wrong");
        let silent_err: SilentError = anyhow_err.into();
        assert!(matches!(silent_err, SilentError::AnyhowError(_)));
        assert_eq!(silent_err.status(), StatusCode::INTERNAL_SERVER_ERROR);
    }

    // ==================== SilentError 变体测试 ====================

    #[test]
    fn test_body_empty() {
        let err = SilentError::BodyEmpty;
        assert_eq!(err.status(), StatusCode::BAD_REQUEST);
        assert_eq!(err.message(), "body is empty");
    }

    #[test]
    fn test_json_empty() {
        let err = SilentError::JsonEmpty;
        assert_eq!(err.status(), StatusCode::BAD_REQUEST);
        assert_eq!(err.message(), "json is empty");
    }

    #[test]
    fn test_content_type_error() {
        let err = SilentError::ContentTypeError;
        assert_eq!(err.status(), StatusCode::BAD_REQUEST);
        assert_eq!(err.message(), "content-type is error");
    }

    #[test]
    fn test_content_type_missing_error() {
        let err = SilentError::ContentTypeMissingError;
        assert_eq!(err.status(), StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(err.message(), "content-type is missing");
    }

    #[test]
    fn test_params_empty() {
        let err = SilentError::ParamsEmpty;
        assert_eq!(err.status(), StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(err.message(), "params is empty");
    }

    #[test]
    fn test_params_not_found() {
        let err = SilentError::ParamsNotFound;
        assert_eq!(err.status(), StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(err.message(), "params not found");
    }

    #[test]
    fn test_config_not_found() {
        let err = SilentError::ConfigNotFound;
        assert_eq!(err.status(), StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(err.message(), "config not found");
    }

    #[test]
    fn test_ws_error() {
        let err = SilentError::WsError("Connection closed".to_string());
        assert_eq!(err.status(), StatusCode::INTERNAL_SERVER_ERROR);
        assert!(err.message().contains("Connection closed"));
    }

    #[test]
    fn test_not_found() {
        let err = SilentError::NotFound;
        assert_eq!(err.status(), StatusCode::NOT_FOUND);
        assert_eq!(err.message(), "not found");
    }

    // ==================== business_error 方法测试 ====================

    #[test]
    fn test_business_error_with_string() {
        let err = SilentError::business_error(StatusCode::BAD_REQUEST, "Invalid data");
        assert!(matches!(err, SilentError::BusinessError { .. }));
        assert_eq!(err.status(), StatusCode::BAD_REQUEST);
        assert_eq!(err.message(), "Invalid data");
    }

    #[test]
    fn test_business_error_with_str() {
        let err = SilentError::business_error(StatusCode::UNAUTHORIZED, "Unauthorized");
        assert_eq!(err.status(), StatusCode::UNAUTHORIZED);
        assert_eq!(err.message(), "Unauthorized");
    }

    #[test]
    fn test_business_error_obj() {
        #[derive(Serialize)]
        struct ErrorDetail {
            field: String,
            reason: String,
        }

        let detail = ErrorDetail {
            field: "email".to_string(),
            reason: "Invalid format".to_string(),
        };

        let err = SilentError::business_error_obj(StatusCode::BAD_REQUEST, detail);
        assert!(matches!(err, SilentError::BusinessError { .. }));
        assert_eq!(err.status(), StatusCode::BAD_REQUEST);
        // message 应该是 JSON 字符串
        assert!(err.message().contains("email"));
        assert!(err.message().contains("Invalid format"));
    }

    // ==================== status() 方法测试 ====================

    #[test]
    fn test_status_for_business_error() {
        let err = SilentError::business_error(StatusCode::FORBIDDEN, "Forbidden");
        assert_eq!(err.status(), StatusCode::FORBIDDEN);
    }

    #[test]
    fn test_status_for_io_error() {
        let io_err = io::Error::new(io::ErrorKind::ConnectionRefused, "connection refused");
        let err: SilentError = io_err.into();
        assert_eq!(err.status(), StatusCode::INTERNAL_SERVER_ERROR);
    }

    // ==================== message() 方法测试 ====================

    #[test]
    fn test_message_for_business_error() {
        let err = SilentError::business_error(StatusCode::IM_A_TEAPOT, "I'm a teapot");
        assert_eq!(err.message(), "I'm a teapot");
    }

    #[test]
    fn test_message_for_serde_json_error() {
        let json_err = serde_json::from_str::<Value>("{invalid}").unwrap_err();
        let err: SilentError = json_err.into();
        assert!(!err.message().is_empty());
    }

    #[test]
    fn test_message_for_serde_de_error() {
        let de_err = serde::de::value::Error::custom("deserialization failed");
        let err: SilentError = de_err.into();
        assert_eq!(err.message(), "deserialization failed");
    }

    // ==================== trace() 方法测试 ====================

    #[test]
    fn test_trace() {
        let err = SilentError::BodyEmpty;
        let backtrace = err.trace();
        // Backtrace 应该能够被捕获
        let _ = format!("{:?}", backtrace);
    }

    // ==================== From<SilentError> for Response 测试 ====================

    #[tokio::test]
    async fn test_response_from_business_error() {
        let err = SilentError::business_error(StatusCode::BAD_REQUEST, "Bad request");
        let res: Response = err.into();
        assert_eq!(res.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_response_from_body_empty() {
        let err = SilentError::BodyEmpty;
        let res: Response = err.into();
        assert_eq!(res.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_response_from_json_empty() {
        let err = SilentError::JsonEmpty;
        let res: Response = err.into();
        assert_eq!(res.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_response_from_content_type_error() {
        let err = SilentError::ContentTypeError;
        let res: Response = err.into();
        assert_eq!(res.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_response_from_not_found() {
        let err = SilentError::NotFound;
        let res: Response = err.into();
        assert_eq!(res.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_response_from_ws_error() {
        let err = SilentError::WsError("WebSocket handshake failed".to_string());
        let res: Response = err.into();
        assert_eq!(res.status(), StatusCode::INTERNAL_SERVER_ERROR);
    }

    #[tokio::test]
    async fn test_response_from_serde_json_error() {
        let json_err = serde_json::from_str::<Value>("invalid").unwrap_err();
        let err: SilentError = json_err.into();
        let res: Response = err.into();
        assert_eq!(res.status(), StatusCode::UNPROCESSABLE_ENTITY);
    }

    #[tokio::test]
    async fn test_response_from_serde_de_error() {
        let de_err = serde::de::value::Error::custom("custom error");
        let err: SilentError = de_err.into();
        let res: Response = err.into();
        assert_eq!(res.status(), StatusCode::UNPROCESSABLE_ENTITY);
    }

    #[tokio::test]
    async fn test_response_with_json_message() {
        let err = SilentError::business_error_obj(
            StatusCode::BAD_REQUEST,
            serde_json::json!({"error": "Invalid input"}),
        );
        let res: Response = err.into();
        assert_eq!(res.status(), StatusCode::BAD_REQUEST);
        // 应该设置 Content-Type 为 application/json
    }

    #[tokio::test]
    async fn test_response_with_non_json_message() {
        let err =
            SilentError::business_error(StatusCode::INTERNAL_SERVER_ERROR, "Plain text error");
        let res: Response = err.into();
        assert_eq!(res.status(), StatusCode::INTERNAL_SERVER_ERROR);
        // 不应该设置 JSON Content-Type
    }

    // ==================== 错误显示测试 ====================

    #[test]
    fn test_error_display() {
        let err = SilentError::BodyEmpty;
        let display_str = format!("{}", err);
        assert_eq!(display_str, "body is empty");
    }

    #[test]
    fn test_error_debug() {
        let err = SilentError::JsonEmpty;
        let debug_str = format!("{:?}", err);
        assert!(debug_str.contains("JsonEmpty"));
    }

    // ==================== 综合测试 ====================

    #[test]
    fn test_error_chain() {
        let io_err = io::Error::new(io::ErrorKind::NotFound, "config.toml");
        let silent_err: SilentError = io_err.into();
        // 验证错误链
        assert!(silent_err.source().is_some());
    }

    #[test]
    fn test_multiple_error_conversions() {
        // 测试多种错误类型的转换
        let errors: Vec<SilentError> = vec![
            SilentError::BodyEmpty,
            SilentError::JsonEmpty,
            SilentError::ContentTypeError,
            SilentError::NotFound,
            SilentError::ParamsEmpty,
        ];

        for err in errors {
            let _status = err.status();
            let _msg = err.message();
            let _res: Response = err.into();
        }
    }
}
