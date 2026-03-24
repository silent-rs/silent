//! Swagger UI HTML 生成与静态资源服务
//!
//! 统一管理 Handler 和 Middleware 共用的 HTML 模板与资源分发逻辑。
//! 当启用 `swagger-ui-embedded` feature 时，静态资源从编译时嵌入的二进制数据中读取；
//! 否则从 unpkg CDN 加载。

use crate::{Result, SwaggerUiOptions};
use silent::{Response, StatusCode};

/// CDN 版本号（与 build.rs 中保持一致）
const SWAGGER_UI_VERSION: &str = "5.17.14";

/// 生成 Swagger UI 主页 HTML
///
/// 根据是否启用 `swagger-ui-embedded` feature，
/// 自动选择从本地相对路径或 CDN 加载静态资源。
pub fn generate_index_html(
    ui_path: &str,
    api_doc_path: &str,
    options: &SwaggerUiOptions,
) -> String {
    let ui_base = ui_path.trim_end_matches('/');

    let (css_href, favicon_href, bundle_src, preset_src) = if cfg!(feature = "swagger-ui-embedded")
    {
        (
            format!("{ui_base}/swagger-ui.css"),
            format!("{ui_base}/favicon-32x32.png"),
            format!("{ui_base}/swagger-ui-bundle.js"),
            format!("{ui_base}/swagger-ui-standalone-preset.js"),
        )
    } else {
        let cdn = format!("https://unpkg.com/swagger-ui-dist@{SWAGGER_UI_VERSION}");
        (
            format!("{cdn}/swagger-ui.css"),
            format!("{cdn}/favicon-32x32.png"),
            format!("{cdn}/swagger-ui-bundle.js"),
            format!("{cdn}/swagger-ui-standalone-preset.js"),
        )
    };

    let try_it_out = if options.try_it_out_enabled {
        "true"
    } else {
        "false"
    };

    format!(
        r#"<!DOCTYPE html>
<html lang="zh-CN">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>API Documentation - Swagger UI</title>
    <link rel="stylesheet" type="text/css" href="{css_href}" />
    <link rel="icon" type="image/png" href="{favicon_href}" sizes="32x32" />
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
        <h1>Silent Framework API Documentation</h1>
        <p>OpenAPI 3.0</p>
    </div>
    <div id="swagger-ui"></div>

    <script src="{bundle_src}"></script>
    <script src="{preset_src}"></script>
    <script>
        window.onload = function() {{
            const ui = SwaggerUIBundle({{
                url: '{api_doc_path}',
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
                tryItOutEnabled: {try_it_out}
            }});
            window.ui = ui;
        }}
    </script>
</body>
</html>"#
    )
}

/// 服务 Swagger UI 静态资源
///
/// 当 `swagger-ui-embedded` feature 启用时，从嵌入的二进制数据中返回资源；
/// 否则返回 404（由 CDN 直接在浏览器端加载）。
pub fn serve_asset(#[allow(unused_variables)] asset_path: &str) -> Result<Response> {
    #[cfg(feature = "swagger-ui-embedded")]
    {
        if let Some((content_type, data)) = crate::embedded::get_embedded_asset(asset_path) {
            let mut response = Response::empty();
            response.set_status(StatusCode::OK);
            response.set_header(
                http::header::CONTENT_TYPE,
                http::HeaderValue::from_static(content_type),
            );
            response.set_header(
                http::header::CACHE_CONTROL,
                http::HeaderValue::from_static("public, max-age=86400"),
            );
            response.set_body(data.to_vec().into());
            return Ok(response);
        }
    }

    let mut response = Response::empty();
    response.set_status(StatusCode::NOT_FOUND);
    response.set_body("Asset not found".into());
    Ok(response)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_index_html_contains_api_doc_path() {
        let html = generate_index_html("/docs", "/docs/openapi.json", &SwaggerUiOptions::default());
        assert!(html.contains("/docs/openapi.json"));
        assert!(html.contains("swagger-ui"));
    }

    #[test]
    fn test_generate_index_html_try_it_out_disabled() {
        let options = SwaggerUiOptions {
            try_it_out_enabled: false,
        };
        let html = generate_index_html("/docs", "/docs/openapi.json", &options);
        assert!(html.contains("tryItOutEnabled: false"));
    }

    #[test]
    fn test_generate_index_html_resource_source() {
        let html = generate_index_html("/docs", "/docs/openapi.json", &SwaggerUiOptions::default());

        if cfg!(feature = "swagger-ui-embedded") {
            // 嵌入模式：使用相对路径
            assert!(html.contains("/docs/swagger-ui-bundle.js"));
            assert!(html.contains("/docs/swagger-ui.css"));
        } else {
            // CDN 模式：使用 unpkg
            assert!(html.contains("unpkg.com/swagger-ui-dist@"));
        }
    }

    #[test]
    fn test_serve_asset_not_found() {
        // 不存在的资源应返回成功（404 响应体）
        let resp = serve_asset("nonexistent.file");
        assert!(resp.is_ok());
    }
}
