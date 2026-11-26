//! 前端静态资源：单个二进制里的前端文件，加 SPA 兜底。
//!
//! 应用用 rust-embed 把 `web/dist` 编进二进制（release 从二进制里读，debug 从磁盘读，
//! 所以前端重新构建后不用重新编译后端），再把那个类型交给 [`spa_route`]：
//!
//! ```ignore
//! #[derive(RustEmbed)]
//! #[folder = "web/dist/"]
//! struct WebAssets;
//!
//! let route = silent_boot::web::assets::spa_route(Arc::new(WebAssets), "index.html", None);
//! ```
//!
//! 请求的路径命中文件就返回文件；没命中但看起来不是文件（最后一层没有扩展名）就返回 `index.html`，
//! 交给前端路由；看起来是文件却不存在（例如 `/assets/app-9f3a.js`）返回 404。
//! 文件带 ETag，`assets/` 下的文件按内容带了哈希，可以长期缓存。

use std::borrow::Cow;
use std::collections::BTreeMap;
use std::hash::{Hash, Hasher};
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use silent::headers::ContentType;
use silent::prelude::*;
use silent::{Method, StatusCode};

/// 资源集合。任何 rust-embed 的类型都自动实现它。
pub trait AssetSource: Send + Sync + 'static {
    /// 读取一个文件。`path` 是相对于资源根目录的路径，不带开头的 `/`。
    fn get(&self, path: &str) -> Option<Asset>;
}

/// 一个资源文件。
#[derive(Debug, Clone)]
pub struct Asset {
    /// 文件内容。
    pub data: Cow<'static, [u8]>,
    /// 最后修改时间，有就带上。
    pub last_modified: Option<SystemTime>,
}

impl<T: rust_embed::RustEmbed + Send + Sync + 'static> AssetSource for T {
    fn get(&self, path: &str) -> Option<Asset> {
        let file = T::get(path)?;
        Some(Asset {
            data: file.data,
            last_modified: file
                .metadata
                .last_modified()
                .and_then(|secs| SystemTime::UNIX_EPOCH.checked_add(Duration::from_secs(secs))),
        })
    }
}

/// 内存里的资源集合，测试和没有前端的应用用。
#[derive(Debug, Clone, Default)]
pub struct MemoryAssets(pub BTreeMap<String, Vec<u8>>);

impl MemoryAssets {
    /// 空集合。
    pub fn new() -> Self {
        Self::default()
    }
}

impl AssetSource for MemoryAssets {
    fn get(&self, path: &str) -> Option<Asset> {
        self.0.get(path).map(|data| Asset {
            data: Cow::Owned(data.clone()),
            last_modified: None,
        })
    }
}

/// 生成兜底路由，挂在根路径的最后。
///
/// - `index` 是前端入口文件；
/// - `dev_script` 是开发时注入 HTML 的脚本地址，没有就传 `None`；
/// - `reserved` 是不归前端管的前缀（不带开头的 `/`，例如 `api`、`swagger`）：
///   这些路径下没找到文件就返回 404，不回退到 `index.html`。
pub fn spa_route(
    source: Arc<dyn AssetSource>,
    index: impl Into<String>,
    dev_script: Option<String>,
    reserved: &[&str],
) -> Route {
    Route::new("<path:**>").insert_handler(
        Method::GET,
        Arc::new(ServeAssets {
            source,
            index: index.into(),
            dev_script,
            reserved: reserved
                .iter()
                .map(|p| format!("{}/", p.trim_matches('/')))
                .collect(),
        }),
    )
}

#[derive(Clone)]
struct ServeAssets {
    source: Arc<dyn AssetSource>,
    index: String,
    dev_script: Option<String>,
    reserved: Vec<String>,
}

#[async_trait::async_trait]
impl Handler for ServeAssets {
    async fn call(&self, req: Request) -> Result<Response> {
        let requested = req.get_path_params::<String>("path").unwrap_or_default();
        let path = percent_decode(&requested);
        if path.contains("..") || path.contains('\\') || path.contains('\0') {
            return Ok(not_found());
        }
        let path = path.trim_start_matches('/');
        let (path, asset) = match self.source.get(path) {
            Some(asset) => (path.to_owned(), asset),
            None if looks_like_file(path) || self.is_reserved(path) => {
                return Ok(not_found());
            }
            None => {
                let index = self.index.trim_start_matches('/').to_owned();
                match self.source.get(&index) {
                    Some(asset) => (index, asset),
                    None => return Ok(not_found()),
                }
            }
        };
        Ok(self.serve(&path, asset, &req))
    }
}

