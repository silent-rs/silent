pub struct QuicConnection {
    incoming: quinn::Incoming,
}

impl QuicConnection {
    pub fn new(incoming: quinn::Incoming) -> Self {
        Self { incoming }
    }
    pub fn into_incoming(self) -> quinn::Incoming {
        self.incoming
    }
}

// 为了与 Connection trait 的约束对齐，实现空的 AsyncRead/AsyncWrite。
// 实际 QUIC 连接不会通过这些接口读写，RouteConnectionService 会 downcast 并走 QUIC 处理路径。
impl tokio::io::AsyncRead for QuicConnection {
    fn poll_read(
        self: std::pin::Pin<&mut Self>,
        _cx: &mut std::task::Context<'_>,
        _buf: &mut tokio::io::ReadBuf<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        std::task::Poll::Ready(Err(std::io::Error::other(
            "QuicConnection does not support AsyncRead",
        )))
    }
}
impl tokio::io::AsyncWrite for QuicConnection {
    fn poll_write(
        self: std::pin::Pin<&mut Self>,
        _cx: &mut std::task::Context<'_>,
        _buf: &[u8],
    ) -> std::task::Poll<std::io::Result<usize>> {
        std::task::Poll::Ready(Err(std::io::Error::other(
            "QuicConnection does not support AsyncWrite",
        )))
    }
    fn poll_flush(
        self: std::pin::Pin<&mut Self>,
        _cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        std::task::Poll::Ready(Ok(()))
    }
    fn poll_shutdown(
        self: std::pin::Pin<&mut Self>,
        _cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        std::task::Poll::Ready(Ok(()))
    }
}
impl Unpin for QuicConnection {}

#[cfg(all(test, feature = "quic"))]
mod tests {
    use super::*;
    #[test]
    fn test_quic_connection_type_link() {
        // 仅验证类型可用（不构造实际值）
        let _ = std::mem::size_of::<QuicConnection>();
    }
}
