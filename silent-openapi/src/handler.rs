//! Swagger UI 处理器
//!
//! 提供Swagger UI的处理器实现，可以直接作为Silent路由使用。

use crate::{OpenApiError, Result, SwaggerUiOptions};
use async_trait::async_trait;
use silent::{Handler, Request, Response, StatusCode};
use utoipa::openapi::OpenApi;

/// Swagger UI 处理器
///
/// 实现了Silent的Handler trait，可以直接添加到路由中。
/// 负责处理Swagger UI相关的所有请求，包括：
/// - Swagger UI 静态资源
/// - OpenAPI 规范JSON
/// - 重定向到Swagger UI主页
#[derive(Clone)]
pub struct SwaggerUiHandler {
    /// Swagger UI的基础路径
    ui_path: String,
    /// OpenAPI JSON的路径
    api_doc_path: String,
    /// OpenAPI 规范的JSON字符串
    openapi_json: String,
    /// UI 配置
    options: SwaggerUiOptions,
}

impl SwaggerUiHandler {
    /// 创建新的Swagger UI处理器
    ///
    /// # 参数
    ///
    /// - `ui_path`: Swagger UI的访问路径，如 "/swagger-ui"
    /// - `openapi`: OpenAPI规范对象
    ///
    /// # 示例
    ///
    /// ```rust
    /// use silent_openapi::SwaggerUiHandler;
    /// use utoipa::OpenApi;
    ///
    /// #[derive(OpenApi)]
    /// #[openapi(paths(), components(schemas()))]
    /// struct ApiDoc;
    ///
    /// let handler = SwaggerUiHandler::new("/swagger-ui", ApiDoc::openapi());
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
    ///
    /// # 参数
    ///
    /// - `ui_path`: Swagger UI的访问路径
    /// - `api_doc_path`: OpenAPI JSON的访问路径
    /// - `openapi`: OpenAPI规范对象
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

    /// 使用自定义选项创建处理器
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

    /// 检查请求路径是否匹配
    fn matches_path(&self, path: &str) -> bool {
        // 匹配以下情况：
        // 1. 完全匹配 ui_path (重定向到主页)
        // 2. 以 ui_path/ 开头的路径 (Swagger UI资源)
        // 3. 完全匹配 api_doc_path (OpenAPI JSON)
        path == self.ui_path
            || path.starts_with(&format!("{}/", self.ui_path))
            || path == self.api_doc_path
    }

    /// 处理OpenAPI JSON请求
    async fn handle_openapi_json(&self) -> Result<Response> {
        let mut response = Response::empty();
        response.set_status(StatusCode::OK);
        response.set_header(
            http::header::CONTENT_TYPE,
            http::HeaderValue::from_static("application/json; charset=utf-8"),
        );
        response.set_body(self.openapi_json.clone().into());
        Ok(response)
    }

    /// 处理Swagger UI重定向
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

    /// 处理Swagger UI资源请求
    async fn handle_ui_resource(&self, path: &str) -> Result<Response> {
        // 移除基础路径前缀，获取相对路径
        let relative_path = path
            .strip_prefix(&format!("{}/", self.ui_path))
            .unwrap_or("");

        // 处理根路径请求（显示Swagger UI主页）
        if relative_path.is_empty() || relative_path == "index.html" {
            return self.serve_swagger_ui_index().await;
        }

        // 处理其他静态资源
        self.serve_swagger_ui_asset(relative_path).await
    }

    /// 服务Swagger UI主页
    async fn serve_swagger_ui_index(&self) -> Result<Response> {
        // 生成Swagger UI的HTML页面
        let html = format!(
            r#"<!DOCTYPE html>
<html lang="zh-CN">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>Swagger UI</title>
    <link rel="stylesheet" type="text/css" href="https://unpkg.com/swagger-ui-dist@5.17.14/swagger-ui.css" />
    <link rel="icon" type="image/png" href="https://unpkg.com/swagger-ui-dist@5.17.14/favicon-32x32.png" sizes="32x32" />
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
            margin:0;
            background: #fafafa;
        }}
    </style>
</head>
<body>
    <div id="swagger-ui"></div>

    <script src="https://unpkg.com/swagger-ui-dist@5.17.14/swagger-ui-bundle.js"></script>
    <script src="https://unpkg.com/swagger-ui-dist@5.17.14/swagger-ui-standalone-preset.js"></script>
    <script>
        window.onload = function() {{
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
                tryItOutEnabled: {}
            }})
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
        response.set_body(html.into());
        Ok(response)
    }

    /// 服务Swagger UI静态资源
    async fn serve_swagger_ui_asset(&self, _asset_path: &str) -> Result<Response> {
        // 对于基础版本，我们使用CDN资源，所以这里返回404
        // 在后续版本中可以考虑嵌入静态资源
        let mut response = Response::empty();
        response.set_status(StatusCode::NOT_FOUND);
        response.set_body("Asset not found".into());
        Ok(response)
    }

    /// 将处理器转换为可直接挂载的 Route 树
    ///
    /// 自动在 `<ui_path>` 下注册以下路由（GET/HEAD）：
    /// - `<ui_path>`
    /// - `<ui_path>/openapi.json`
    /// - `<ui_path>/<path:**>`
    pub fn into_route(self) -> silent::prelude::Route {
        use silent::prelude::{HandlerGetter, Method, Route};
        use std::sync::Arc;

        let mount = self.ui_path.trim_start_matches('/');

        let base = Route::new(mount)
            .insert_handler(Method::GET, Arc::new(self.clone()))
            .insert_handler(Method::HEAD, Arc::new(self.clone()))
            .append(
                Route::new("<path:**>")
                    .insert_handler(Method::GET, Arc::new(self.clone()))
                    .insert_handler(Method::HEAD, Arc::new(self)),
            );

        Route::new("").append(base)
    }
}

