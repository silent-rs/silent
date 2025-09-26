use super::{HttpTransport, TransportFuture};
use crate::core::connection::Connection;
use crate::core::socket_addr::SocketAddr;
use crate::handler::Handler;
use crate::{Request, Response};
use bytes::Bytes;
use futures::StreamExt;
use futures::io::{AsyncReadExt, AsyncWriteExt};
use futures_timer::Delay;
use http::{
    HeaderMap, Method, Uri, Version,
    header::{self, HeaderName, HeaderValue},
};
use http_body::Body;
use httparse;
use std::error::Error as StdError;

const MAX_HEADERS: usize = 32;
const MAX_HEADER_BYTES: usize = 64 * 1024;
const READ_BUF_SIZE: usize = 2048;
const DEFAULT_MAX_PIPELINED: usize = 32;
const DEFAULT_KEEP_ALIVE_TIMEOUT_SECS: u64 = 15;

/// 基于 async-io 的最小 HTTP/1.1 传输实现。
///
/// - 支持 Content-Length 与 chunked 请求体。
/// - 支持 HTTP/1.1 keep-alive 与请求流水线。
/// - 响应统一转回 Hyper Response 结构以复用现有适配逻辑。
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
            let mut buf = Vec::with_capacity(READ_BUF_SIZE * 2);
            let mut tmp = [0u8; READ_BUF_SIZE];

            let mut served_requests = 0usize;
            let mut deadline = std::time::Instant::now()
                + std::time::Duration::from_secs(DEFAULT_KEEP_ALIVE_TIMEOUT_SECS);

            loop {
                if served_requests >= DEFAULT_MAX_PIPELINED {
                    break;
                }

                let mut parsed = loop {
                    if let Some(header_len) = find_crlf_crlf(&buf) {
                        if header_len > MAX_HEADER_BYTES {
                            return Err("request header too large".into());
                        }
                        let head = &buf[..header_len];
                        let parsed = parse_head(head)?;
                        buf.drain(..header_len);
                        break parsed;
                    }

                    if buf.len() > MAX_HEADER_BYTES {
                        return Err("request header too large".into());
                    }

                    let now = std::time::Instant::now();
                    if now >= deadline {
                        return Ok(());
                    }

                    let timeout = deadline - now;
                    let read_future = stream.read(&mut tmp);
                    let timeout_future = Delay::new(timeout);
                    futures::pin_mut!(read_future);
                    futures::pin_mut!(timeout_future);
                    match futures::future::select(read_future, timeout_future).await {
                        futures::future::Either::Left((Ok(n), _)) => {
                            if n == 0 {
                                if buf.is_empty() {
                                    return Ok(());
                                } else {
                                    return Err("connection closed while reading header".into());
                                }
                            }
                            buf.extend_from_slice(&tmp[..n]);
                        }
                        futures::future::Either::Left((Err(e), _)) => return Err(e.into()),
                        futures::future::Either::Right(_) => return Ok(()),
                    }
                };

                let (body, trailers) = if parsed.transfer_encoding_chunked {
                    read_chunked_body(stream.as_mut(), &mut buf).await?
                } else {
                    while buf.len() < parsed.content_length {
                        let n = stream.read(&mut tmp).await?;
                        if n == 0 {
                            return Err("unexpected eof while reading body".into());
                        }
                        buf.extend_from_slice(&tmp[..n]);
                    }
                    let body = buf.drain(..parsed.content_length).collect::<Vec<_>>();
                    (body, None)
                };

                if let Some(trailers) = trailers {
                    for (name, value) in trailers.iter() {
                        parsed.headers.append(name.clone(), value.clone());
                    }
                }

                #[cfg_attr(not(feature = "upgrade"), allow(unused_mut))]
                let mut req = build_request(&parsed, body, peer_addr.clone())?;

                #[cfg(feature = "upgrade")]
                let (tx, rx) = futures::channel::oneshot::channel::<Box<dyn Connection + Send>>();
                #[cfg(feature = "upgrade")]
                {
                    req.extensions_mut()
                        .insert(crate::ws::AsyncUpgradeRx::new(rx));
                }

                let res = routes.clone().call(req).await.unwrap_or_else(Into::into);
                let mut keep_alive = request_wants_keep_alive(&parsed);
                if let Some(conn_header) = res.headers().get(header::CONNECTION) {
                    if header_value_contains(Some(conn_header), "close") {
                        keep_alive = false;
                    } else if header_value_contains(Some(conn_header), "keep-alive") {
                        keep_alive = true;
                    }
                }

                let status = write_response(stream.as_mut(), res, keep_alive).await?;

                #[cfg(feature = "upgrade")]
                if status == http::StatusCode::SWITCHING_PROTOCOLS {
                    let _ = tx.send(stream);
                    return Ok(());
                }

                #[cfg(not(feature = "upgrade"))]
                if status == http::StatusCode::SWITCHING_PROTOCOLS {
                    return Ok(());
                }

                served_requests += 1;

                if !keep_alive {
                    break;
                }

                deadline = std::time::Instant::now()
                    + std::time::Duration::from_secs(DEFAULT_KEEP_ALIVE_TIMEOUT_SECS);
            }

            Ok(())
        })
    }

    fn requires_tokio(&self) -> bool {
        false
    }
}

