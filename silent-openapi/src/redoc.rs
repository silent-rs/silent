//! ReDoc 文档 UI
//!
//! 提供 ReDoc 风格的 API 文档页面，作为 Swagger UI 的替代选择。
//! 支持 Handler 和 Middleware 两种集成方式，与 Swagger UI 共用同一 OpenAPI JSON 端点。

use crate::{OpenApiError, Result};
use async_trait::async_trait;
use silent::{Handler, MiddleWareHandler, Next, Request, Response, StatusCode};
use utoipa::openapi::OpenApi;

const REDOC_VERSION: &str = "2.1.5";

/// 生成 ReDoc HTML 页面
fn generate_redoc_html(api_doc_url: &str) -> String {
    format!(
        r#"<!DOCTYPE html>
<html lang="zh-CN">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>API Documentation - ReDoc</title>
    <style>
        body {{ margin: 0; padding: 0; }}
    </style>
</head>
<body>
    <redoc spec-url='{api_doc_url}'></redoc>
    <script src="https://unpkg.com/redoc@{REDOC_VERSION}/bundles/redoc.standalone.js"></script>
</body>
</html>"#
    )
}

/// ReDoc 处理器
///
/// 以 Handler 方式提供 ReDoc 文档页面。
/// 可与 `SwaggerUiHandler` 并存，共用同一 OpenAPI JSON 路径。
#[derive(Clone)]
pub struct ReDocHandler {
    ui_path: String,
    api_doc_path: String,
    openapi_json: String,
}

impl ReDocHandler {
    /// 创建 ReDoc 处理器
    ///
    /// # 参数
    ///
    /// - `ui_path`: ReDoc 页面路径，如 "/redoc"
    /// - `openapi`: OpenAPI 规范对象
    pub fn new(ui_path: &str, openapi: OpenApi) -> Result<Self> {
        let api_doc_path = format!("{}/openapi.json", ui_path.trim_end_matches('/'));
        let openapi_json = serde_json::to_string_pretty(&openapi).map_err(OpenApiError::Json)?;

        Ok(Self {
            ui_path: ui_path.to_string(),
            api_doc_path,
            openapi_json,
        })
    }

    /// 使用自定义 OpenAPI JSON 路径（如与 Swagger UI 共用同一端点）
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
        })
    }

    fn matches_path(&self, path: &str) -> bool {
        path == self.ui_path
            || path.starts_with(&format!("{}/", self.ui_path))
            || path == self.api_doc_path
    }

    /// 将处理器转换为可直接挂载的 Route 树
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

impl silent::prelude::RouterAdapt for ReDocHandler {
    fn into_router(self) -> silent::prelude::Route {
        self.into_route()
    }
}

#[async_trait]
impl Handler for ReDocHandler {
    async fn call(&self, req: Request) -> silent::Result<Response> {
        let path = req.uri().path();

        if !self.matches_path(path) {
            return Err(silent::SilentError::NotFound);
        }

        if path == self.api_doc_path {
            let mut response = Response::empty();
            response.set_status(StatusCode::OK);
            response.set_header(
                http::header::CONTENT_TYPE,
                http::HeaderValue::from_static("application/json; charset=utf-8"),
            );
            response.set_body(self.openapi_json.clone().into());
            Ok(response)
        } else if path == self.ui_path {
            let mut response = Response::empty();
            response.set_status(StatusCode::MOVED_PERMANENTLY);
            response.set_header(
                http::header::LOCATION,
                http::HeaderValue::from_str(&format!("{}/", self.ui_path))
                    .unwrap_or_else(|_| http::HeaderValue::from_static("/")),
            );
            Ok(response)
        } else {
            let html = generate_redoc_html(&self.api_doc_path);
            let mut response = Response::empty();
            response.set_status(StatusCode::OK);
            response.set_header(
                http::header::CONTENT_TYPE,
                http::HeaderValue::from_static("text/html; charset=utf-8"),
            );
            response.set_body(html.into());
            Ok(response)
        }
    }
}

/// ReDoc 中间件
///
/// 以 Middleware 方式提供 ReDoc 文档页面。
/// 当请求匹配 ReDoc 路径时拦截返回，否则透传到下游处理器。
#[derive(Clone)]
pub struct ReDocMiddleware {
    ui_path: String,
    api_doc_path: String,
    openapi_json: String,
}

impl ReDocMiddleware {
    /// 创建 ReDoc 中间件
    pub fn new(ui_path: &str, openapi: OpenApi) -> Result<Self> {
        let api_doc_path = format!("{}/openapi.json", ui_path.trim_end_matches('/'));
        let openapi_json = serde_json::to_string_pretty(&openapi).map_err(OpenApiError::Json)?;

        Ok(Self {
            ui_path: ui_path.to_string(),
            api_doc_path,
            openapi_json,
        })
    }