// 允许在 Route::append 直接使用处理器
impl silent::prelude::RouterAdapt for SwaggerUiHandler {
    fn into_router(self) -> silent::prelude::Route {
        self.into_route()
    }
}

#[async_trait]
impl Handler for SwaggerUiHandler {
    async fn call(&self, req: Request) -> silent::Result<Response> {
        let path = req.uri().path();

        // 检查路径是否匹配
        if !self.matches_path(path) {
            return Err(silent::SilentError::NotFound);
        }

        let result = if path == self.api_doc_path {
            // 返回OpenAPI JSON
            self.handle_openapi_json().await
        } else if path == self.ui_path {
            // 重定向到Swagger UI主页
            self.handle_ui_redirect().await
        } else {
            // 处理Swagger UI资源
            self.handle_ui_resource(path).await
        };

        match result {
            Ok(response) => Ok(response),
            Err(e) => {
                eprintln!("Swagger UI处理错误: {}", e);
                Err(silent::SilentError::NotFound)
            }
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
    fn test_swagger_ui_handler_creation() {
        let handler = SwaggerUiHandler::new("/swagger-ui", TestApiDoc::openapi());
        assert!(handler.is_ok());

        let handler = handler.unwrap();
        assert_eq!(handler.ui_path, "/swagger-ui");
        assert_eq!(handler.api_doc_path, "/swagger-ui/openapi.json");
    }

    #[test]
    fn test_path_matching() {
        let handler = SwaggerUiHandler::new("/swagger-ui", TestApiDoc::openapi()).unwrap();

        assert!(handler.matches_path("/swagger-ui"));
        assert!(handler.matches_path("/swagger-ui/"));
        assert!(handler.matches_path("/swagger-ui/index.html"));
        assert!(handler.matches_path("/swagger-ui/openapi.json"));
        assert!(handler.matches_path("/swagger-ui/any/asset.js"));
        assert!(!handler.matches_path("/api/users"));
        assert!(!handler.matches_path("/swagger"));
    }

    #[tokio::test]
    async fn test_openapi_json_response() {
        let handler = SwaggerUiHandler::new("/swagger-ui", TestApiDoc::openapi()).unwrap();
        let response = handler.handle_openapi_json().await.unwrap();

        // 注意：Silent的Response没有public的status()方法
        // 这里只验证响应能成功创建
        assert!(response.headers().get(http::header::CONTENT_TYPE).is_some());
    }

    #[tokio::test]
    async fn test_call_openapi_json_via_dispatch() {
        let handler = SwaggerUiHandler::new("/docs", TestApiDoc::openapi()).unwrap();
        let mut req = Request::empty();
        *req.uri_mut() = http::Uri::from_static("http://localhost/docs/openapi.json");
        let resp = handler.call(req).await.unwrap();
        assert!(
            resp.headers()
                .get(http::header::CONTENT_TYPE)
                .map(|v| v.to_str().unwrap_or("").contains("application/json"))
                .unwrap_or(false)
        );
    }

    #[tokio::test]
    async fn test_call_redirect_and_asset() {
        let handler = SwaggerUiHandler::new("/docs", TestApiDoc::openapi()).unwrap();
        // 重定向
        let mut req = Request::empty();
        *req.uri_mut() = http::Uri::from_static("http://localhost/docs");
        let resp = handler.call(req).await.unwrap();
        assert!(resp.headers().get(http::header::LOCATION).is_some());

        // 静态资源404 分支可达
        let mut req2 = Request::empty();
        *req2.uri_mut() = http::Uri::from_static("http://localhost/docs/unknown.css");
        let _resp2 = handler.call(req2).await.unwrap();
    }

    #[tokio::test]
    async fn test_handle_ui_resource_index_html() {
        let handler = SwaggerUiHandler::new("/docs", TestApiDoc::openapi()).unwrap();
        let resp = handler
            .handle_ui_resource("/docs/index.html")
            .await
            .unwrap();
        let ct = resp.headers().get(http::header::CONTENT_TYPE).unwrap();
        assert!(ct.to_str().unwrap_or("").contains("text/html"));
    }

    #[tokio::test]
    async fn test_head_fallback_via_route() {
        // 使用 into_route 挂载后，通过 Route 执行 HEAD，验证可达（GET 回退 HEAD）。
        let handler = SwaggerUiHandler::new("/docs", TestApiDoc::openapi()).unwrap();
        let route = handler.into_route();
        let mut req = Request::empty();
        *req.method_mut() = http::Method::HEAD;
        *req.uri_mut() = http::Uri::from_static("http://localhost/docs/openapi.json");
        let resp = route.call(req).await.unwrap();
        assert!(resp.headers().get(http::header::CONTENT_TYPE).is_some());
    }
}

// 选项类型在 crate 根导出