fn find_crlf_crlf(buf: &[u8]) -> Option<usize> {
    if buf.len() < 4 {
        return None;
    }
    for i in 0..=buf.len() - 4 {
        if &buf[i..i + 4] == b"\r\n\r\n" {
            return Some(i + 4);
        }
    }
    None
}

#[derive(Clone)]
struct ParsedHead {
    method: Method,
    uri: Uri,
    version: Version,
    headers: HeaderMap,
    content_length: usize,
    transfer_encoding_chunked: bool,
}

fn parse_head(head: &[u8]) -> Result<ParsedHead, Box<dyn StdError + Send + Sync>> {
    let mut headers_storage = [httparse::EMPTY_HEADER; MAX_HEADERS];
    let mut req = httparse::Request::new(&mut headers_storage);
    match req.parse(head)? {
        httparse::Status::Partial => {
            return Err("incomplete http request head".into());
        }
        httparse::Status::Complete(_) => {}
    }

    let method = req
        .method
        .ok_or_else(|| "missing method".to_string())?
        .parse::<Method>()?;
    let path = req
        .path
        .ok_or_else(|| "missing uri".to_string())?
        .parse::<Uri>()?;
    let version = match req.version {
        Some(0) => Version::HTTP_10,
        Some(1) => Version::HTTP_11,
        Some(2) => Version::HTTP_2,
        _ => Version::HTTP_11,
    };

    let mut headers = HeaderMap::with_capacity(req.headers.len());
    let mut content_length: Option<usize> = None;
    let mut transfer_encoding_chunked = false;
    for header in req.headers.iter() {
        let name = HeaderName::from_bytes(header.name.as_bytes())?;
        let value = HeaderValue::from_bytes(header.value)?;
        if name == header::CONTENT_LENGTH && content_length.is_none() {
            content_length = Some(
                std::str::from_utf8(header.value)?
                    .trim()
                    .parse::<usize>()
                    .map_err(|_| "invalid content-length")?,
            );
        }
        if name == header::TRANSFER_ENCODING {
            let val = std::str::from_utf8(header.value)?.to_ascii_lowercase();
            if val.split(',').any(|token| token.trim() == "chunked") {
                transfer_encoding_chunked = true;
            }
        }
        headers.append(name, value);
    }

    Ok(ParsedHead {
        method,
        uri: path,
        version,
        headers,
        content_length: content_length.unwrap_or(0),
        transfer_encoding_chunked,
    })
}

fn build_request(
    parsed: &ParsedHead,
    body: Vec<u8>,
    peer_addr: SocketAddr,
) -> Result<Request, Box<dyn StdError + Send + Sync>> {
    let mut builder = http::Request::builder()
        .method(parsed.method.clone())
        .uri(parsed.uri.clone())
        .version(parsed.version);
    if let Some(hm) = builder.headers_mut() {
        *hm = parsed.headers.clone();
    }
    let req_http = builder.body(())?;
    let (parts, _) = req_http.into_parts();
    let req_body = if body.is_empty() {
        crate::core::req_body::ReqBody::Empty
    } else {
        crate::core::req_body::ReqBody::Once(Bytes::from(body))
    };
    let mut req = Request::from_parts(parts, req_body);
    req.set_remote(peer_addr);
    Ok(req)
}

async fn read_chunked_body(
    stream: &mut (dyn Connection + Send),
    buffer: &mut Vec<u8>,
) -> Result<(Vec<u8>, Option<HeaderMap>), Box<dyn StdError + Send + Sync>> {
    let mut decoded = Vec::new();
    let mut trailers = HeaderMap::new();
    let mut tmp = [0u8; READ_BUF_SIZE];

    loop {
        let size_line_pos = loop {
            if let Some(pos) = find_crlf(buffer) {
                break pos;
            }
            let n = stream.read(&mut tmp).await?;
            if n == 0 {
                return Err("unexpected eof while reading chunk size".into());
            }
            buffer.extend_from_slice(&tmp[..n]);
        };

        let size_line = buffer[..size_line_pos].to_vec();
        buffer.drain(..size_line_pos + 2);
        let size = parse_chunk_size(&size_line)?;

        if size == 0 {
            loop {
                if let Some(pos) = find_crlf(buffer) {
                    let line = buffer[..pos].to_vec();
                    buffer.drain(..pos + 2);
                    if line.is_empty() {
                        break;
                    }
                    let line_str = std::str::from_utf8(&line)?.trim();
                    if let Some((name, value)) = line_str.split_once(':') {
                        let header_name = HeaderName::from_bytes(name.trim().as_bytes())?;
                        let header_value = HeaderValue::from_str(value.trim())?;
                        trailers.append(header_name, header_value);
                    } else {
                        return Err("invalid trailer header".into());
                    }
                } else {
                    let n = stream.read(&mut tmp).await?;
                    if n == 0 {
                        break;
                    }
                    buffer.extend_from_slice(&tmp[..n]);
                }
            }
            break;
        }

        while buffer.len() < size + 2 {
            let n = stream.read(&mut tmp).await?;
            if n == 0 {
                return Err("unexpected eof while reading chunk body".into());
            }
            buffer.extend_from_slice(&tmp[..n]);
        }

        decoded.extend_from_slice(&buffer[..size]);
        buffer.drain(..size + 2);
    }

    let trailers = if trailers.is_empty() {
        None
    } else {
        Some(trailers)
    };

    Ok((decoded, trailers))
}

