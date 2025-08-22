//! Swagger UI 中间件
//!
//! 提供中间件形式的Swagger UI支持，可以更灵活地集成到现有路由中。

use crate::{OpenApiError, Result, SwaggerUiOptions};
use async_trait::async_trait;
use silent::{MiddleWareHandler, Next, Request, Response, StatusCode};
use utoipa::openapi::OpenApi;

/// Swagger UI 中间件
///
/// 实现了Silent的MiddleWareHandler trait，可以作为中间件添加到路由中。
/// 当请求匹配Swagger UI相关路径时，直接返回响应；否则继续执行后续处理器。
#[derive(Clone)]
pub struct SwaggerUiMiddleware {
    /// Swagger UI的基础路径
    ui_path: String,
    /// OpenAPI JSON的路径
    api_doc_path: String,
    /// OpenAPI 规范的JSON字符串
    openapi_json: String,
    /// UI 配置
    options: SwaggerUiOptions,
}

impl SwaggerUiMiddleware {
    /// 创建新的Swagger UI中间件
    ///
    /// # 参数
    ///
    /// - `ui_path`: Swagger UI的访问路径，如 "/swagger-ui"
    /// - `openapi`: OpenAPI规范对象
    ///
    /// # 示例
    ///
    /// ```rust
    /// use silent::prelude::*;
    /// use silent_openapi::SwaggerUiMiddleware;
    /// use utoipa::OpenApi;
    ///
    /// #[derive(OpenApi)]
    /// #[openapi(paths(), components(schemas()))]
    /// struct ApiDoc;
    ///
    /// let middleware = SwaggerUiMiddleware::new("/swagger-ui", ApiDoc::openapi());
    ///
    /// let route = Route::new("")
    ///     .hook(middleware)
    ///     .get(your_handler);
    /// ```
    pub fn new(ui_path: &str, openapi: OpenApi) -> Result<Self> {
        let api_doc_path = format!("{}/openapi.json", ui_path.trim_end_matches('/'));
        let openapi_json = serde_json::to_string_pretty(&openapi).map_err(OpenApiError::Json)?;

        Ok(Self {
            ui_path: ui_path.to_string(),
            api_doc_path,
            openapi_json,
            options: SwaggerUiOptions::default(),
        })
    }

    /// 使用自定义的API文档路径
    pub fn with_custom_api_doc_path(
        ui_path: &str,
        api_doc_path: &str,
        openapi: OpenApi,
    ) -> Result<Self> {
        let openapi_json = serde_json::to_string_pretty(&openapi).map_err(OpenApiError::Json)?;

        Ok(Self {
            ui_path: ui_path.to_string(),
            api_doc_path: api_doc_path.to_string(),
            openapi_json,
            options: SwaggerUiOptions::default(),
        })
    }

    /// 使用自定义选项创建中间件
    pub fn with_options(
        ui_path: &str,
        openapi: OpenApi,
        options: SwaggerUiOptions,
    ) -> Result<Self> {
        let api_doc_path = format!("{}/openapi.json", ui_path.trim_end_matches('/'));
        let openapi_json = serde_json::to_string_pretty(&openapi).map_err(OpenApiError::Json)?;

        Ok(Self {
            ui_path: ui_path.to_string(),
            api_doc_path,
            openapi_json,
            options,
        })
    }

    /// 检查请求路径是否匹配Swagger UI相关路径
    fn matches_swagger_path(&self, path: &str) -> bool {
        path == self.ui_path
            || path.starts_with(&format!("{}/", self.ui_path))
            || path == self.api_doc_path
    }

    /// 处理Swagger UI相关请求
    async fn handle_swagger_request(&self, path: &str) -> Result<Response> {
        if path == self.api_doc_path {
            self.handle_openapi_json().await
        } else if path == self.ui_path {
            self.handle_ui_redirect().await
        } else {
            self.handle_ui_resource(path).await
        }
    }

