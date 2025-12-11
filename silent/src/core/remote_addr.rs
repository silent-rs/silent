use crate::core::socket_addr::SocketAddr;
use std::fmt::{Debug, Display, Formatter};
use std::io;
use std::net::{Ipv4Addr, Ipv6Addr};
use std::str::FromStr;

/// 表示请求的远端地址信息。
///
/// - `Socket` 变体：带端口的套接字地址（TCP/TLS/Unix）。
/// - `Ipv4` / `Ipv6` 变体：仅包含 IP、不带端口的远端信息（常见于反向代理注入的 `X-Real-IP`）。
#[derive(Clone)]
pub enum RemoteAddr {
    Socket(SocketAddr),
    Ipv4(Ipv4Addr),
    Ipv6(Ipv6Addr),
}

impl RemoteAddr {
    /// 提取远端 IP 信息。
    ///
    /// - 对于 `Socket`，提取其中的 IP（Unix Socket 返回 `None`）。
    /// - 对于 `Ipv4`/`Ipv6`，直接返回对应的 `IpAddr`。
    #[inline]
    pub fn ip(&self) -> Option<std::net::IpAddr> {
        match self {
            RemoteAddr::Socket(SocketAddr::Tcp(addr)) => Some(addr.ip()),
            #[cfg(feature = "tls")]
            RemoteAddr::Socket(SocketAddr::TlsTcp(addr)) => Some(addr.ip()),
            #[cfg(unix)]
            RemoteAddr::Socket(SocketAddr::Unix(_)) => None,
            RemoteAddr::Ipv4(ip) => Some(std::net::IpAddr::V4(*ip)),
            RemoteAddr::Ipv6(ip) => Some(std::net::IpAddr::V6(*ip)),
        }
    }
}

impl From<SocketAddr> for RemoteAddr {
    fn from(inner: SocketAddr) -> Self {
        RemoteAddr::Socket(inner)
    }
}

impl From<std::net::SocketAddr> for RemoteAddr {
    fn from(addr: std::net::SocketAddr) -> Self {
        RemoteAddr::Socket(SocketAddr::from(addr))
    }
}

#[cfg(unix)]
impl From<std::os::unix::net::SocketAddr> for RemoteAddr {
    fn from(addr: std::os::unix::net::SocketAddr) -> Self {
        RemoteAddr::Socket(SocketAddr::from(addr))
    }
}

impl FromStr for RemoteAddr {
    type Err = io::Error;

    fn from_str(s: &str) -> io::Result<Self> {
        // 优先尝试解析为标准的 IP:PORT 套接字地址
        if let Ok(sock) = s.parse::<std::net::SocketAddr>() {
            return Ok(RemoteAddr::from(sock));
        }
        // 然后尝试解析为纯 IPv4 地址
        if let Ok(ipv4) = s.parse::<Ipv4Addr>() {
            return Ok(RemoteAddr::Ipv4(ipv4));
        }
        // 再尝试解析为纯 IPv6 地址
        if let Ok(ipv6) = s.parse::<Ipv6Addr>() {
            return Ok(RemoteAddr::Ipv6(ipv6));
        }
        // 最后在 unix 平台上尝试解析为 Unix Socket 路径
        #[cfg(unix)]
        if let Ok(unix) = std::os::unix::net::SocketAddr::from_pathname(s) {
            return Ok(RemoteAddr::from(unix));
        }

        Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "invalid remote address",
        ))
    }
}

impl Debug for RemoteAddr {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            RemoteAddr::Socket(addr) => f.debug_tuple("Socket").field(addr).finish(),
            RemoteAddr::Ipv4(ip) => f.debug_tuple("Ipv4").field(ip).finish(),
            RemoteAddr::Ipv6(ip) => f.debug_tuple("Ipv6").field(ip).finish(),
        }
    }
}

impl Display for RemoteAddr {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            RemoteAddr::Socket(addr) => Display::fmt(addr, f),
            RemoteAddr::Ipv4(ip) => Display::fmt(ip, f),
            RemoteAddr::Ipv6(ip) => Display::fmt(ip, f),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::RemoteAddr;
    use crate::core::socket_addr::SocketAddr;
    use std::str::FromStr;

    #[test]
    fn test_remote_addr_from_socket_addr() {
        let addr = std::net::SocketAddr::from(([127, 0, 0, 1], 8080));
        let socket_addr = SocketAddr::from(addr);
        let remote = RemoteAddr::from(socket_addr);
        assert_eq!(remote.to_string(), "127.0.0.1:8080");
        assert_eq!(remote.ip().unwrap().to_string(), "127.0.0.1");
    }

    #[test]
    fn test_remote_addr_from_ip_only_str() {
        let remote = RemoteAddr::from_str("127.0.0.1").unwrap();
        assert_eq!(remote.ip().unwrap().to_string(), "127.0.0.1");
        assert_eq!(remote.to_string(), "127.0.0.1");
    }
}