fn parse_chunk_size(line: &[u8]) -> Result<usize, Box<dyn StdError + Send + Sync>> {
    let raw = std::str::from_utf8(line)?.trim();
    let size_str = raw.split(';').next().unwrap_or("");
    usize::from_str_radix(size_str.trim(), 16).map_err(|_| "invalid chunk size".into())
}

fn find_crlf(buf: &[u8]) -> Option<usize> {
    buf.windows(2).position(|w| w == b"\r\n")
}

fn header_value_contains(value: Option<&HeaderValue>, needle: &str) -> bool {
    value
        .and_then(|v| v.to_str().ok())
        .map(|s| {
            s.split(',')
                .any(|token| token.trim().eq_ignore_ascii_case(needle))
        })
        .unwrap_or(false)
}

fn request_wants_keep_alive(parsed: &ParsedHead) -> bool {
    let conn = parsed.headers.get(header::CONNECTION);
    match parsed.version {
        Version::HTTP_11 => !header_value_contains(conn, "close"),
        Version::HTTP_10 => header_value_contains(conn, "keep-alive"),
        _ => !header_value_contains(conn, "close"),
    }
}

async fn write_response(
    stream: &mut (dyn Connection + Send),
    res: Response,
    mut keep_alive: bool,
) -> Result<http::StatusCode, Box<dyn StdError + Send + Sync>> {
    let hyper_res: hyper::Response<crate::core::res_body::ResBody> =
        crate::core::adapt::ResponseAdapt::tran_from_response(res);
    let (mut parts, mut body) = hyper_res.into_parts();

    if header_value_contains(parts.headers.get(header::CONNECTION), "close") {
        keep_alive = false;
    }

    let mut use_chunked =
        header_value_contains(parts.headers.get(header::TRANSFER_ENCODING), "chunked");

    if parts.status != http::StatusCode::SWITCHING_PROTOCOLS {
        if !parts.headers.contains_key(header::CONTENT_LENGTH) && !use_chunked {
            let hint = body.size_hint();
            if let Some(exact) = hint.exact() {
                parts.headers.insert(
                    header::CONTENT_LENGTH,
                    header::HeaderValue::from_str(&exact.to_string())?,
                );
            } else {
                use_chunked = true;
                parts.headers.remove(header::CONTENT_LENGTH);
                parts.headers.insert(
                    header::TRANSFER_ENCODING,
                    header::HeaderValue::from_static("chunked"),
                );
            }
        }

        if !use_chunked {
            parts.headers.remove(header::TRANSFER_ENCODING);
        }

        if keep_alive {
            if !parts.headers.contains_key(header::CONNECTION) {
                parts.headers.insert(
                    header::CONNECTION,
                    header::HeaderValue::from_static("keep-alive"),
                );
            }
        } else {
            parts.headers.insert(
                header::CONNECTION,
                header::HeaderValue::from_static("close"),
            );
        }
    }

    let ver = match parts.version {
        Version::HTTP_10 => "HTTP/1.0",
        _ => "HTTP/1.1",
    };
    let status = parts.status;
    let reason = status.canonical_reason().unwrap_or("");
    let mut head = format!("{ver} {} {reason}\r\n", status.as_u16());

    for (k, v) in parts.headers.iter() {
        head.push_str(k.as_str());
        head.push_str(": ");
        head.push_str(v.to_str().unwrap_or(""));
        head.push_str("\r\n");
    }
    head.push_str("\r\n");
    stream.write_all(head.as_bytes()).await?;

    if parts.status != http::StatusCode::SWITCHING_PROTOCOLS {
        if use_chunked {
            while let Some(chunk_res) = body.next().await {
                let chunk = chunk_res?;
                if chunk.is_empty() {
                    continue;
                }
                let header = format!("{:X}\r\n", chunk.len());
                stream.write_all(header.as_bytes()).await?;
                stream.write_all(&chunk).await?;
                stream.write_all(b"\r\n").await?;
            }
            stream.write_all(b"0\r\n\r\n").await?;
        } else {
            while let Some(chunk_res) = body.next().await {
                let chunk = chunk_res?;
                if !chunk.is_empty() {
                    stream.write_all(&chunk).await?;
                }
            }
        }
    }

    stream.flush().await?;
    Ok(status)
}