impl ServeAssets {
    fn is_reserved(&self, path: &str) -> bool {
        self.reserved.iter().any(|prefix| path.starts_with(prefix))
    }

    fn serve(&self, path: &str, asset: Asset, req: &Request) -> Response {
        let mut hash = std::collections::hash_map::DefaultHasher::new();
        asset.data.hash(&mut hash);
        let etag = format!("\"{:016x}\"", hash.finish());
        let unchanged = req
            .headers()
            .get(header::IF_NONE_MATCH)
            .and_then(|value| value.to_str().ok())
            .is_some_and(|value| value.split(',').any(|candidate| candidate.trim() == etag));
        if unchanged {
            return Response::empty()
                .with_status(StatusCode::NOT_MODIFIED)
                .with_header(header::ETAG, etag.parse().unwrap());
        }

        let mime = mime_guess::from_path(path).first_or_octet_stream();
        // `assets/` 下的文件名带内容哈希，可以长期缓存；其余文件每次都带上 ETag 重新确认。
        let immutable = path.starts_with("assets/");
        let injected = (mime.essence_str() == "text/html")
            .then(|| {
                self.dev_script
                    .as_ref()
                    .map(|script| inject_script(&asset.data, script))
            })
            .flatten();
        let data: &[u8] = injected.as_deref().unwrap_or(&asset.data);

        let mut res = Response::empty().with_header(header::ETAG, etag.parse().unwrap());
        res.set_typed_header(ContentType::from(mime));
        let cache_control = if immutable {
            "public, max-age=31536000, immutable"
        } else {
            "public, max-age=0, no-cache"
        };
        res.set_header(header::CACHE_CONTROL, cache_control.parse().unwrap());
        if let Some(modified) = asset.last_modified {
            res.set_typed_header(silent::headers::LastModified::from(modified));
        }
        res.set_body(full(data.to_vec()));
        res
    }
}

fn not_found() -> Response {
    Response::empty().with_status(StatusCode::NOT_FOUND)
}

/// 最后一层带扩展名的路径当成文件，例如 `assets/app-9f3a.js`、`favicon.ico`。
fn looks_like_file(path: &str) -> bool {
    path.rsplit('/')
        .next()
        .is_some_and(|last| matches!(last.rfind('.'), Some(at) if at > 0 && at < last.len() - 1))
}

/// 把 `<script src="…" async></script>` 插到 `</body>` 前面；没有 `</body>` 就接在末尾。
fn inject_script(html: &[u8], src: &str) -> Vec<u8> {
    const TAG: &str = "<script src=\"{}\" async></script>";
    let tag = TAG.replace("{}", src);
    let lower = String::from_utf8_lossy(html).to_ascii_lowercase();
    match lower.rfind("</body>") {
        Some(at) => {
            // `</body>` 里的字符都是 ASCII，小写化不改变字节位置。
            let mut out = html[..at].to_vec();
            out.extend_from_slice(tag.as_bytes());
            out.extend_from_slice(&html[at..]);
            out
        }
        None => {
            let mut out = html.to_vec();
            out.extend_from_slice(tag.as_bytes());
            out
        }
    }
}

