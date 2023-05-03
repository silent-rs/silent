use crate::StatusCode;
use std::io;
use thiserror::Error;

/// SilentError is the error type for the `silent` library.
#[derive(Error, Debug)]
pub enum SilentError {
    /// IO 错误
    #[error("io error")]
    IOError(#[from] io::Error),
    /// IO 错误
    #[error("serde_json error `{0}`")]
    SerdeJsonError(#[from] serde_json::Error),
    /// Hyper 错误
    #[error("the data for key `{0}` is not available")]
    HyperError(#[from] hyper::Error),
    /// Body为空 错误
    #[error("body is empty")]
    BodyEmpty,
    /// Params为空 错误
    #[error("params is empty")]
    ParamsEmpty,
    /// 业务错误
    #[error("business error: {msg} ({code})")]
    BusinessError {
        /// 错误码
        code: StatusCode,
        /// 错误信息
        msg: String,
    },
}
