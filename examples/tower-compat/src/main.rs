use std::sync::Arc;

use silent::prelude::*;
use tower_http::add_extension::AddExtensionLayer;
use tower_http::set_header::SetResponseHeaderLayer;

/// 共享应用状态（通过 Tower AddExtensionLayer 注入）
#[derive(Clone)]
struct AppInfo {
    name: &'static str,
    version: &'static str,
}

fn main() {
    logger::fmt().with_max_level(Level::INFO).init();

    let app_info = Arc::new(AppInfo {
        name: "Silent",
        version: "2.16",
    });

    let route = Route::new("")
        // tower-http: 为所有响应添加 X-Powered-By header
        .hook_layer(SetResponseHeaderLayer::overriding(
            http::header::HeaderName::from_static("x-powered-by"),
            http::HeaderValue::from_static("Silent/Tower"),
        ))
        // tower-http: 为所有响应添加 X-Request-Id header
        .hook_layer(SetResponseHeaderLayer::if_not_present(
            http::header::HeaderName::from_static("x-request-id"),
            http::HeaderValue::from_static("default-id"),
        ))
        // tower-http: 注入共享状态到请求 Extensions
        .hook_layer(AddExtensionLayer::new(app_info))
        .get(|req: Request| async move {
            // 从 Tower 注入的 Extensions 中获取 AppInfo
            let info = req.extensions().get::<Arc<AppInfo>>().unwrap();
            Ok(format!("{} v{}", info.name, info.version))
        })
        .append(Route::new("hello/<name>").get(|req: Request| async move {
            let name: String = req.get_path_params("name")?;
            let info = req.extensions().get::<Arc<AppInfo>>().unwrap();
            Ok(format!(
                "Hello {} from {} v{}!",
                name, info.name, info.version
            ))
        }));

    Server::new().run(route);
}
