//! `healthcheck`：探测正在运行的服务。容器镜像里没有 curl、wget 时用它。
//!
//! 只发一个最简单的 HTTP/1.1 请求，不引入 HTTP 客户端。

use std::io;
use std::net::SocketAddr;
use std::time::Duration;

use silent::StatusCode;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

/// 探测失败的原因。
#[derive(Debug, thiserror::Error)]
pub enum HealthError {
    /// 连不上。
    #[error("连不上 {addr}：{source}")]
    Connect {
        /// 探测的地址。
        addr: String,
        /// 操作系统给出的错误。
        source: io::Error,
    },
    /// 超时没回应。
    #[error("{addr} 在 {timeout_ms} 毫秒内没有回应")]
    Timeout {
        /// 探测的地址。
        addr: String,
        /// 等了多久。
        timeout_ms: u64,
    },
    /// 回应不是 HTTP。
    #[error("{addr} 的回应不是 HTTP 响应：{line:?}")]
    Malformed {
        /// 探测的地址。
        addr: String,
        /// 收到的第一行。
        line: String,
    },
    /// 地址的写法不对。
    #[error("探测地址 {url:?} 不对：只支持 http://IP地址:端口/路径")]
    Url {
        /// 传进来的地址。
        url: String,
    },
}

/// 解析 `http://127.0.0.1:8080/healthz`，得到地址和路径。主机只认 IP，不认域名。
pub fn parse_url(url: &str) -> Result<(SocketAddr, String), HealthError> {
    let invalid = || HealthError::Url {
        url: url.to_owned(),
    };
    let rest = url.strip_prefix("http://").ok_or_else(invalid)?;
    let (authority, path) = match rest.find('/') {
        Some(at) => (&rest[..at], rest[at..].to_owned()),
        None => (rest, "/".to_owned()),
    };
    let (host, port) = if let Some(bracketed) = authority.strip_prefix('[') {
        let close = bracketed.find(']').ok_or_else(invalid)?;
        let port = bracketed[close + 1..]
            .strip_prefix(':')
            .ok_or_else(invalid)?;
        (&bracketed[..close], port)
    } else {
        authority.rsplit_once(':').ok_or_else(invalid)?
    };
    let ip: std::net::IpAddr = host.parse().map_err(|_| invalid())?;
    let port: u16 = port.parse().map_err(|_| invalid())?;
    Ok((SocketAddr::new(ip, port), path))
}

/// 请求 `addr` 上的 `path`，返回 HTTP 状态码。连接、超时、响应格式的问题都算失败。
pub async fn check(
    addr: SocketAddr,
    path: &str,
    timeout: Duration,
) -> Result<StatusCode, HealthError> {
    let text = addr.to_string();
    let attempt = async {
        let mut stream = tokio::net::TcpStream::connect(addr)
            .await
            .map_err(|source| HealthError::Connect {
                addr: text.clone(),
                source,
            })?;
        let request = format!(
            "GET {path} HTTP/1.1\r\nhost: {addr}\r\nuser-agent: silent-boot/healthcheck\r\nconnection: close\r\n\r\n"
        );
        stream
            .write_all(request.as_bytes())
            .await
            .map_err(|source| HealthError::Connect {
                addr: text.clone(),
                source,
            })?;
        let mut buf = Vec::new();
        // 读到响应头结束就够，状态码在第一行。
        let mut chunk = [0u8; 1024];
        loop {
            let n = stream
                .read(&mut chunk)
                .await
                .map_err(|source| HealthError::Connect {
                    addr: text.clone(),
                    source,
                })?;
            if n == 0 {
                break;
            }
            buf.extend_from_slice(&chunk[..n]);
            if buf.windows(4).any(|w| w == b"\r\n\r\n") || buf.len() > 16 * 1024 {
                break;
            }
        }
        let head = String::from_utf8_lossy(&buf);
        let line = head.lines().next().unwrap_or_default().to_owned();
        // `HTTP/1.1 200 OK`
        let status = line
            .split_whitespace()
            .nth(1)
            .and_then(|code| code.parse::<u16>().ok())
            .ok_or_else(|| HealthError::Malformed {
                addr: text.clone(),
                line,
            })?;
        Ok(StatusCode::from_u16(status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR))
    };
    tokio::time::timeout(timeout, attempt)
        .await
        .unwrap_or(Err(HealthError::Timeout {
            addr: text,
            timeout_ms: timeout.as_millis() as u64,
        }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn url_is_parsed() {
        let (addr, path) = parse_url("http://127.0.0.1:8080/healthz").unwrap();
        assert_eq!(addr.to_string(), "127.0.0.1:8080");
        assert_eq!(path, "/healthz");
        let (addr, path) = parse_url("http://[::1]:9000").unwrap();
        assert_eq!(addr.to_string(), "[::1]:9000");
        assert_eq!(path, "/");
        assert!(parse_url("https://a.example/healthz").is_err());
        assert!(parse_url("127.0.0.1:8080").is_err());
        assert!(parse_url("http://localhost:8080/").is_err());
        assert!(parse_url("http://[::1").is_err());
        assert!(parse_url("http://[::1]9000").is_err());
    }

    #[tokio::test]
    async fn check_reports_the_status_of_a_real_listener() {
        use silent::prelude::*;
        let route = Route::new("")
            .append(Route::new("ok").get(|_req: Request| async { Ok("fine") }))
            .append(
                Route::new("broken")
                    .get(|_req: Request| async { Err::<Response, _>(SilentError::NotFound) }),
            );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            Server::new()
                .listen(silent::Listener::from(listener))
                .serve(route)
                .await;
        });
        assert_eq!(
            check(addr, "/ok", Duration::from_secs(5)).await.unwrap(),
            StatusCode::OK
        );
        assert_eq!(
            check(addr, "/broken", Duration::from_secs(5))
                .await
                .unwrap(),
            StatusCode::NOT_FOUND
        );
        server.abort();
    }

    #[tokio::test]
    async fn check_fails_on_closed_ports_and_timeouts() {
        // 先占一个端口再放掉，拿到一个确定没人监听的地址。
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        drop(listener);
        assert!(matches!(
            check(addr, "/healthz", Duration::from_secs(1)).await,
            Err(HealthError::Connect { .. })
        ));

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        // 接了连接但什么都不回：连接要一直拿着，放掉的话对端会读到结束。
        tokio::spawn(async move {
            let mut held = Vec::new();
            while let Ok((stream, _)) = listener.accept().await {
                held.push(stream);
            }
        });
        assert!(matches!(
            check(addr, "/healthz", Duration::from_millis(200)).await,
            Err(HealthError::Timeout { .. })
        ));
    }
}