/// 解码 `%XX`。解码不了的片段原样保留。
fn percent_decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' && index + 2 < bytes.len() {
            let decoded = std::str::from_utf8(&bytes[index + 1..index + 3])
                .ok()
                .and_then(|hex| u8::from_str_radix(hex, 16).ok());
            if let Some(byte) = decoded {
                out.push(byte);
                index += 3;
                continue;
            }
        }
        out.push(bytes[index]);
        index += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use silent::testing::TestClient;

    fn assets() -> Arc<dyn AssetSource> {
        Arc::new(MemoryAssets(
            [
                (
                    "index.html".to_owned(),
                    b"<html><body>app</body></html>".to_vec(),
                ),
                ("assets/app-9f3a.js".to_owned(), b"console.log(1)".to_vec()),
                ("favicon.ico".to_owned(), vec![0, 1, 2]),
            ]
            .into_iter()
            .collect(),
        ))
    }

    fn route(dev_script: Option<&str>) -> Route {
        Route::new_root()
            .append(Route::new("api/ping").get(|_req: Request| async { Ok("pong") }))
            .append(spa_route(
                assets(),
                "index.html",
                dev_script.map(str::to_owned),
                &["api"],
            ))
    }

    fn header_value(res: &silent::testing::TestResponse, name: &str) -> String {
        res.headers()
            .get(name)
            .and_then(|value| value.to_str().ok())
            .unwrap_or_default()
            .to_owned()
    }

    #[tokio::test]
    async fn files_are_served_with_types_and_cache_rules() {
        let route = route(None);
        let res = TestClient::get("/assets/app-9f3a.js").send(&route).await;
        assert_eq!(res.status(), StatusCode::OK);
        assert_eq!(header_value(&res, "content-type"), "text/javascript");
        let cache = header_value(&res, "cache-control");
        assert!(cache.contains("max-age=31536000"), "{cache}");
        assert!(cache.contains("immutable"), "{cache}");
        assert!(cache.contains("public"), "{cache}");

        let res = TestClient::get("/index.html").send(&route).await;
        let cache = header_value(&res, "cache-control");
        assert!(cache.contains("max-age=0"), "{cache}");
        assert!(cache.contains("no-cache"), "{cache}");
        assert!(!cache.contains("immutable"), "{cache}");
    }

    #[tokio::test]
    async fn client_routes_fall_back_to_index_and_missing_files_stay_404() {
        let route = route(None);
        let res = TestClient::get("/articles/123").send(&route).await;
        assert_eq!(res.status(), StatusCode::OK);
        assert_eq!(header_value(&res, "content-type"), "text/html");
        assert!(res.text().await.contains("app"));

        let res = TestClient::get("/assets/missing.js").send(&route).await;
        assert_eq!(res.status(), StatusCode::NOT_FOUND);
        let res = TestClient::get("/favicon.ico").send(&route).await;
        assert_eq!(res.status(), StatusCode::OK);
        assert_eq!(res.bytes().await.len(), 3);

        let res = TestClient::get("/..%2Fetc%2Fpasswd").send(&route).await;
        assert_eq!(res.status(), StatusCode::NOT_FOUND);

        // 已经注册的接口照常；接口前缀下没注册的路径不回退到 index.html。
        let res = TestClient::get("/api/ping").send(&route).await;
        assert_eq!(res.status(), StatusCode::OK);
        assert_eq!(res.text().await, "pong");
        let res = TestClient::get("/api/nope").send(&route).await;
        assert_eq!(res.status(), StatusCode::NOT_FOUND);
        let res = TestClient::get("/articles").send(&route).await;
        assert_eq!(res.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn etag_revalidation_returns_304() {
        let route = route(None);
        let res = TestClient::get("/assets/app-9f3a.js").send(&route).await;
        let etag = header_value(&res, "etag");
        assert!(etag.starts_with('"'), "{etag}");
        let res = TestClient::get("/assets/app-9f3a.js")
            .header(header::IF_NONE_MATCH, etag)
            .send(&route)
            .await;
        assert_eq!(res.status(), StatusCode::NOT_MODIFIED);
        assert!(res.bytes().await.is_empty());
    }

    #[tokio::test]
    async fn dev_script_is_injected_into_html_only() {
        let route = route(Some("http://127.0.0.1:9230/__dev__/reload.js"));
        let res = TestClient::get("/").send(&route).await;
        assert_eq!(
            res.text().await,
            "<html><body>app<script src=\"http://127.0.0.1:9230/__dev__/reload.js\" async></script></body></html>"
        );
        let res = TestClient::get("/assets/app-9f3a.js").send(&route).await;
        assert_eq!(res.text().await, "console.log(1)");
    }

    #[test]
    fn helper_rules() {
        assert!(looks_like_file("assets/app.js"));
        assert!(looks_like_file("favicon.ico"));
        assert!(!looks_like_file("articles/123"));
        assert!(!looks_like_file("articles/1.5/details"));
        assert_eq!(percent_decode("/a%20b/%E4%B8%AD.js"), "/a b/中.js");
        assert_eq!(percent_decode("/a%zz"), "/a%zz");
        assert_eq!(percent_decode("/100%"), "/100%");
        assert_eq!(percent_decode("/%E4%B8"), "/\u{fffd}");
        assert_eq!(
            inject_script(b"<HTML><BODY>x", "s.js"),
            b"<HTML><BODY>x<script src=\"s.js\" async></script>".to_vec()
        );
        assert_eq!(
            inject_script("<html><body>中文</BODY></html>".as_bytes(), "s.js"),
            "<html><body>中文<script src=\"s.js\" async></script></BODY></html>".as_bytes()
        );
    }
}
