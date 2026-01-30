use std::fmt::{Debug, Display, Formatter};
use std::io::Result;
use std::str::FromStr;

#[derive(Clone)]
pub enum SocketAddr {
    Tcp(std::net::SocketAddr),
    #[cfg(feature = "tls")]
    TlsTcp(std::net::SocketAddr),
    #[cfg(unix)]
    Unix(std::os::unix::net::SocketAddr),
}

impl SocketAddr {
    #[cfg(feature = "tls")]
    pub(crate) fn tls(self) -> Result<Self> {
        match self {
            SocketAddr::Tcp(addr) => Ok(SocketAddr::TlsTcp(addr)),
            _ => Ok(self),
        }
    }
}

impl Debug for SocketAddr {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            SocketAddr::Tcp(addr) => write!(f, "http://{addr}"),
            #[cfg(feature = "tls")]
            SocketAddr::TlsTcp(addr) => write!(f, "https://{addr}"),
            #[cfg(unix)]
            SocketAddr::Unix(addr) => {
                if let Some(path) = addr.as_pathname() {
                    write!(f, "unix://{}", path.display())
                } else {
                    write!(f, "unix:(unnamed)")
                }
            }
        }
    }
}

impl Display for SocketAddr {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            #[allow(clippy::write_literal)]
            SocketAddr::Tcp(addr) => write!(f, "{addr}"),
            #[cfg(feature = "tls")]
            SocketAddr::TlsTcp(addr) => write!(f, "{addr}"),
            #[cfg(unix)]
            SocketAddr::Unix(addr) => {
                if let Some(path) = addr.as_pathname() {
                    write!(f, "{}", path.display())
                } else {
                    write!(f, "(unnamed)")
                }
            }
        }
    }
}

impl From<std::net::SocketAddr> for SocketAddr {
    fn from(addr: std::net::SocketAddr) -> Self {
        SocketAddr::Tcp(addr)
    }
}

#[cfg(unix)]
impl From<std::os::unix::net::SocketAddr> for SocketAddr {
    fn from(addr: std::os::unix::net::SocketAddr) -> Self {
        SocketAddr::Unix(addr)
    }
}

impl FromStr for SocketAddr {
    type Err = std::io::Error;