    /// 处理OpenAPI JSON请求
    async fn handle_openapi_json(&self) -> Result<Response> {
        let mut response = Response::empty();
        response.set_status(StatusCode::OK);
        response.set_header(
            http::header::CONTENT_TYPE,
            http::HeaderValue::from_static("application/json; charset=utf-8"),
        );
        response.set_header(
            http::header::ACCESS_CONTROL_ALLOW_ORIGIN,
            http::HeaderValue::from_static("*"),
        );
        response.set_body(self.openapi_json.clone().into());
        Ok(response)
    }

    /// 处理UI主页重定向
    async fn handle_ui_redirect(&self) -> Result<Response> {
        let redirect_url = format!("{}/", self.ui_path);
        let mut response = Response::empty();
        response.set_status(StatusCode::MOVED_PERMANENTLY);
        response.set_header(
            http::header::LOCATION,
            http::HeaderValue::from_str(&redirect_url)
                .unwrap_or_else(|_| http::HeaderValue::from_static("/")),
        );
        Ok(response)
    }

    /// 处理UI资源请求
    async fn handle_ui_resource(&self, path: &str) -> Result<Response> {
        let relative_path = path
            .strip_prefix(&format!("{}/", self.ui_path))
            .unwrap_or("");

        if relative_path.is_empty() || relative_path == "index.html" {
            self.serve_swagger_ui_index().await
        } else {
            // 对于其他资源，返回404（基础版本使用CDN）
            let mut response = Response::empty();
            response.set_status(StatusCode::NOT_FOUND);
            response.set_body("Resource not found".into());
            Ok(response)
        }
    }

    /// 生成Swagger UI主页HTML
    async fn serve_swagger_ui_index(&self) -> Result<Response> {
        let html = format!(
            r#"<!DOCTYPE html>
<html lang="zh-CN">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>API Documentation - Swagger UI</title>
    <link rel="stylesheet" type="text/css" href="https://unpkg.com/swagger-ui-dist@4.15.5/swagger-ui.css" />
    <link rel="icon" type="image/png" href="https://unpkg.com/swagger-ui-dist@4.15.5/favicon-32x32.png" sizes="32x32" />
    <style>
        html {{
            box-sizing: border-box;
            overflow: -moz-scrollbars-vertical;
            overflow-y: scroll;
        }}
        *, *:before, *:after {{
            box-sizing: inherit;
        }}
        body {{
            margin: 0;
            background: #fafafa;
        }}
        .swagger-ui .topbar {{
            display: none;
        }}
        .swagger-ui .info {{
            margin: 50px 0;
        }}
        .custom-header {{
            background: #89CFF0;
            padding: 20px;
            text-align: center;
            color: #1976d2;
            font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif;
        }}
        .custom-header h1 {{
            margin: 0;
            font-size: 24px;
            font-weight: 600;
        }}
        .custom-header p {{
            margin: 8px 0 0 0;
            opacity: 0.8;
        }}
    </style>
</head>
<body>
    <div class="custom-header">
        <h1>🚀 Silent Framework API Documentation</h1>
        <p>基于 OpenAPI 3.0 规范的交互式 API 文档</p>
    </div>
    <div id="swagger-ui"></div>

    <script src="https://unpkg.com/swagger-ui-dist@4.15.5/swagger-ui-bundle.js"></script>
    <script src="https://unpkg.com/swagger-ui-dist@4.15.5/swagger-ui-standalone-preset.js"></script>
    <script>
        window.onload = function() {{
            // 配置Swagger UI
            const ui = SwaggerUIBundle({{
                url: '{}',
                dom_id: '#swagger-ui',
                deepLinking: true,
                presets: [
                    SwaggerUIBundle.presets.apis,
                    SwaggerUIStandalonePreset
                ],
                plugins: [
                    SwaggerUIBundle.plugins.DownloadUrl
                ],
                layout: "StandaloneLayout",
                validatorUrl: null,
                docExpansion: "list",
                defaultModelsExpandDepth: 1,
                defaultModelExpandDepth: 1,
                displayRequestDuration: true,
                filter: true,
                showExtensions: true,
                showCommonExtensions: true,
                tryItOutEnabled: {}
            }});

            // 添加自定义样式
            window.ui = ui;
        }}
    </script>
</body>
</html>"#,
            self.api_doc_path,
            if self.options.try_it_out_enabled {
                "true"
            } else {
                "false"
            }
        );

        let mut response = Response::empty();
        response.set_status(StatusCode::OK);
        response.set_header(
            http::header::CONTENT_TYPE,
            http::HeaderValue::from_static("text/html; charset=utf-8"),
        );
        response.set_header(
            http::header::CACHE_CONTROL,
            http::HeaderValue::from_static("no-cache, no-store, must-revalidate"),
        );
        response.set_body(html.into());
        Ok(response)
    }
}

