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
    use std::io::ErrorKind;
    use std::str::FromStr;

    // ==================== From trait 测试 ====================

    #[test]
    fn test_remote_addr_from_socket_addr_tcp() {
        let addr = std::net::SocketAddr::from(([127, 0, 0, 1], 8080));
        let socket_addr = SocketAddr::from(addr);
        let remote = RemoteAddr::from(socket_addr);
        assert_eq!(remote.to_string(), "127.0.0.1:8080");
        assert_eq!(remote.ip().unwrap().to_string(), "127.0.0.1");
    }

    #[test]
    fn test_remote_addr_from_std_socket_addr_ipv4() {
        let addr = std::net::SocketAddr::from(([192, 168, 1, 1], 3000));
        let remote = RemoteAddr::from(addr);
        assert_eq!(remote.to_string(), "192.168.1.1:3000");
        assert_eq!(remote.ip().unwrap().to_string(), "192.168.1.1");
    }

    #[test]
    fn test_remote_addr_from_std_socket_addr_ipv6() {
        let addr = std::net::SocketAddr::from(([0, 0, 0, 0, 0, 0, 0, 1], 8080));
        let remote = RemoteAddr::from(addr);
        assert!(remote.to_string().contains("[::1]:8080"));
        assert_eq!(remote.ip().unwrap().to_string(), "::1");
    }

    #[cfg(unix)]
    #[test]
    fn test_remote_addr_from_unix_socket_addr() {
        use std::os::unix::net::SocketAddr as UnixSocketAddr;

        let path = "/tmp/test.sock";
        if let Ok(addr) = UnixSocketAddr::from_pathname(path) {
            let remote = RemoteAddr::from(addr);
            assert!(remote.to_string().contains(path));
            // Unix socket 应该返回 None
            assert!(remote.ip().is_none());
        }
    }

    // ==================== FromStr trait 测试 ====================

    #[test]
    fn test_from_str_socket_addr_ipv4() {
        let remote = RemoteAddr::from_str("127.0.0.1:8080").unwrap();
        assert_eq!(remote.to_string(), "127.0.0.1:8080");
        assert_eq!(remote.ip().unwrap().to_string(), "127.0.0.1");
    }

    #[test]
    fn test_from_str_socket_addr_ipv6() {
        let remote = RemoteAddr::from_str("[::1]:8080").unwrap();
        assert!(remote.to_string().contains("[::1]:8080"));
        assert_eq!(remote.ip().unwrap().to_string(), "::1");
    }

    #[test]
    fn test_from_str_ipv4_only() {
        let remote = RemoteAddr::from_str("127.0.0.1").unwrap();
        assert_eq!(remote.to_string(), "127.0.0.1");
        assert_eq!(remote.ip().unwrap().to_string(), "127.0.0.1");
    }

    #[test]
    fn test_from_str_ipv6_only() {
        let remote = RemoteAddr::from_str("::1").unwrap();
        assert_eq!(remote.to_string(), "::1");
        assert_eq!(remote.ip().unwrap().to_string(), "::1");
    }

    #[test]
    fn test_from_str_ipv6_full() {
        let remote = RemoteAddr::from_str("2001:db8::1").unwrap();
        assert_eq!(remote.to_string(), "2001:db8::1");
        assert_eq!(remote.ip().unwrap().to_string(), "2001:db8::1");
    }

    #[cfg(unix)]
    #[test]
    fn test_from_str_unix_socket() {
        let remote = RemoteAddr::from_str("/tmp/test.sock").unwrap();
        assert!(remote.to_string().contains("/tmp/test.sock"));
        // Unix socket 应该返回 None
        assert!(remote.ip().is_none());
    }

    #[test]
    fn test_from_str_invalid() {
        // 使用包含非法字符的字符串，这在所有平台上都应该失败
        let result = RemoteAddr::from_str("\0invalid");
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().kind(), ErrorKind::InvalidInput);
    }

    #[test]
    fn test_from_str_empty_string() {
        // 空字符串在某些平台上可能被解析为Unix socket
        // 这个测试只验证调用不会panic
        let result = RemoteAddr::from_str("");
        // 不同平台行为不同，我们只验证返回类型
        match result {
            Ok(_) | Err(_) => {
                // 两种情况都是可接受的
            }
        }
    }

    // ==================== ip() 方法测试 ====================

    #[test]
    fn test_ip_tcp_socket() {
        let addr = std::net::SocketAddr::from(([10, 0, 0, 1], 9000));
        let socket_addr = SocketAddr::from(addr);
        let remote = RemoteAddr::from(socket_addr);

        let ip = remote.ip().unwrap();
        assert_eq!(
            ip,
            std::net::IpAddr::V4(std::net::Ipv4Addr::new(10, 0, 0, 1))
        );
    }

    #[test]
    fn test_ip_ipv4_variant() {
        let remote = RemoteAddr::Ipv4(std::net::Ipv4Addr::new(192, 168, 1, 100));
        let ip = remote.ip().unwrap();
        assert_eq!(
            ip,
            std::net::IpAddr::V4(std::net::Ipv4Addr::new(192, 168, 1, 100))
        );
    }

    #[test]
    fn test_ip_ipv6_variant() {
        let remote = RemoteAddr::Ipv6(std::net::Ipv6Addr::new(0, 0, 0, 0, 0, 0, 0, 1));
        let ip = remote.ip().unwrap();
        assert_eq!(
            ip,
            std::net::IpAddr::V6(std::net::Ipv6Addr::new(0, 0, 0, 0, 0, 0, 0, 1))
        );
    }

    #[cfg(unix)]
    #[test]
    fn test_ip_unix_socket_none() {
        use std::os::unix::net::SocketAddr as UnixSocketAddr;

        let path = "/tmp/test.sock";
        if let Ok(addr) = UnixSocketAddr::from_pathname(path) {
            let socket_addr = SocketAddr::from(addr);
            let remote = RemoteAddr::from(socket_addr);
            // Unix socket 应该返回 None
            assert!(remote.ip().is_none());
        }
    }

    // ==================== Display trait 测试 ====================

    #[test]
    fn test_display_tcp_socket() {
        let addr = std::net::SocketAddr::from(([8, 8, 8, 8], 53));
        let socket_addr = SocketAddr::from(addr);
        let remote = RemoteAddr::from(socket_addr);
        assert_eq!(format!("{}", remote), "8.8.8.8:53");
    }

    #[test]
    fn test_display_ipv4() {
        let remote = RemoteAddr::Ipv4(std::net::Ipv4Addr::new(1, 1, 1, 1));
        assert_eq!(format!("{}", remote), "1.1.1.1");
    }

    #[test]
    fn test_display_ipv6() {
        let remote = RemoteAddr::Ipv6(std::net::Ipv6Addr::new(0x2001, 0xdb8, 0, 0, 0, 0, 0, 1));
        assert_eq!(format!("{}", remote), "2001:db8::1");
    }

    #[test]
    fn test_display_ipv6_loopback() {
        let remote = RemoteAddr::Ipv6(std::net::Ipv6Addr::new(0, 0, 0, 0, 0, 0, 0, 1));
        assert_eq!(format!("{}", remote), "::1");
    }

    // ==================== Debug trait 测试 ====================

    #[test]
    fn test_debug_tcp_socket() {
        let addr = std::net::SocketAddr::from(([127, 0, 0, 1], 8080));
        let socket_addr = SocketAddr::from(addr);
        let remote = RemoteAddr::from(socket_addr);

        let debug_str = format!("{:?}", remote);
        assert!(debug_str.contains("Socket"));
        assert!(debug_str.contains("127.0.0.1:8080"));
    }

    #[test]
    fn test_debug_ipv4() {
        let remote = RemoteAddr::Ipv4(std::net::Ipv4Addr::new(10, 0, 0, 1));
        let debug_str = format!("{:?}", remote);
        assert!(debug_str.contains("Ipv4"));
        assert!(debug_str.contains("10.0.0.1"));
    }

    #[test]
    fn test_debug_ipv6() {
        let remote = RemoteAddr::Ipv6(std::net::Ipv6Addr::new(0, 0, 0, 0, 0, 0, 0, 1));
        let debug_str = format!("{:?}", remote);
        assert!(debug_str.contains("Ipv6"));
        assert!(debug_str.contains("::1"));
    }

    // ==================== Clone trait 测试 ====================

    #[test]
    fn test_clone_remote_addr() {
        let addr = std::net::SocketAddr::from(([192, 168, 1, 1], 8080));
        let socket_addr = SocketAddr::from(addr);
        let remote1 = RemoteAddr::from(socket_addr);

        let remote2 = remote1.clone();

        assert_eq!(remote1.to_string(), remote2.to_string());
        assert_eq!(remote1.ip(), remote2.ip());
    }

    #[test]
    fn test_clone_ipv4() {
        let remote1 = RemoteAddr::Ipv4(std::net::Ipv4Addr::new(1, 2, 3, 4));
        let remote2 = remote1.clone();

        assert_eq!(remote1.to_string(), remote2.to_string());
    }

    #[test]
    fn test_clone_ipv6() {
        let remote1 = RemoteAddr::Ipv6(std::net::Ipv6Addr::new(0, 0, 0, 0, 0, 0, 0, 1));
        let remote2 = remote1.clone();

        assert_eq!(remote1.to_string(), remote2.to_string());
    }

    // ==================== 边界条件测试 ====================

    #[test]
    fn test_from_str_localhost() {
        // localhost 不是标准的 IP 或 Socket 地址格式
        // 在Unix平台上可能被解析为Unix socket路径
        let result = RemoteAddr::from_str("localhost");
        // 验证调用的有效性
        match result {
            Ok(remote) => {
                // 如果成功解析，可能是Unix socket或某种有效地址
                let _ = remote.to_string();
            }
            Err(e) => {
                // 如果失败，应该是InvalidInput错误
                assert_eq!(e.kind(), ErrorKind::InvalidInput);
            }
        }
    }

    #[test]
    fn test_ipv4_private_ranges() {
        // 10.0.0.0/8
        let remote = RemoteAddr::from_str("10.0.0.1").unwrap();
        assert_eq!(remote.ip().unwrap().to_string(), "10.0.0.1");

        // 172.16.0.0/12
        let remote = RemoteAddr::from_str("172.16.0.1").unwrap();
        assert_eq!(remote.ip().unwrap().to_string(), "172.16.0.1");

        // 192.168.0.0/16
        let remote = RemoteAddr::from_str("192.168.1.1").unwrap();
        assert_eq!(remote.ip().unwrap().to_string(), "192.168.1.1");
    }

    #[test]
    fn test_ipv6_loopback_various_formats() {
        // 所有这些格式都应该解析为同样的地址
        let addr1 = RemoteAddr::from_str("::1").unwrap();
        let addr2 = RemoteAddr::from_str("0:0:0:0:0:0:0:1").unwrap();
        let addr3 = RemoteAddr::from_str("0000:0000:0000:0000:0000:0000:0000:0001").unwrap();

        assert_eq!(addr1.to_string(), addr2.to_string());
        assert_eq!(addr2.to_string(), addr3.to_string());
    }
}
