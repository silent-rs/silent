use crate::{Handler, MiddleWareHandler, Next, Request, Response, Result};
use async_trait::async_trait;
use http::header::{ACCEPT_ENCODING, CONTENT_ENCODING, CONTENT_LENGTH, CONTENT_TYPE, VARY};

use async_compression::futures::bufread::{BrotliEncoder, GzipEncoder};
use bytes::Bytes;
use futures::io::{AsyncRead, AsyncReadExt, BufReader};
use futures_util::stream::{self, BoxStream};
use futures_util::{StreamExt, TryStreamExt};

use crate::core::res_body::stream_body;

/// 压缩算法
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Algorithm {
    Brotli,
    Gzip,
}

/// Compression 中间件
///
/// 根据客户端 `Accept-Encoding` 头自动压缩响应体（gzip / brotli）。
///
/// # 行为
///
/// 1. 解析请求的 `Accept-Encoding` 头，协商压缩算法（优先 brotli > gzip）
/// 2. 调用下游 handler 获取响应
/// 3. 检查响应 `Content-Type` 是否为可压缩类型（text/*、application/json 等）
/// 4. 跳过已设置 `Content-Encoding` 的响应（避免二次压缩）
/// 5. 将响应体通过流式压缩编码器包装，设置 `Content-Encoding` 和 `Vary` 头
///
/// # 示例
///
/// ```rust
/// use silent::prelude::*;
/// use silent::middlewares::Compression;
///
/// let route = Route::new("/")
///     .hook(Compression::new())
///     .get(|_req: Request| async { Ok("hello") });
/// ```
///
/// 仅启用 gzip：
///
/// ```rust
/// use silent::prelude::*;
/// use silent::middlewares::Compression;
///
/// let route = Route::new("/")
///     .hook(Compression::gzip_only())
///     .get(|_req: Request| async { Ok("hello") });
/// ```
#[derive(Clone)]
pub struct Compression {
    enable_brotli: bool,
    enable_gzip: bool,
    /// 最小压缩阈值（字节），小于此大小的响应不压缩
    min_size: usize,
}

impl Default for Compression {
    fn default() -> Self {
        Self::new()
    }
}

impl Compression {
    /// 创建默认中间件，同时启用 brotli 和 gzip。
    pub fn new() -> Self {
        Self {
            enable_brotli: true,
            enable_gzip: true,
            min_size: 128,
        }
    }

    /// 仅启用 gzip 压缩。
    pub fn gzip_only() -> Self {
        Self {
            enable_brotli: false,
            enable_gzip: true,
            min_size: 128,
        }
    }

    /// 仅启用 brotli 压缩。
    pub fn brotli_only() -> Self {
        Self {
            enable_brotli: true,
            enable_gzip: false,
            min_size: 128,
        }
    }

    /// 设置最小压缩阈值（字节）。
    pub fn min_size(mut self, size: usize) -> Self {
        self.min_size = size;
        self
    }

    /// 根据 Accept-Encoding 协商压缩算法
    fn negotiate(&self, accept: &str) -> Option<Algorithm> {
        let mut brotli_ok = false;
        let mut gzip_ok = false;

        for item in accept.split(',') {
            let item = item.trim();
            let mut parts = item.split(';');
            let encoding = parts.next().map(str::trim).unwrap_or("");

            let mut quality = 1.0_f32;
            for param in parts {
                let mut kv = param.splitn(2, '=');
                if kv.next().map(str::trim) == Some("q")
                    && let Some(v) = kv.next()
                    && let Ok(parsed) = v.trim().parse::<f32>()
                {
                    quality = parsed;
                }
            }
            if quality == 0.0 {
                continue;
            }

            match encoding {
                "br" if self.enable_brotli => brotli_ok = true,
                "gzip" | "x-gzip" if self.enable_gzip => gzip_ok = true,
                "*" if self.enable_brotli => brotli_ok = true,
                "*" if self.enable_gzip => gzip_ok = true,
                _ => {}
            }
        }

        if brotli_ok {
            Some(Algorithm::Brotli)
        } else if gzip_ok {
            Some(Algorithm::Gzip)
        } else {
            None
        }
    }
}

/// 判断 Content-Type 是否适合压缩
fn is_compressible(content_type: &str) -> bool {
    let ct = content_type.to_ascii_lowercase();
    // 提取 MIME 主类型/子类型（忽略参数）
    let mime_part = ct.split(';').next().unwrap_or("").trim();

    if mime_part.starts_with("text/") {
        return true;
    }

    matches!(
        mime_part,
        "application/json"
            | "application/xml"
            | "application/javascript"
            | "application/ecmascript"
            | "application/x-javascript"
            | "application/xhtml+xml"
            | "application/rss+xml"
            | "application/svg+xml"
            | "image/svg+xml"
    )
}

