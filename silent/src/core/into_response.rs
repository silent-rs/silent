use crate::Response;

/// 将类型转换为 HTTP 响应的 trait。
///
/// 这是 handler 返回值的核心抽象。任何实现了此 trait 的类型都可以作为
/// handler 的返回值或错误类型。
///
/// # 内置实现
///
/// 框架通过桥接 `Into<Response>` 自动为以下类型提供了 `IntoResponse` 实现：
/// - `Response` — 直接返回
/// - `String` / `&str` — 通过 `Serialize` 转为 JSON 响应
/// - `SilentError` — 转为对应状态码的错误响应
/// - 任何实现了 `Serialize` 的类型 — 自动 JSON 序列化
///
/// # 自定义错误示例
///
/// ```rust
/// use silent::{IntoResponse, Response, StatusCode};
///
/// enum AppError {
///     NotFound(String),
///     Internal(String),
/// }
///
/// impl IntoResponse for AppError {
///     fn into_response(self) -> Response {
///         match self {
///             AppError::NotFound(msg) => {
///                 Response::json(&serde_json::json!({"error": msg}))
///                     .with_status(StatusCode::NOT_FOUND)
///             }
///             AppError::Internal(msg) => {
///                 Response::json(&serde_json::json!({"error": msg}))
///                     .with_status(StatusCode::INTERNAL_SERVER_ERROR)
///             }
///         }
///     }
/// }
/// ```
pub trait IntoResponse {
    fn into_response(self) -> Response;
}

/// 桥接：任何实现了 `Into<Response>` 的类型自动获得 `IntoResponse`。
///
/// 这确保了向后兼容——现有的 `Serialize` 类型、`SilentError`、`Response`
/// 等所有已实现 `From<T> for Response` 的类型都无需修改。
impl<T: Into<Response>> IntoResponse for T {
    fn into_response(self) -> Response {
        self.into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::StatusCode;

    #[test]
    fn test_string_into_response() {
        let res = "hello".to_string().into_response();
        assert_eq!(res.status(), StatusCode::OK);
    }

    #[test]
    fn test_str_into_response() {
        let res = "hello".into_response();
        assert_eq!(res.status(), StatusCode::OK);
    }

    #[test]
    fn test_response_into_response() {
        let res = Response::empty().into_response();
        assert_eq!(res.status(), StatusCode::OK);
    }

    #[test]
    fn test_silent_error_into_response() {
        let err = crate::SilentError::business_error(StatusCode::NOT_FOUND, "not found");
        let res = err.into_response();
        assert_eq!(res.status(), StatusCode::NOT_FOUND);
    }

    // 自定义错误类型
    enum AppError {
        NotFound,
        BadRequest(String),
    }

    impl From<AppError> for Response {
        fn from(e: AppError) -> Self {
            match e {
                AppError::NotFound => Response::empty().with_status(StatusCode::NOT_FOUND),
                AppError::BadRequest(msg) => {
                    Response::text(&msg).with_status(StatusCode::BAD_REQUEST)
                }
            }
        }
    }

    #[test]
    fn test_custom_error_into_response() {
        let res = AppError::NotFound.into_response();
        assert_eq!(res.status(), StatusCode::NOT_FOUND);

        let res = AppError::BadRequest("invalid".to_string()).into_response();
        assert_eq!(res.status(), StatusCode::BAD_REQUEST);
    }
}
