//! Silent OpenAPI 错误处理
//!
//! 定义了OpenAPI相关操作可能出现的错误类型。

use thiserror::Error;

/// Silent OpenAPI操作相关的错误类型
#[derive(Error, Debug)]
pub enum OpenApiError {
    /// JSON序列化/反序列化错误
    #[error("JSON处理错误: {0}")]
    Json(#[from] serde_json::Error),

    /// Silent框架错误
    #[error("Silent框架错误: {0}")]
    Silent(#[from] silent::SilentError),

    /// OpenAPI文档生成错误
    #[error("OpenAPI文档生成错误: {0}")]
    OpenApiGeneration(String),

    /// Swagger UI资源错误
    #[error("Swagger UI资源错误: {0}")]
    SwaggerUi(String),

    /// HTTP错误
    #[error("HTTP错误: {0}")]
    Http(#[from] http::Error),

    /// 路径匹配错误
    #[error("路径匹配错误: {path}")]
    PathMismatch { path: String },

    /// 资源未找到
    #[error("资源未找到: {resource}")]
    ResourceNotFound { resource: String },

    /// 配置错误
    #[error("配置错误: {message}")]
    Configuration { message: String },
}

/// Silent OpenAPI的Result类型别名
pub type Result<T> = std::result::Result<T, OpenApiError>;

impl OpenApiError {
    /// 创建OpenAPI文档生成错误
    pub fn openapi_generation<S: Into<String>>(message: S) -> Self {
        Self::OpenApiGeneration(message.into())
    }

    /// 创建Swagger UI错误
    pub fn swagger_ui<S: Into<String>>(message: S) -> Self {
        Self::SwaggerUi(message.into())
    }

    /// 创建配置错误
    pub fn configuration<S: Into<String>>(message: S) -> Self {
        Self::Configuration {
            message: message.into(),
        }
    }

    /// 创建资源未找到错误
    pub fn resource_not_found<S: Into<String>>(resource: S) -> Self {
        Self::ResourceNotFound {
            resource: resource.into(),
        }
    }
}

impl From<OpenApiError> for silent::Response {
    fn from(error: OpenApiError) -> Self {
        use silent::StatusCode;

        let (status, message) = match &error {
            OpenApiError::ResourceNotFound { .. } => (StatusCode::NOT_FOUND, error.to_string()),
            OpenApiError::PathMismatch { .. } => (StatusCode::NOT_FOUND, error.to_string()),
            OpenApiError::Configuration { .. } => {
                (StatusCode::INTERNAL_SERVER_ERROR, error.to_string())
            }
            _ => (StatusCode::INTERNAL_SERVER_ERROR, error.to_string()),
        };

        let mut response = silent::Response::empty();
        response.set_status(status);
        response.set_body(message.into());
        response
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_creation() {
        let error = OpenApiError::openapi_generation("测试错误");
        assert!(error.to_string().contains("测试错误"));

        let error = OpenApiError::resource_not_found("swagger.json");
        assert!(error.to_string().contains("swagger.json"));
    }

    #[test]
    fn test_error_conversion_to_response() {
        let error = OpenApiError::resource_not_found("test.json");
        let _response: silent::Response = error.into();
        // 注意：Silent Response没有public的status方法，这里只验证转换成功
    }
}