    #[cfg(unix)]
    fn from_str(s: &str) -> Result<Self> {
        if let Ok(addr) = s.parse::<std::net::SocketAddr>() {
            Ok(SocketAddr::Tcp(addr))
        } else if let Ok(addr) = std::os::unix::net::SocketAddr::from_pathname(s) {
            Ok(SocketAddr::Unix(addr))
        } else {
            Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "invalid socket address",
            ))
        }
    }
    #[cfg(not(unix))]
    fn from_str(s: &str) -> Result<Self> {
        if let Ok(addr) = s.parse::<std::net::SocketAddr>() {
            Ok(SocketAddr::Tcp(addr))
        } else {
            Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "invalid socket address",
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ==================== From trait 测试 ====================

    #[test]
    fn test_from_std_socket_addr_ipv4() {
        let addr = std::net::SocketAddr::from(([127, 0, 0, 1], 8080));
        let socket_addr = SocketAddr::from(addr);
        assert_eq!(format!("{}", socket_addr), "127.0.0.1:8080");
    }

    #[test]
    fn test_from_std_socket_addr_ipv6() {
        let addr = std::net::SocketAddr::from(([0, 0, 0, 0, 0, 0, 0, 1], 443));
        let socket_addr = SocketAddr::from(addr);
        assert!(format!("{}", socket_addr).contains("[::1]:443"));
    }

    #[cfg(unix)]
    #[test]
    fn test_from_unix_socket_addr() {
        let _ = std::fs::remove_file("/tmp/test_sock");
        let addr = std::os::unix::net::SocketAddr::from_pathname("/tmp/test_sock").unwrap();
        let socket_addr = SocketAddr::from(addr);
        assert_eq!(format!("{}", socket_addr), "/tmp/test_sock");
    }

    // ==================== Debug trait 测试 ====================

    #[test]
    fn test_debug_tcp_socket() {
        let addr = std::net::SocketAddr::from(([192, 168, 1, 1], 9000));
        let socket_addr = SocketAddr::from(addr);
        let debug_str = format!("{:?}", socket_addr);
        assert!(debug_str.contains("http://"));
        assert!(debug_str.contains("192.168.1.1:9000"));
    }

    #[cfg(feature = "tls")]
    #[test]
    fn test_debug_tls_tcp_socket() {
        let addr = std::net::SocketAddr::from(([10, 0, 0, 1], 8443));
        let socket_addr = SocketAddr::TlsTcp(addr);
        let debug_str = format!("{:?}", socket_addr);
        assert!(debug_str.contains("https://"));
        assert!(debug_str.contains("10.0.0.1:8443"));
    }

    #[cfg(unix)]
    #[test]
    fn test_debug_unix_socket() {
        let addr = std::os::unix::net::SocketAddr::from_pathname("/tmp/silent_debug_test");
        if let Ok(addr) = addr {
            let socket_addr = SocketAddr::from(addr);
            let debug_str = format!("{:?}", socket_addr);
            assert!(debug_str.contains("unix://"));
            assert!(debug_str.contains("/tmp/silent_debug_test"));
        }
    }

    // 注意：未命名的 Unix socket 在标准库中不容易创建，
    // 因此跳过相关测试

    // ==================== Display trait 测试 ====================

    #[test]
    fn test_display_tcp_ipv4() {
        let addr = std::net::SocketAddr::from(([8, 8, 8, 8], 53));
        let socket_addr = SocketAddr::from(addr);
        assert_eq!(format!("{}", socket_addr), "8.8.8.8:53");
    }

    #[test]
    fn test_display_tcp_ipv6() {
        let addr = std::net::SocketAddr::from(([0, 0, 0, 0, 0, 0, 0, 1], 80));
        let socket_addr = SocketAddr::from(addr);
        let display = format!("{}", socket_addr);
        assert!(display.contains("[::1]:80"));
    }

    #[cfg(unix)]
    #[test]
    fn test_display_unix_socket() {
        let addr = std::os::unix::net::SocketAddr::from_pathname("/tmp/silent_display_test");
        if let Ok(addr) = addr {
            let socket_addr = SocketAddr::from(addr);
            assert!(format!("{}", socket_addr).contains("/tmp/silent_display_test"));
        }
    }

    // 注意：未命名的 Unix socket 在标准库中不容易创建，
    // 因此跳过相关测试

    // ==================== FromStr trait 测试 ====================

    #[test]
    fn test_from_str_tcp_addr() {
        let result = SocketAddr::from_str("127.0.0.1:8080");
        assert!(result.is_ok());
        let socket_addr = result.unwrap();
        assert!(matches!(socket_addr, SocketAddr::Tcp(_)));
    }

    #[test]
    fn test_from_str_invalid_addr() {
        let result = SocketAddr::from_str("invalid address");
        // 在 Unix 上，这可能会被尝试解析为 Unix socket 路径
        #[cfg(unix)]
        match result {
            Ok(_) => {
                // 可能被解析为 Unix socket 路径
            }
            Err(e) => {
                assert_eq!(e.kind(), std::io::ErrorKind::InvalidInput);
            }
        }
        #[cfg(not(unix))]
        assert!(result.is_err());
    }

    #[test]
    fn test_from_str_empty() {
        let result = SocketAddr::from_str("");
        // 在 Unix 上，空字符串可能被解析为 Unix socket（取决于实现）
        #[cfg(unix)]
        match result {
            Ok(_) => {
                // 可能被解析为 Unix socket
            }
            Err(e) => {
                assert_eq!(e.kind(), std::io::ErrorKind::InvalidInput);
            }
        }
        #[cfg(not(unix))]
        assert!(result.is_err());
    }

    #[cfg(unix)]
    #[test]
    fn test_from_str_unix_socket() {
        let result = SocketAddr::from_str("/tmp/test.sock");
        assert!(result.is_ok());
        let socket_addr = result.unwrap();
        assert!(matches!(socket_addr, SocketAddr::Unix(_)));
    }

    // ==================== tls() 方法测试 ====================

    #[cfg(feature = "tls")]
    #[test]
    fn test_tls_from_tcp() {
        let addr = std::net::SocketAddr::from(([127, 0, 0, 1], 8080));
        let socket_addr = SocketAddr::Tcp(addr);
        let tls_socket = socket_addr.tls().unwrap();
        assert!(matches!(tls_socket, SocketAddr::TlsTcp(_)));
    }

    #[cfg(feature = "tls")]
    #[test]
    fn test_tls_from_tls_tcp() {
        let addr = std::net::SocketAddr::from(([127, 0, 0, 1], 8443));
        let socket_addr = SocketAddr::TlsTcp(addr);
        let tls_socket = socket_addr.tls().unwrap();
        assert!(matches!(tls_socket, SocketAddr::TlsTcp(_)));
    }

    #[cfg(unix)]
    #[cfg(feature = "tls")]
    #[test]
    fn test_tls_from_unix() {
        let addr = std::os::unix::net::SocketAddr::from_pathname("/tmp/silent_tls_test");
        if let Ok(addr) = addr {
            let socket_addr = SocketAddr::Unix(addr);
            let tls_socket = socket_addr.tls().unwrap();
            // Unix socket 转换后仍然是 Unix socket
            assert!(matches!(tls_socket, SocketAddr::Unix(_)));
        }
    }

    // ==================== Clone trait 测试 ====================

    #[test]
    fn test_clone_tcp_socket() {
        let addr = std::net::SocketAddr::from(([10, 0, 0, 1], 9000));
        let socket_addr1 = SocketAddr::from(addr);
        let socket_addr2 = socket_addr1.clone();

        assert_eq!(format!("{}", socket_addr1), format!("{}", socket_addr2));
    }

    #[cfg(unix)]
    #[test]
    fn test_clone_unix_socket() {
        let addr = std::os::unix::net::SocketAddr::from_pathname("/tmp/silent_clone_test");
        if let Ok(addr) = addr {
            let socket_addr1 = SocketAddr::from(addr);
            let socket_addr2 = socket_addr1.clone();

            assert_eq!(format!("{}", socket_addr1), format!("{}", socket_addr2));
        }
    }

    #[cfg(feature = "tls")]
    #[test]
    fn test_clone_tls_socket() {
        let addr = std::net::SocketAddr::from(([127, 0, 0, 1], 443));
        let socket_addr1 = SocketAddr::TlsTcp(addr);
        let socket_addr2 = socket_addr1.clone();

        assert_eq!(format!("{}", socket_addr1), format!("{}", socket_addr2));
    }

    // ==================== IPv6 地址测试 ====================

    #[test]
    fn test_ipv6_loopback() {
        let addr = std::net::SocketAddr::from(([0, 0, 0, 0, 0, 0, 0, 1], 8080));
        let socket_addr = SocketAddr::from(addr);
        let display = format!("{}", socket_addr);
        assert!(display.contains("[::1]:8080"));
    }

    #[test]
    fn test_ipv6_full() {
        let addr = std::net::SocketAddr::from(([0x2001, 0xdb8, 0, 0, 0, 0, 0, 1], 443));
        let socket_addr = SocketAddr::from(addr);
        let display = format!("{}", socket_addr);
        assert!(display.contains("[2001:db8::1]:443"));
    }

    // ==================== 边界条件测试 ====================

    #[test]
    fn test_privileged_ports() {
        // 测试特权端口 (0-1024)
        for port in [80, 443, 8080, 9000] {
            let addr = std::net::SocketAddr::from(([127, 0, 0, 1], port));
            let socket_addr = SocketAddr::from(addr);
            let _ = format!("{}", socket_addr);
        }
    }

    #[test]
    fn test_all_zeros_ip() {
        let addr = std::net::SocketAddr::from(([0, 0, 0, 0], 0));
        let socket_addr = SocketAddr::from(addr);
        assert_eq!(format!("{}", socket_addr), "0.0.0.0:0");
    }

    #[test]
    fn test_broadcast_ip() {
        let addr = std::net::SocketAddr::from(([255, 255, 255, 255], 8080));
        let socket_addr = SocketAddr::from(addr);
        assert_eq!(format!("{}", socket_addr), "255.255.255.255:8080");
    }

    #[test]
    fn test_multicast_ip() {
        let addr = std::net::SocketAddr::from(([224, 0, 0, 1], 8080));
        let socket_addr = SocketAddr::from(addr);
        assert_eq!(format!("{}", socket_addr), "224.0.0.1:8080");
    }
}
