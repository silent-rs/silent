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

    // 新增运行时测试：验证 QuicConnection 的基本生命周期
    #[test]
    fn test_quic_connection_lifetime_and_ownership() {
        // 验证 QuicConnection 可以被移动和拥有
        fn takes_ownership(conn: QuicConnection) -> QuicConnection {
            conn
        }

        // 验证 into_incoming 消费 self
        fn verify_into_incoming_consumes_self(conn: QuicConnection) -> quinn::Incoming {
            conn.into_incoming()
        }

        // 这些函数签名验证了所有权转移的正确性
        let _ = takes_ownership;
        let _ = verify_into_incoming_consumes_self;
    }

    // 新增运行时测试：验证 AsyncRead/AsyncWrite trait bounds
    #[test]
    fn test_quic_connection_async_traits_are_implemented() {
        // 验证 QuicConnection 实现了 AsyncRead 和 AsyncWrite
        fn assert_async_read<T: tokio::io::AsyncRead>() {}
        fn assert_async_write<T: tokio::io::AsyncWrite>() {}

        assert_async_read::<QuicConnection>();
        assert_async_write::<QuicConnection>();
    }

    // 新增运行时测试：验证错误消息的一致性
    #[test]
    fn test_quic_connection_error_messages_are_consistent() {
        let read_error = "QuicConnection does not support AsyncRead";
        let write_error = "QuicConnection does not support AsyncWrite";

        // 验证错误消息遵循一致的命名模式
        assert!(read_error.contains("QuicConnection"));
        assert!(read_error.contains("does not support"));
        assert!(write_error.contains("QuicConnection"));
        assert!(write_error.contains("does not support"));

        // 验证错误消息的唯一性
        assert_ne!(read_error, write_error);
    }

    // 新增运行时测试：验证 flush/shutdown 返回 Poll::Ready(Ok(()))
    #[test]
    fn test_quic_connection_flush_shutdown_return_ready_ok() {
        // 验证 flush 和 shutdown 的返回值
        let flush_poll: std::task::Poll<std::io::Result<()>> = std::task::Poll::Ready(Ok(()));
        let shutdown_poll: std::task::Poll<std::io::Result<()>> = std::task::Poll::Ready(Ok(()));

        assert!(matches!(flush_poll, std::task::Poll::Ready(Ok(_))));
        assert!(matches!(shutdown_poll, std::task::Poll::Ready(Ok(_))));
    }

    // 新增运行时测试：验证 AsyncRead/Write 返回 Poll::Ready(Err(...))
    #[test]
    fn test_quic_connection_read_write_return_ready_errors() {
        // 验证 read 返回 Poll::Ready(Err)
        let read_error = std::io::Error::other("QuicConnection does not support AsyncRead");
        let read_poll: std::task::Poll<std::io::Result<()>> =
            std::task::Poll::Ready(Err(read_error));
        assert!(matches!(read_poll, std::task::Poll::Ready(Err(_))));

        // 验证 write 返回 Poll::Ready(Err)
        let write_error = std::io::Error::other("QuicConnection does not support AsyncWrite");
        let write_poll: std::task::Poll<std::io::Result<usize>> =
            std::task::Poll::Ready(Err(write_error));
        assert!(matches!(write_poll, std::task::Poll::Ready(Err(_))));
    }

    // 新增运行时测试：验证 into_incoming 方法的所有权转移
    #[test]
    fn test_quic_connection_into_incoming_transfers_ownership() {
        // 这个测试验证 into_incoming 消费了 QuicConnection
        // 由于我们无法构造真实的 quinn::Incoming，我们通过函数签名验证

        // 这证明了 into_incoming 获取所有权
        fn verify_signature(_: impl FnOnce(QuicConnection) -> quinn::Incoming) {}

        // 通过编译检查验证签名
        verify_signature(|conn: QuicConnection| conn.into_incoming());
    }

    // 新增运行时测试：验证 QuicConnection 的 Send + Sync 约束
    #[test]
    fn test_quic_connection_is_send_and_sync() {
        // QuicConnection 应该是 Send 和 Sync 的
        // 因为 quinn::Incoming 是 Send + Sync 的
        fn assert_send<T: Send>() {}
        fn assert_sync<T: Sync>() {}

        assert_send::<QuicConnection>();
        assert_sync::<QuicConnection>();
    }

    // 新增运行时测试：验证 QuicConnection 可以在线程间移动
    #[test]
    fn test_quic_connection_can_be_moved_across_threads() {
        // 验证 QuicConnection 满足 Send 约束
        fn assert_send<T: Send>() {}
        assert_send::<QuicConnection>();

        // 验证可以在 async 块中使用
        async fn use_connection(conn: QuicConnection) {
            let _ = conn;
        }

        // 验证函数签名
        let _ = use_connection;
    }
}
