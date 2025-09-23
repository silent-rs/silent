use super::{HttpTransport, TransportFuture};
use crate::core::connection::Connection;
use crate::core::socket_addr::SocketAddr;
use crate::handler::Handler;
use crate::{Request, Response};
use bytes::Bytes;
use futures::io::{AsyncReadExt, AsyncWriteExt};
use http::{HeaderMap, HeaderName, HeaderValue, Method, Version};
use http_body_util::BodyExt;
use std::error::Error as StdError;

/// 占位的 Async-IO HTTP 传输实现。
///
/// 注意：当前仅作为占位，返回未实现错误。
/// 后续将以 async-io/async-net/async-h1 实现真正的 HTTP 编解码与服务流程。
#[allow(dead_code)]
pub struct AsyncIoTransport;

#[allow(dead_code)]
impl AsyncIoTransport {
    pub fn new() -> Self {
        Self
    }
}

impl Default for AsyncIoTransport {
    fn default() -> Self {
        Self::new()
    }
}

impl HttpTransport for AsyncIoTransport {
    fn serve<'a>(
        &'a self,
        mut stream: Box<dyn Connection + Send>,
        peer_addr: SocketAddr,
        routes: std::sync::Arc<dyn Handler>,
    ) -> TransportFuture<'a> {
        Box::pin(async move {
            let mut buf = Vec::with_capacity(4096);
            let mut tmp = [0u8; 1024];
            loop {
                let n = stream.read(&mut tmp).await?;
                if n == 0 {
                    break;
                }
                buf.extend_from_slice(&tmp[..n]);
                if let Some(pos) = find_crlf_crlf(&buf) {
                    let header_end = pos + 4;
                    let (head, rest) = buf.split_at(header_end);
                    let (method, path, version, headers) = parse_head(head)?;
                    let content_len = headers
                        .get("content-length")
                        .and_then(|v| v.to_str().ok())
                        .and_then(|s| s.parse::<usize>().ok())
                        .unwrap_or(0);
                    let mut body = rest.to_vec();
                    while body.len() < content_len {
                        let n = stream.read(&mut tmp).await?;
                        if n == 0 {
                            break;
                        }
                        body.extend_from_slice(&tmp[..n]);
                    }

                    let mut req = build_request(method, path, version, headers, body, peer_addr)?;
                    #[cfg(feature = "upgrade")]
                    let (tx, rx) =
                        futures::channel::oneshot::channel::<Box<dyn Connection + Send>>();
                    #[cfg(feature = "upgrade")]
                    {
                        // 注入升级接收器，供 ws::upgrade::on 使用
                        req.extensions_mut()
                            .insert(crate::ws::AsyncUpgradeRx::new(rx));
                    }

                    let res = routes.clone().call(req).await.unwrap_or_else(Into::into);
                    #[cfg(feature = "upgrade")]
                    {
                        let status = write_response(stream.as_mut(), res).await?;
                        if status == http::StatusCode::SWITCHING_PROTOCOLS {
                            // 将底层流交由 WS 处理
                            let _ = tx.send(stream);
                        }
                    }
                    #[cfg(not(feature = "upgrade"))]
                    {
                        let _ = write_response(stream.as_mut(), res).await?;
                    }
                    break;
                }
                if buf.len() > 64 * 1024 {
                    return Err("request header too large".into());
                }
            }
            Ok(())
        })
    }

    fn requires_tokio(&self) -> bool {
        false
    }
}

#[allow(clippy::manual_find)]
fn find_crlf_crlf(buf: &[u8]) -> Option<usize> {
    if buf.len() < 4 {
        return None;
    }
    (0..=buf.len() - 4).find(|&i| &buf[i..i + 4] == b"\r\n\r\n")
}

fn parse_head(
    head: &[u8],
) -> Result<(Method, String, Version, HeaderMap), Box<dyn StdError + Send + Sync>> {
    let s = std::str::from_utf8(head)?;
    let mut lines = s.split("\r\n");
    let start = lines.next().ok_or("missing request line")?;
    let mut parts = start.split_whitespace();
    let method_str = parts.next().ok_or("missing method")?;
    let method = Method::from_bytes(method_str.as_bytes())?;
    let path = parts.next().ok_or("missing path")?.to_string();
    let ver = parts.next().unwrap_or("HTTP/1.1");
    let version = if ver.contains("1.0") {
        Version::HTTP_10
    } else {
        Version::HTTP_11
    };
    let mut headers = HeaderMap::new();
    for line in lines {
        if line.is_empty() {
            break;
        }
        if let Some((k, v)) = line.split_once(':') {
            let name = HeaderName::from_bytes(k.trim().as_bytes())?;
            let val = HeaderValue::from_str(v.trim())?;
            headers.append(name, val);
        }
    }
    Ok((method, path, version, headers))
}

fn build_request(
    method: Method,
    path: String,
    version: Version,
    headers: HeaderMap,
    body: Vec<u8>,
    peer_addr: SocketAddr,
) -> Result<Request, Box<dyn StdError + Send + Sync>> {
    let mut builder = http::Request::builder()
        .method(method)
        .uri(&path)
        .version(version);
    if let Some(hm) = builder.headers_mut() {
        *hm = headers;
    }
    let req_http = builder.body(())?;
    let (parts, _) = req_http.into_parts();
    let mut req = Request::from_parts(
        parts,
        crate::core::req_body::ReqBody::Once(Bytes::from(body)),
    );
    req.set_remote(peer_addr);
    Ok(req)
}

async fn write_response(
    stream: &mut (dyn Connection + Send),
    res: Response,
) -> Result<http::StatusCode, Box<dyn StdError + Send + Sync>> {
    let hyper_res: hyper::Response<crate::core::res_body::ResBody> =
        crate::core::adapt::ResponseAdapt::tran_from_response(res);
    let (parts, body) = hyper_res.into_parts();
    let collected = body.collect().await?;
    let body_bytes = collected.to_bytes();
    let ver = match parts.version {
        Version::HTTP_10 => "HTTP/1.0",
        _ => "HTTP/1.1",
    };
    let status = parts.status.as_u16();
    let reason = parts.status.canonical_reason().unwrap_or("");
    let mut head = format!("{ver} {status} {reason}\r\n");
    let mut headers = parts.headers;
    if parts.status != http::StatusCode::SWITCHING_PROTOCOLS {
        if !headers.contains_key("content-length") {
            headers.insert(
                http::header::CONTENT_LENGTH,
                http::HeaderValue::from_str(&body_bytes.len().to_string())?,
            );
        }
        headers.insert(
            http::header::CONNECTION,
            http::HeaderValue::from_static("close"),
        );
    }
    for (k, v) in headers.iter() {
        head.push_str(k.as_str());
        head.push_str(": ");
        head.push_str(v.to_str().unwrap_or(""));
        head.push_str("\r\n");
    }
    head.push_str("\r\n");
    stream.write_all(head.as_bytes()).await?;
    if parts.status != http::StatusCode::SWITCHING_PROTOCOLS {
        stream.write_all(&body_bytes).await?;
    }
    stream.flush().await?;
    Ok(parts.status)
}