#[async_trait]
impl MiddleWareHandler for SwaggerUiMiddleware {
    /// 检查请求是否匹配Swagger UI路径
    async fn match_req(&self, req: &Request) -> bool {
        let path = req.uri().path();
        self.matches_swagger_path(path)
    }

    /// 处理匹配的请求
    async fn handle(&self, req: Request, _next: &Next) -> silent::Result<Response> {
        let path = req.uri().path();

        match self.handle_swagger_request(path).await {
            Ok(response) => Ok(response),
            Err(e) => {
                eprintln!("Swagger UI中间件处理错误: {}", e);
                // 返回适当的错误响应
                let mut response = Response::empty();
                response.set_status(StatusCode::INTERNAL_SERVER_ERROR);
                response.set_body(format!("Swagger UI Error: {}", e).into());
                Ok(response)
            }
        }
    }
}

/// 便捷函数：创建Swagger UI中间件并添加到路由
///
/// # 参数
///
/// - `route`: 要添加中间件的路由
/// - `ui_path`: Swagger UI的访问路径
/// - `openapi`: OpenAPI规范对象
///
/// # 示例
///
/// ```rust
/// use silent::prelude::*;
/// use silent_openapi::add_swagger_ui;
/// use utoipa::OpenApi;
///
/// #[derive(OpenApi)]
/// #[openapi(paths(), components(schemas()))]
/// struct ApiDoc;
///
/// let route = Route::new("api")
///     .get(some_handler);
///
/// let route_with_swagger = add_swagger_ui(route, "/docs", ApiDoc::openapi());
/// ```
pub fn add_swagger_ui(
    route: silent::prelude::Route,
    ui_path: &str,
    openapi: OpenApi,
) -> silent::prelude::Route {
    match SwaggerUiMiddleware::new(ui_path, openapi) {
        Ok(middleware) => route.hook(middleware),
        Err(e) => {
            eprintln!("创建Swagger UI中间件失败: {}", e);
            route
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use utoipa::OpenApi;

    #[derive(OpenApi)]
    #[openapi(
        info(title = "Test API", version = "1.0.0"),
        paths(),
        components(schemas())
    )]
    struct TestApiDoc;

    #[test]
    fn test_middleware_creation() {
        let middleware = SwaggerUiMiddleware::new("/docs", TestApiDoc::openapi());
        assert!(middleware.is_ok());

        let middleware = middleware.unwrap();
        assert_eq!(middleware.ui_path, "/docs");
        assert_eq!(middleware.api_doc_path, "/docs/openapi.json");
    }

    #[test]
    fn test_path_matching() {
        let middleware = SwaggerUiMiddleware::new("/docs", TestApiDoc::openapi()).unwrap();

        assert!(middleware.matches_swagger_path("/docs"));
        assert!(middleware.matches_swagger_path("/docs/"));
        assert!(middleware.matches_swagger_path("/docs/index.html"));
        assert!(middleware.matches_swagger_path("/docs/openapi.json"));
        assert!(!middleware.matches_swagger_path("/api/users"));
        assert!(!middleware.matches_swagger_path("/doc"));
    }

    #[tokio::test]
    async fn test_openapi_json_handling() {
        let middleware = SwaggerUiMiddleware::new("/docs", TestApiDoc::openapi()).unwrap();
        let response = middleware.handle_openapi_json().await.unwrap();

        // 验证Content-Type头（Silent Response没有public的status方法）
        let content_type = response.headers().get(http::header::CONTENT_TYPE);
        assert!(content_type.is_some());
    }
}

// 选项类型在 crate 根导出
