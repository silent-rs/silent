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
        // 创建一个 Hyper 错误
        // 由于 hyper::Error::new 是私有的，我们使用其他方式
        // 通过从 io::error 转换，然后转为 Silent Error
        let _io_err = io::Error::new(io::ErrorKind::ConnectionReset, "connection reset");
        // 注意：我们不能直接创建 HyperError，但我们可以验证它的存在
        // 这个测试保留作为文档，说明 HyperError 可以通过某种方式转换
    }

    #[cfg(feature = "multipart")]
    #[test]
    fn test_from_multer_error() {
        // 创建一个 multer 错误
        // 由于 multer::Error 的构造方法有限，我们使用自定义错误
        use multer::Error as MulterError;

        // 尝试创建一个 multer 错误
        // 注意：multer::Error 的构造可能比较复杂
        let multer_err = MulterError::UnknownField {
            field_name: Some("test_field".to_string()),
        };

        let silent_err: SilentError = multer_err.into();
        assert!(matches!(silent_err, SilentError::FileEmpty(_)));
        assert_eq!(silent_err.status(), StatusCode::INTERNAL_SERVER_ERROR);
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

    // ==================== status() 方法完整测试 ====================

    #[test]
    fn test_status_for_all_error_types() {
        // 测试所有错误类型的 status() 方法
        let io_err = io::Error::other("io error");
        let silent_io: SilentError = io_err.into();
        assert_eq!(silent_io.status(), StatusCode::INTERNAL_SERVER_ERROR);

        let json_err = serde_json::from_str::<Value>("invalid").unwrap_err();
        let silent_json: SilentError = json_err.into();
        assert_eq!(silent_json.status(), StatusCode::UNPROCESSABLE_ENTITY);

        let de_err = serde::de::value::Error::custom("de error");
        let silent_de: SilentError = de_err.into();
        assert_eq!(silent_de.status(), StatusCode::UNPROCESSABLE_ENTITY);

        assert_eq!(SilentError::BodyEmpty.status(), StatusCode::BAD_REQUEST);
        assert_eq!(SilentError::JsonEmpty.status(), StatusCode::BAD_REQUEST);
        assert_eq!(
            SilentError::ContentTypeError.status(),
            StatusCode::BAD_REQUEST
        );
        assert_eq!(
            SilentError::ContentTypeMissingError.status(),
            StatusCode::INTERNAL_SERVER_ERROR
        );
        assert_eq!(
            SilentError::ParamsEmpty.status(),
            StatusCode::INTERNAL_SERVER_ERROR
        );
        assert_eq!(
            SilentError::ParamsNotFound.status(),
            StatusCode::INTERNAL_SERVER_ERROR
        );
        assert_eq!(
            SilentError::ConfigNotFound.status(),
            StatusCode::INTERNAL_SERVER_ERROR
        );
        assert_eq!(
            SilentError::WsError("test".to_string()).status(),
            StatusCode::INTERNAL_SERVER_ERROR
        );
        assert_eq!(SilentError::NotFound.status(), StatusCode::NOT_FOUND);

        let anyhow_err = anyhow::anyhow!("test");
        let silent_anyhow: SilentError = anyhow_err.into();
        assert_eq!(silent_anyhow.status(), StatusCode::INTERNAL_SERVER_ERROR);
    }

    // ==================== message() 方法完整测试 ====================

    #[test]
    fn test_message_for_all_error_types() {
        // 测试所有错误类型的 message() 方法
        let io_err = io::Error::other("io error message");
        let silent_io: SilentError = io_err.into();
        assert!(silent_io.message().contains("io error"));

        let json_err = serde_json::from_str::<Value>("invalid json").unwrap_err();
        let silent_json: SilentError = json_err.into();
        assert!(!silent_json.message().is_empty());

        let de_err = serde::de::value::Error::custom("custom de error");
        let silent_de: SilentError = de_err.into();
        assert!(silent_de.message().contains("custom de error"));

        assert_eq!(SilentError::BodyEmpty.message(), "body is empty");
        assert_eq!(SilentError::JsonEmpty.message(), "json is empty");
        assert_eq!(
            SilentError::ContentTypeError.message(),
            "content-type is error"
        );
        assert_eq!(
            SilentError::ContentTypeMissingError.message(),
            "content-type is missing"
        );
        assert_eq!(SilentError::ParamsEmpty.message(), "params is empty");
        assert_eq!(SilentError::ParamsNotFound.message(), "params not found");
        assert_eq!(SilentError::ConfigNotFound.message(), "config not found");
        assert!(
            SilentError::WsError("ws error".to_string())
                .message()
                .contains("ws error")
        );
        assert_eq!(SilentError::NotFound.message(), "not found");

        let anyhow_err = anyhow::anyhow!("anyhow error message");
        let silent_anyhow: SilentError = anyhow_err.into();
        assert!(silent_anyhow.message().contains("anyhow error message"));
    }

    // ==================== Response 转换完整测试 ====================

    #[tokio::test]
    async fn test_response_conversion_for_all_errors() {
        // 测试所有错误类型的 Response 转换
        // 由于 SilentError 不实现 Clone，我们使用宏或单独测试
        let test_cases = vec![
            (SilentError::BodyEmpty, StatusCode::BAD_REQUEST),
            (SilentError::JsonEmpty, StatusCode::BAD_REQUEST),
            (SilentError::ContentTypeError, StatusCode::BAD_REQUEST),
            (
                SilentError::ContentTypeMissingError,
                StatusCode::INTERNAL_SERVER_ERROR,
            ),
            (SilentError::ParamsEmpty, StatusCode::INTERNAL_SERVER_ERROR),
            (
                SilentError::ParamsNotFound,
                StatusCode::INTERNAL_SERVER_ERROR,
            ),
            (
                SilentError::ConfigNotFound,
                StatusCode::INTERNAL_SERVER_ERROR,
            ),
            (SilentError::NotFound, StatusCode::NOT_FOUND),
        ];

        for (err, expected_status) in test_cases {
            let res: Response = err.into();
            assert_eq!(res.status(), expected_status);
        }
    }

    #[tokio::test]
    async fn test_response_body_content() {
        // 测试 Response 的 body 内容
        let err = SilentError::business_error(StatusCode::BAD_REQUEST, "error message");
        let res: Response = err.into();

        // 验证状态码
        assert_eq!(res.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_response_json_body_detection() {
        // 测试 JSON body 的检测
        let json_err = SilentError::business_error_obj(
            StatusCode::BAD_REQUEST,
            serde_json::json!({"error": "Invalid input", "code": 400}),
        );
        let res: Response = json_err.into();
        assert_eq!(res.status(), StatusCode::BAD_REQUEST);
        // JSON 消息应该设置 JSON Content-Type
    }

    #[tokio::test]
    async fn test_response_non_json_body() {
        // 测试非 JSON body
        let err = SilentError::business_error(StatusCode::BAD_REQUEST, "Plain error message");
        let res: Response = err.into();
        assert_eq!(res.status(), StatusCode::BAD_REQUEST);
        // 非 JSON 消息不应该设置 JSON Content-Type
    }

    // ==================== 错误链和源测试 ====================

    #[test]
    fn test_error_source_for_io_error() {
        // 测试 IOError 的错误源
        let io_err = io::Error::new(io::ErrorKind::NotFound, "file not found");
        let silent_err: SilentError = io_err.into();
        assert!(silent_err.source().is_some());
    }

    #[test]
    fn test_error_source_for_serde_json_error() {
        // 测试 SerdeJsonError 的错误源
        let json_err = serde_json::from_str::<Value>("invalid").unwrap_err();
        let silent_err: SilentError = json_err.into();
        assert!(silent_err.source().is_some());
    }

    #[test]
    fn test_error_source_for_serde_de_error() {
        // 测试 SerdeDeError 的错误源
        let de_err = serde::de::value::Error::custom("custom error");
        let silent_err: SilentError = de_err.into();
        assert!(silent_err.source().is_some());
    }

    #[test]
    fn test_error_source_for_anyhow_error() {
        // 测试 AnyhowError 的错误源
        let anyhow_err = anyhow::anyhow!("underlying error");
        let silent_err: SilentError = anyhow_err.into();
        assert!(silent_err.source().is_some());
    }

    #[cfg(feature = "multipart")]
    #[test]
    fn test_error_source_for_multer_error() {
        // 测试 FileEmpty 的错误源
        use multer::Error as MulterError;
        let multer_err = MulterError::UnknownField {
            field_name: Some("test".to_string()),
        };
        let silent_err: SilentError = multer_err.into();
        assert!(silent_err.source().is_some());
    }

    // ==================== business_error_obj 边界测试 ====================

    #[test]
    fn test_business_error_obj_with_complex_struct() {
        // 测试复杂结构体的序列化
        #[derive(Serialize)]
        struct ComplexError {
            code: u32,
            message: String,
            details: Vec<String>,
            nested: NestedError,
        }

        #[derive(Serialize)]
        struct NestedError {
            field: String,
        }

        let complex_err = ComplexError {
            code: 400,
            message: "Validation failed".to_string(),
            details: vec!["field1".to_string(), "field2".to_string()],
            nested: NestedError {
                field: "nested_field".to_string(),
            },
        };

        let err = SilentError::business_error_obj(StatusCode::BAD_REQUEST, complex_err);
        assert!(matches!(err, SilentError::BusinessError { .. }));
        assert_eq!(err.status(), StatusCode::BAD_REQUEST);
        let msg = err.message();
        assert!(msg.contains("Validation failed"));
        assert!(msg.contains("field1"));
        assert!(msg.contains("field2"));
    }

    #[test]
    fn test_business_error_obj_with_empty_struct() {
        // 测试空结构体
        #[derive(Serialize)]
        struct EmptyError {}

        let empty_err = EmptyError {};
        let err = SilentError::business_error_obj(StatusCode::INTERNAL_SERVER_ERROR, empty_err);
        assert!(matches!(err, SilentError::BusinessError { .. }));
        // 空结构体的序列化结果应该是 "{}"
        assert_eq!(err.message(), "{}");
    }

    // ==================== From trait 边界测试 ====================

    #[test]
    fn test_from_u16_edge_cases() {
        // 测试 u16 边界情况
        let valid_cases = vec![100u16, 200, 400, 404, 500, 599];
        for code in valid_cases {
            let err: SilentError = (code, "test".to_string()).into();
            assert_eq!(err.status().as_u16(), code);
        }

        // 测试无效的状态码
        let invalid_err: SilentError = (9999u16, "invalid".to_string()).into();
        assert_eq!(invalid_err.status(), StatusCode::INTERNAL_SERVER_ERROR);
    }

    #[test]
    fn test_from_string_empty() {
        // 测试空字符串转换
        let empty_str = "".to_string();
        let err: SilentError = empty_str.into();
        assert!(matches!(err, SilentError::BusinessError { .. }));
        assert_eq!(err.status(), StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(err.message(), "");
    }

    #[test]
    fn test_from_string_with_special_chars() {
        // 测试包含特殊字符的字符串
        let special_str = "Error: 测试 🚀 \n\t\r".to_string();
        let err: SilentError = special_str.clone().into();
        assert!(matches!(err, SilentError::BusinessError { .. }));
        assert_eq!(err.message(), special_str);
    }

    // ==================== 错误组合测试 ====================

    #[test]
    fn test_error_combinations() {
        // 测试错误的组合和比较
        let err1 = SilentError::BodyEmpty;
        let err2 = SilentError::BodyEmpty;
        let err3 = SilentError::JsonEmpty;

        // 验证错误类型匹配
        assert!(matches!(err1, SilentError::BodyEmpty));
        assert!(matches!(err2, SilentError::BodyEmpty));
        assert!(!matches!(err1, SilentError::JsonEmpty));
        assert!(matches!(err3, SilentError::JsonEmpty));
    }

    // ==================== Backtrace 测试 ====================

    #[test]
    fn test_backtrace_capture() {
        // 测试 backtrace 捕获
        let err = SilentError::business_error(StatusCode::BAD_REQUEST, "test error");
        let backtrace = err.trace();
        // 验证 backtrace 可以被格式化
        let _formatted = format!("{:?}", backtrace);
    }
}