/// 将 AsyncRead 转换为 BoxStream<Result<Bytes, std::io::Error>>
fn to_stream<R>(reader: R) -> BoxStream<'static, std::result::Result<Bytes, std::io::Error>>
where
    R: AsyncRead + Unpin + Send + 'static,
{
    const CHUNK_SIZE: usize = 16 * 1024;
    let buf = vec![0u8; CHUNK_SIZE];
    stream::try_unfold((reader, buf), |(mut reader, mut buf)| async move {
        let n = reader.read(&mut buf).await?;
        if n == 0 {
            Ok(None)
        } else {
            let bytes = Bytes::copy_from_slice(&buf[..n]);
            Ok(Some((bytes, (reader, buf))))
        }
    })
    .boxed()
}

#[async_trait]
impl MiddleWareHandler for Compression {
    async fn handle(&self, req: Request, next: &Next) -> Result<Response> {
        // 提取 Accept-Encoding 并协商算法
        let algorithm = req
            .headers()
            .get(ACCEPT_ENCODING)
            .and_then(|v| v.to_str().ok())
            .and_then(|accept| self.negotiate(accept));

        let mut res = next.call(req).await?;

        let algorithm = match algorithm {
            Some(a) => a,
            None => return Ok(res),
        };

        // 跳过已压缩的响应
        if res.headers().contains_key(CONTENT_ENCODING) {
            return Ok(res);
        }

        // 检查 Content-Type 是否可压缩
        let compressible = res
            .headers()
            .get(CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .map(is_compressible)
            .unwrap_or(false);

        if !compressible {
            return Ok(res);
        }

        // 检查最小大小阈值（仅对已知大小的响应体生效）
        if self.min_size > 0 {
            use http_body::Body;
            let hint = res.body.size_hint();
            if let Some(upper) = hint.upper() {
                if (upper as usize) < self.min_size {
                    return Ok(res);
                }
            }
        }

        // 取出响应体，转换为压缩流
        let body = res.take_body();
        let body_stream = body.map(|result| result.map_err(std::io::Error::other));
        let reader = body_stream.into_async_read();

        let compressed_stream = match algorithm {
            Algorithm::Brotli => {
                let encoder = BrotliEncoder::new(BufReader::new(reader));
                to_stream(encoder)
            }
            Algorithm::Gzip => {
                let encoder = GzipEncoder::new(BufReader::new(reader));
                to_stream(encoder)
            }
        };

        let encoding = match algorithm {
            Algorithm::Brotli => "br",
            Algorithm::Gzip => "gzip",
        };
        res.headers_mut()
            .insert(CONTENT_ENCODING, encoding.parse().unwrap());
        res.headers_mut()
            .insert(VARY, "Accept-Encoding".parse().unwrap());
        // 压缩后大小未知，移除 Content-Length
        res.headers_mut().remove(CONTENT_LENGTH);
        res.set_body(stream_body(compressed_stream));

        Ok(res)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::res_body::ResBody;

    // ==================== 构造函数测试 ====================

    #[test]
    fn test_compression_new() {
        let mid = Compression::new();
        assert!(mid.enable_brotli);
        assert!(mid.enable_gzip);
        assert_eq!(mid.min_size, 128);
    }

    #[test]
    fn test_compression_default() {
        let mid = Compression::default();
        assert!(mid.enable_brotli);
        assert!(mid.enable_gzip);
    }

    #[test]
    fn test_compression_gzip_only() {
        let mid = Compression::gzip_only();
        assert!(!mid.enable_brotli);
        assert!(mid.enable_gzip);
    }

    #[test]
    fn test_compression_brotli_only() {
        let mid = Compression::brotli_only();
        assert!(mid.enable_brotli);
        assert!(!mid.enable_gzip);
    }

    #[test]
    fn test_compression_min_size() {
        let mid = Compression::new().min_size(1024);
        assert_eq!(mid.min_size, 1024);
    }

    #[test]
    fn test_compression_clone() {
        let mid1 = Compression::new();
        let mid2 = mid1.clone();
        assert_eq!(mid1.enable_brotli, mid2.enable_brotli);
        assert_eq!(mid1.enable_gzip, mid2.enable_gzip);
        assert_eq!(mid1.min_size, mid2.min_size);
    }

    // ==================== negotiate 测试 ====================

    #[test]
    fn test_negotiate_brotli() {
        let mid = Compression::new();
        assert_eq!(mid.negotiate("br"), Some(Algorithm::Brotli));
    }

    #[test]
    fn test_negotiate_gzip() {
        let mid = Compression::new();
        assert_eq!(mid.negotiate("gzip"), Some(Algorithm::Gzip));
        assert_eq!(mid.negotiate("x-gzip"), Some(Algorithm::Gzip));
    }

    #[test]
    fn test_negotiate_prefers_brotli() {
        let mid = Compression::new();
        assert_eq!(mid.negotiate("gzip, br"), Some(Algorithm::Brotli));
        assert_eq!(mid.negotiate("br, gzip"), Some(Algorithm::Brotli));
    }

    #[test]
    fn test_negotiate_wildcard() {
        let mid = Compression::new();
        assert_eq!(mid.negotiate("*"), Some(Algorithm::Brotli));
    }

    #[test]
    fn test_negotiate_wildcard_gzip_only() {
        let mid = Compression::gzip_only();
        assert_eq!(mid.negotiate("*"), Some(Algorithm::Gzip));
    }

    #[test]
    fn test_negotiate_zero_quality() {
        let mid = Compression::new();
        assert_eq!(mid.negotiate("br;q=0, gzip"), Some(Algorithm::Gzip));
        assert_eq!(mid.negotiate("br;q=0, gzip;q=0"), None);
    }

    #[test]
    fn test_negotiate_no_match() {
        let mid = Compression::new();
        assert_eq!(mid.negotiate("identity"), None);
        assert_eq!(mid.negotiate("deflate"), None);
        assert_eq!(mid.negotiate(""), None);
    }

    #[test]
    fn test_negotiate_gzip_only_rejects_br() {
        let mid = Compression::gzip_only();
        assert_eq!(mid.negotiate("br"), None);
        assert_eq!(mid.negotiate("gzip"), Some(Algorithm::Gzip));
    }

    #[test]
    fn test_negotiate_brotli_only_rejects_gzip() {
        let mid = Compression::brotli_only();
        assert_eq!(mid.negotiate("gzip"), None);
        assert_eq!(mid.negotiate("br"), Some(Algorithm::Brotli));
    }

    // ==================== is_compressible 测试 ====================

    #[test]
    fn test_is_compressible_text() {
        assert!(is_compressible("text/plain"));
        assert!(is_compressible("text/html"));
        assert!(is_compressible("text/css"));
        assert!(is_compressible("text/javascript"));
        assert!(is_compressible("text/html; charset=utf-8"));
    }

    #[test]
    fn test_is_compressible_application() {
        assert!(is_compressible("application/json"));
        assert!(is_compressible("application/xml"));
        assert!(is_compressible("application/javascript"));
        assert!(is_compressible("application/xhtml+xml"));
        assert!(is_compressible("application/svg+xml"));
    }

    #[test]
    fn test_is_compressible_image_svg() {
        assert!(is_compressible("image/svg+xml"));
    }

    #[test]
    fn test_not_compressible() {
        assert!(!is_compressible("image/png"));
        assert!(!is_compressible("image/jpeg"));
        assert!(!is_compressible("video/mp4"));
        assert!(!is_compressible("application/octet-stream"));
        assert!(!is_compressible("audio/mpeg"));
    }

    // ==================== 集成测试 ====================

    #[cfg(feature = "server")]
    #[tokio::test]
    async fn test_compression_gzip_response() {
        use crate::route::Route;

        let mid = Compression::new().min_size(0);
        let route = Route::new("/").hook(mid).get(|_req: Request| async {
            let mut resp = Response::empty();
            resp.headers_mut()
                .insert(CONTENT_TYPE, "text/plain".parse().unwrap());
            resp.set_body(ResBody::Once(Bytes::from("hello world")));
            Ok(resp)
        });
        let route = Route::new_root().append(route);

        let mut req = Request::empty();
        req.headers_mut()
            .insert(ACCEPT_ENCODING, "gzip".parse().unwrap());

        let res: Result<Response> = crate::Handler::call(&route, req).await;
        assert!(res.is_ok());
        let resp = res.unwrap();
        assert_eq!(
            resp.headers()
                .get(CONTENT_ENCODING)
                .unwrap()
                .to_str()
                .unwrap(),
            "gzip"
        );
        assert_eq!(
            resp.headers().get(VARY).unwrap().to_str().unwrap(),
            "Accept-Encoding"
        );
        assert!(!resp.headers().contains_key(CONTENT_LENGTH));
    }

    #[cfg(feature = "server")]
    #[tokio::test]
    async fn test_compression_brotli_response() {
        use crate::route::Route;

        let mid = Compression::new().min_size(0);
        let route = Route::new("/").hook(mid).get(|_req: Request| async {
            let mut resp = Response::empty();
            resp.headers_mut()
                .insert(CONTENT_TYPE, "application/json".parse().unwrap());
            resp.set_body(ResBody::Once(Bytes::from("{\"key\":\"value\"}")));
            Ok(resp)
        });
        let route = Route::new_root().append(route);

        let mut req = Request::empty();
        req.headers_mut()
            .insert(ACCEPT_ENCODING, "br, gzip".parse().unwrap());

        let res: Result<Response> = crate::Handler::call(&route, req).await;
        assert!(res.is_ok());
        let resp = res.unwrap();
        assert_eq!(
            resp.headers()
                .get(CONTENT_ENCODING)
                .unwrap()
                .to_str()
                .unwrap(),
            "br"
        );
    }

    #[cfg(feature = "server")]
    #[tokio::test]
    async fn test_compression_skip_non_compressible() {
        use crate::route::Route;

        let mid = Compression::new().min_size(0);
        let route = Route::new("/").hook(mid).get(|_req: Request| async {
            let mut resp = Response::empty();
            resp.headers_mut()
                .insert(CONTENT_TYPE, "image/png".parse().unwrap());
            resp.set_body(ResBody::Once(Bytes::from(vec![0u8; 100])));
            Ok(resp)
        });
        let route = Route::new_root().append(route);

        let mut req = Request::empty();
        req.headers_mut()
            .insert(ACCEPT_ENCODING, "gzip".parse().unwrap());

        let res: Result<Response> = crate::Handler::call(&route, req).await;
        assert!(res.is_ok());
        let resp = res.unwrap();
        // 不应压缩图片
        assert!(resp.headers().get(CONTENT_ENCODING).is_none());
    }

    #[cfg(feature = "server")]
    #[tokio::test]
    async fn test_compression_skip_already_encoded() {
        use crate::route::Route;

        let mid = Compression::new().min_size(0);
        let route = Route::new("/").hook(mid).get(|_req: Request| async {
            let mut resp = Response::empty();
            resp.headers_mut()
                .insert(CONTENT_TYPE, "text/plain".parse().unwrap());
            resp.headers_mut()
                .insert(CONTENT_ENCODING, "br".parse().unwrap());
            resp.set_body(ResBody::Once(Bytes::from("already compressed")));
            Ok(resp)
        });
        let route = Route::new_root().append(route);

        let mut req = Request::empty();
        req.headers_mut()
            .insert(ACCEPT_ENCODING, "gzip".parse().unwrap());

        let res: Result<Response> = crate::Handler::call(&route, req).await;
        assert!(res.is_ok());
        let resp = res.unwrap();
        // 保持原有的 Content-Encoding
        assert_eq!(
            resp.headers()
                .get(CONTENT_ENCODING)
                .unwrap()
                .to_str()
                .unwrap(),
            "br"
        );
    }

    #[cfg(feature = "server")]
    #[tokio::test]
    async fn test_compression_skip_no_accept_encoding() {
        use crate::route::Route;

        let mid = Compression::new().min_size(0);
        let route = Route::new("/").hook(mid).get(|_req: Request| async {
            let mut resp = Response::empty();
            resp.headers_mut()
                .insert(CONTENT_TYPE, "text/plain".parse().unwrap());
            resp.set_body(ResBody::Once(Bytes::from("no compression")));
            Ok(resp)
        });
        let route = Route::new_root().append(route);

        let req = Request::empty();
        let res: Result<Response> = crate::Handler::call(&route, req).await;
        assert!(res.is_ok());
        let resp = res.unwrap();
        assert!(resp.headers().get(CONTENT_ENCODING).is_none());
    }

    #[cfg(feature = "server")]
    #[tokio::test]
    async fn test_compression_skip_small_body() {
        use crate::route::Route;

        // min_size 默认 128，10 字节应跳过
        let mid = Compression::new();
        let route = Route::new("/").hook(mid).get(|_req: Request| async {
            let mut resp = Response::empty();
            resp.headers_mut()
                .insert(CONTENT_TYPE, "text/plain".parse().unwrap());
            resp.set_body(ResBody::Once(Bytes::from("small")));
            Ok(resp)
        });
        let route = Route::new_root().append(route);

        let mut req = Request::empty();
        req.headers_mut()
            .insert(ACCEPT_ENCODING, "gzip".parse().unwrap());

        let res: Result<Response> = crate::Handler::call(&route, req).await;
        assert!(res.is_ok());
        let resp = res.unwrap();
        // 小于 min_size，不压缩
        assert!(resp.headers().get(CONTENT_ENCODING).is_none());
    }
}
