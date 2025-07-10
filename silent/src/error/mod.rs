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
        Self::business_error(
            StatusCode::from_u16(value.0).expect("invalid status code"),
            value.1,
        )
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
    use serde_json::Value;
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
}
