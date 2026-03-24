//! Swagger UI 嵌入式静态资源
//!
//! 当启用 `swagger-ui-embedded` feature 时，Swagger UI 的静态资源会在编译时
//! 嵌入到二进制文件中，无需依赖外部 CDN。

/// Swagger UI Bundle JS
pub const SWAGGER_UI_BUNDLE_JS: &[u8] =
    include_bytes!(concat!(env!("OUT_DIR"), "/swagger-ui/swagger-ui-bundle.js"));

/// Swagger UI Standalone Preset JS
pub const SWAGGER_UI_STANDALONE_PRESET_JS: &[u8] = include_bytes!(concat!(
    env!("OUT_DIR"),
    "/swagger-ui/swagger-ui-standalone-preset.js"
));

/// Swagger UI CSS
pub const SWAGGER_UI_CSS: &[u8] =
    include_bytes!(concat!(env!("OUT_DIR"), "/swagger-ui/swagger-ui.css"));

/// Favicon 32x32
pub const FAVICON_32X32: &[u8] =
    include_bytes!(concat!(env!("OUT_DIR"), "/swagger-ui/favicon-32x32.png"));

/// Favicon 16x16
pub const FAVICON_16X16: &[u8] =
    include_bytes!(concat!(env!("OUT_DIR"), "/swagger-ui/favicon-16x16.png"));

/// 根据相对路径获取嵌入的静态资源
///
/// 返回 `(content_type, bytes)` 或 `None`
pub fn get_embedded_asset(path: &str) -> Option<(&'static str, &'static [u8])> {
    match path {
        "swagger-ui-bundle.js" => Some((
            "application/javascript; charset=utf-8",
            SWAGGER_UI_BUNDLE_JS,
        )),
        "swagger-ui-standalone-preset.js" => Some((
            "application/javascript; charset=utf-8",
            SWAGGER_UI_STANDALONE_PRESET_JS,
        )),
        "swagger-ui.css" => Some(("text/css; charset=utf-8", SWAGGER_UI_CSS)),
        "favicon-32x32.png" => Some(("image/png", FAVICON_32X32)),
        "favicon-16x16.png" => Some(("image/png", FAVICON_16X16)),
        _ => None,
    }
}