    /// 使用自定义 OpenAPI JSON 路径
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
        })
    }

    fn matches_path(&self, path: &str) -> bool {
        path == self.ui_path
            || path.starts_with(&format!("{}/", self.ui_path))
            || path == self.api_doc_path
    }
}

#[async_trait]
impl MiddleWareHandler for ReDocMiddleware {
    async fn handle(&self, req: Request, next: &Next) -> silent::Result<Response> {
        let path = req.uri().path();

        if !self.matches_path(path) {
            return next.call(req).await;
        }

        if path == self.api_doc_path {
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
        } else if path == self.ui_path {
            let mut response = Response::empty();
            response.set_status(StatusCode::MOVED_PERMANENTLY);
            response.set_header(
                http::header::LOCATION,
                http::HeaderValue::from_str(&format!("{}/", self.ui_path))
                    .unwrap_or_else(|_| http::HeaderValue::from_static("/")),
            );
            Ok(response)
        } else {
            let html = generate_redoc_html(&self.api_doc_path);
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
    fn test_redoc_handler_creation() {
        let handler = ReDocHandler::new("/redoc", TestApiDoc::openapi());
        assert!(handler.is_ok());
        let handler = handler.unwrap();
        assert_eq!(handler.ui_path, "/redoc");
        assert_eq!(handler.api_doc_path, "/redoc/openapi.json");
    }

    #[test]
    fn test_redoc_handler_path_matching() {
        let handler = ReDocHandler::new("/redoc", TestApiDoc::openapi()).unwrap();
        assert!(handler.matches_path("/redoc"));
        assert!(handler.matches_path("/redoc/"));
        assert!(handler.matches_path("/redoc/openapi.json"));
        assert!(!handler.matches_path("/api/users"));
    }

    #[test]
    fn test_redoc_handler_custom_api_doc_path() {
        let handler = ReDocHandler::with_custom_api_doc_path(
            "/redoc",
            "/docs/openapi.json",
            TestApiDoc::openapi(),
        )
        .unwrap();
        assert_eq!(handler.api_doc_path, "/docs/openapi.json");
        assert!(handler.matches_path("/docs/openapi.json"));
    }

    #[tokio::test]
    async fn test_redoc_handler_openapi_json() {
        let handler = ReDocHandler::new("/redoc", TestApiDoc::openapi()).unwrap();
        let mut req = Request::empty();
        *req.uri_mut() = http::Uri::from_static("http://localhost/redoc/openapi.json");
        let resp = handler.call(req).await.unwrap();
        assert!(
            resp.headers()
                .get(http::header::CONTENT_TYPE)
                .map(|v| v.to_str().unwrap_or("").contains("application/json"))
                .unwrap_or(false)
        );
    }

    #[tokio::test]
    async fn test_redoc_handler_redirect() {
        let handler = ReDocHandler::new("/redoc", TestApiDoc::openapi()).unwrap();
        let mut req = Request::empty();
        *req.uri_mut() = http::Uri::from_static("http://localhost/redoc");
        let resp = handler.call(req).await.unwrap();
        assert!(resp.headers().get(http::header::LOCATION).is_some());
    }

    #[tokio::test]
    async fn test_redoc_handler_html_page() {
        let handler = ReDocHandler::new("/redoc", TestApiDoc::openapi()).unwrap();
        let mut req = Request::empty();
        *req.uri_mut() = http::Uri::from_static("http://localhost/redoc/");
        let resp = handler.call(req).await.unwrap();
        assert!(
            resp.headers()
                .get(http::header::CONTENT_TYPE)
                .map(|v| v.to_str().unwrap_or("").contains("text/html"))
                .unwrap_or(false)
        );
    }

    #[test]
    fn test_redoc_middleware_creation() {
        let mw = ReDocMiddleware::new("/redoc", TestApiDoc::openapi());
        assert!(mw.is_ok());
    }

    #[test]
    fn test_redoc_middleware_path_matching() {
        let mw = ReDocMiddleware::new("/redoc", TestApiDoc::openapi()).unwrap();
        assert!(mw.matches_path("/redoc"));
        assert!(mw.matches_path("/redoc/"));
        assert!(!mw.matches_path("/other"));
    }

    #[test]
    fn test_generate_redoc_html() {
        let html = generate_redoc_html("/api/openapi.json");
        assert!(html.contains("/api/openapi.json"));
        assert!(html.contains("redoc"));
        assert!(html.contains("redoc.standalone.js"));
    }

    #[tokio::test]
    async fn test_redoc_handler_into_route() {
        let handler = ReDocHandler::new("/redoc", TestApiDoc::openapi()).unwrap();
        let route = handler.into_route();
        let mut req = Request::empty();
        *req.uri_mut() = http::Uri::from_static("http://localhost/redoc/openapi.json");
        let resp = route.call(req).await.unwrap();
        assert!(resp.headers().get(http::header::CONTENT_TYPE).is_some());
    }
}
