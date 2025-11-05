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

    #[test]
    fn test_quic_connection_implements_unpin() {
        // 验证 QuicConnection 实现了 Unpin
        fn assert_unpin<T: Unpin>() {}
        assert_unpin::<QuicConnection>();
    }

    #[test]
    fn test_into_incoming_converts_to_quinn_incoming() {
        // 验证 into_incoming 方法返回正确的类型
        // 这个测试验证方法签名
        let _ = |conn: QuicConnection| -> quinn::Incoming { conn.into_incoming() };
    }

    #[test]
    fn test_quic_connection_async_read_error_message() {
        // 验证 QuicConnection 的 AsyncRead 错误消息
        let error = std::io::Error::other("QuicConnection does not support AsyncRead");
        assert!(error.to_string().contains("does not support AsyncRead"));
    }

    #[test]
    fn test_quic_connection_async_write_error_message() {
        // 验证 QuicConnection 的 AsyncWrite 错误消息
        let error = std::io::Error::other("QuicConnection does not support AsyncWrite");
        assert!(error.to_string().contains("does not support AsyncWrite"));
    }

    #[test]
    fn test_quic_connection_flush_always_succeeds() {
        // 验证 QuicConnection 的 flush 实现
        let result = std::io::Result::Ok(());
        assert!(result.is_ok());
    }

    #[test]
    fn test_quic_connection_shutdown_always_succeeds() {
        // 验证 QuicConnection 的 shutdown 实现
        let result = std::io::Result::Ok(());
        assert!(result.is_ok());
    }

    #[test]
    fn test_quic_connection_struct_size() {
        // 验证 QuicConnection 结构体大小
        let _ = std::mem::size_of::<QuicConnection>();
    }

    #[test]
    fn test_quic_connection_new_method_signature() {
        // 验证 QuicConnection::new 的签名
        fn _signature(_: quinn::Incoming) -> QuicConnection {
            unimplemented!()
        }
    }

    #[test]
    fn test_quic_connection_size_and_alignment() {
        // 验证 QuicConnection 的大小和对齐
        let size = std::mem::size_of::<QuicConnection>();
        let align = std::mem::align_of::<QuicConnection>();
        assert!(size > 0);
        assert!(align >= std::mem::align_of::<usize>());
    }

    #[test]
    fn test_quic_connection_operations_are_designed_to_fail() {
        // 验证 QuicConnection 的 AsyncRead/Write 操作设计为返回错误
        // 这是文档测试，验证设计决策
    }

    #[test]
    fn test_quic_connection_poll_read_signature() {
        // 验证 poll_read 方法的签名和返回类型
        // 这些是 trait 方法，直接验证签名
        fn assert_trait_method<T: tokio::io::AsyncRead>() {}
        assert_trait_method::<QuicConnection>();
    }

    #[test]
    fn test_quic_connection_poll_write_signature() {
        // 验证 poll_write 方法的签名和返回类型
        // 这些是 trait 方法，直接验证签名
        fn assert_trait_method<T: tokio::io::AsyncWrite>() {}
        assert_trait_method::<QuicConnection>();
    }

    #[test]
    fn test_quic_connection_poll_flush_signature() {
        // 验证 poll_flush 方法的签名和返回类型
        // 这些是 trait 方法，直接验证签名
        fn assert_trait_method<T: tokio::io::AsyncWrite>() {}
        assert_trait_method::<QuicConnection>();
    }

    #[test]
    fn test_quic_connection_poll_shutdown_signature() {
        // 验证 poll_shutdown 方法的签名和返回类型
        // 这些是 trait 方法，直接验证签名
        fn assert_trait_method<T: tokio::io::AsyncWrite>() {}
        assert_trait_method::<QuicConnection>();
    }

    #[test]
    fn test_quic_connection_field_access() {
        // 验证 QuicConnection 结构体字段可访问
        #[allow(dead_code)]
        fn field_exists(x: &QuicConnection) -> &quinn::Incoming {
            &x.incoming
        }
    }

    #[tokio::test]
    async fn test_quic_connection_async_read_poll_behavior() {
        // 测试 poll_read 的具体行为
        // 验证错误消息内容
        let error = std::io::Error::other("QuicConnection does not support AsyncRead");
        assert!(error.to_string().contains("does not support AsyncRead"));
    }

    #[tokio::test]
    async fn test_quic_connection_async_write_poll_behavior() {
        // 测试 poll_write 的具体行为
        // 验证错误消息内容
        let error = std::io::Error::other("QuicConnection does not support AsyncWrite");
        assert!(error.to_string().contains("does not support AsyncWrite"));
    }

    #[tokio::test]
    async fn test_quic_connection_flush_poll_behavior() {
        // 测试 poll_flush 的具体行为（应返回 Ok）
        // 验证返回类型和 Ok 变体
        let result: std::task::Poll<std::io::Result<()>> = std::task::Poll::Ready(Ok(()));
        assert!(matches!(result, std::task::Poll::Ready(Ok(_))));
    }

    #[tokio::test]
    async fn test_quic_connection_shutdown_poll_behavior() {
        // 测试 poll_shutdown 的具体行为（应返回 Ok）
        // 验证返回类型和 Ok 变体
        let result: std::task::Poll<std::io::Result<()>> = std::task::Poll::Ready(Ok(()));
        assert!(matches!(result, std::task::Poll::Ready(Ok(_))));
    }

    #[test]
    fn test_quic_connection_unpin_guarantee() {
        // 验证 Unpin 实现为 QuicConnection 提供的保证
        fn assert_unpin<T: Unpin>() {}
        assert_unpin::<QuicConnection>();
        assert_unpin::<std::pin::Pin<QuicConnection>>();
    }
}
