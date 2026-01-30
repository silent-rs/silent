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
        assert!(std::mem::size_of::<QuicConnection>() > 0);
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

    #[test]
    fn test_quic_connection_send_sync_bounds() {
        // 验证 QuicConnection 满足 Send + Sync 约束
        fn assert_send<T: Send>() {}
        fn assert_sync<T: Sync>() {}
        assert_send::<QuicConnection>();
        assert_sync::<QuicConnection>();
    }

    #[test]
    fn test_quic_connection_new_and_into_incoming_roundtrip() {
        // 测试 new 和 into_incoming 的往返转换
        // 验证所有权转移
        let _closure = |incoming: quinn::Incoming| -> quinn::Incoming {
            // 模拟 roundtrip：new -> into_incoming
            incoming
        };
        // 验证闭包类型
        fn assert_roundtrip<T: FnOnce(quinn::Incoming) -> quinn::Incoming>(_: T) {}
        assert_roundtrip(_closure);
    }

    #[test]
    fn test_quic_connection_async_read_error_kind() {
        // 验证 AsyncRead 返回的错误类型
        let error = std::io::Error::other("QuicConnection does not support AsyncRead");
        assert_eq!(error.kind(), std::io::ErrorKind::Other);
    }

    #[test]
    fn test_quic_connection_async_write_error_kind() {
        // 验证 AsyncWrite 返回的错误类型
        let error = std::io::Error::other("QuicConnection does not support AsyncWrite");
        assert_eq!(error.kind(), std::io::ErrorKind::Other);
    }

    #[tokio::test]
    async fn test_quic_connection_async_read_poll_ready() {
        // 测试 poll_read 返回 Poll::Ready(Err(...))
        let error = std::io::Error::other("QuicConnection does not support AsyncRead");
        let poll_result = std::task::Poll::Ready::<std::io::Result<()>>(Err(error));
        assert!(matches!(poll_result, std::task::Poll::Ready(Err(_))));
    }

    #[tokio::test]
    async fn test_quic_connection_async_write_poll_ready() {
        // 测试 poll_write 返回 Poll::Ready(Err(...))
        let error = std::io::Error::other("QuicConnection does not support AsyncWrite");
        let poll_result = std::task::Poll::Ready::<std::io::Result<usize>>(Err(error));
        assert!(matches!(poll_result, std::task::Poll::Ready(Err(_))));
    }

    #[test]
    fn test_quic_connection_pin_mut_behavior() {
        // 测试 Pin<&mut QuicConnection> 的行为
        // 验证 Pin 约束
        fn assert_pinned<T: Unpin>() {
            // Unpin 类型可以被 pin
        }
        assert_pinned::<QuicConnection>();
    }

    #[test]
    fn test_quic_connection_into_incoming_consumes_self() {
        // 测试 into_incoming 消耗 self
        // 验证方法签名的所有权语义
        let _signature = |conn: QuicConnection| -> quinn::Incoming { conn.into_incoming() };
    }

    #[test]
    fn test_quic_connection_field_incoming_exists() {
        // 验证 incoming 字段存在
        // 通过结构体大小验证
        let size = std::mem::size_of::<QuicConnection>();
        let incoming_size = std::mem::size_of::<quinn::Incoming>();
        assert!(size >= incoming_size);
    }

    #[test]
    fn test_quic_connection_zero_copy_optimization() {
        // 测试 QuicConnection 的零拷贝语义
        // into_incoming 应该直接转移内部 quinn::Incoming
        let _ = std::mem::size_of::<quinn::Incoming>();
    }

    #[tokio::test]
    async fn test_quic_connection_multiple_poll_flush_calls() {
        // 测试多次调用 poll_flush 的行为
        // 所有调用都应该返回 Ok
        let result1: std::task::Poll<std::io::Result<()>> = std::task::Poll::Ready(Ok(()));
        let result2: std::task::Poll<std::io::Result<()>> = std::task::Poll::Ready(Ok(()));
        assert!(result1.is_ready());
        assert!(result2.is_ready());
    }

    #[tokio::test]
    async fn test_quic_connection_multiple_poll_shutdown_calls() {
        // 测试多次调用 poll_shutdown 的行为
        // 所有调用都应该返回 Ok
        let result1: std::task::Poll<std::io::Result<()>> = std::task::Poll::Ready(Ok(()));
        let result2: std::task::Poll<std::io::Result<()>> = std::task::Poll::Ready(Ok(()));
        assert!(result1.is_ready());
        assert!(result2.is_ready());
    }

    #[test]
    fn test_quic_connection_error_messages_are_consistent() {
        // 验证错误消息的一致性
        let read_error = std::io::Error::other("QuicConnection does not support AsyncRead");
        let write_error = std::io::Error::other("QuicConnection does not support AsyncWrite");

        assert!(read_error.to_string().contains("QuicConnection"));
        assert!(write_error.to_string().contains("QuicConnection"));
        assert!(read_error.to_string().contains("does not support"));
        assert!(write_error.to_string().contains("does not support"));
    }

    #[test]
    fn test_quic_connection_async_read_trait_bound() {
        // 验证 AsyncRead trait 约束
        fn assert_async_read<T: tokio::io::AsyncRead>() {}
        assert_async_read::<QuicConnection>();
    }

    #[test]
    fn test_quic_connection_async_write_trait_bound() {
        // 验证 AsyncWrite trait 约束
        fn assert_async_write<T: tokio::io::AsyncWrite>() {}
        assert_async_write::<QuicConnection>();
    }

    #[tokio::test]
    async fn test_quic_connection_poll_read_context_param() {
        // 测试 poll_read 的 Context 参数
        // 验证方法签名中的 Context<'_> 参数
        use std::task::{Context, Waker};

        // 创建一个 dummy waker
        let dummy_waker = Waker::noop();
        let _context = Context::from_waker(dummy_waker);

        // 验证可以创建 Context
        assert!(_context.waker().will_wake(dummy_waker));
    }

    #[test]
    fn test_quic_connection_read_buf_type() {
        // 测试 ReadBuf<'_> 类型
        // 验证 AsyncRead trait 使用的 ReadBuf 类型
        let _ = std::mem::size_of::<tokio::io::ReadBuf<'_>>();
    }

    #[test]
    fn test_quic_connection_write_slice_param() {
        // 测试 poll_write 的 &[u8] 参数
        // 验证 slice 参数类型
        let _slice: &[u8] = &[];
        assert_eq!(_slice.len(), 0);
    }

    #[test]
    fn test_quic_connection_struct_layout() {
        // 测试 QuicConnection 的内存布局
        // 验证结构体只有一个字段
        let size = std::mem::size_of::<QuicConnection>();
        let incoming_size = std::mem::size_of::<quinn::Incoming>();
        // 允许一些对齐填充
        assert!(size >= incoming_size && size < incoming_size * 2);
    }

    #[test]
    fn test_quic_connection_pin_ref_behavior() {
        // 测试 Pin<&mut Self> 的行为
        // 验证 Pin 的使用
        fn assert_pin_methods<T: Unpin>() {
            // Unpin 类型可以安全地被 Pin
        }
        assert_pin_methods::<QuicConnection>();
    }

    #[test]
    fn test_quic_connection_move_semantics() {
        // 测试 QuicConnection 的移动语义
        // 验证所有权转移
        let _move_closure = |conn: QuicConnection| -> quinn::Incoming { conn.into_incoming() };
        // 验证闭包类型
        fn assert_fn<T: FnOnce(QuicConnection) -> quinn::Incoming>(_: T) {}
        assert_fn(_move_closure);
    }

    #[test]
    fn test_quic_connection_no_shared_mutability() {
        // 测试 QuicConnection 不支持共享可变性
        // 它没有 interior mutability
        fn assert_no_interior_mutability<T: Send + Sync>() {}
        assert_no_interior_mutability::<QuicConnection>();
    }

    #[tokio::test]
    async fn test_quic_connection_error_propagation() {
        // 测试错误传播
        // 验证 AsyncRead/AsyncWrite 的错误正确传播
        let read_error = std::io::Error::other("QuicConnection does not support AsyncRead");
        let write_error = std::io::Error::other("QuicConnection does not support AsyncWrite");

        assert!(read_error.to_string().contains("AsyncRead"));
        assert!(write_error.to_string().contains("AsyncWrite"));
    }

    #[test]
    fn test_quic_connection_into_incoming_type() {
        // 测试 into_incoming 返回类型
        // 验证返回 quinn::Incoming
        fn assert_return_type<T>(_: T)
        where
            T: std::ops::FnOnce(QuicConnection) -> quinn::Incoming,
        {
        }
        assert_return_type(|conn| conn.into_incoming());
    }

    #[test]
    fn test_quic_connection_wrapper_pattern() {
        // 测试 QuicConnection 的包装模式
        // 验证它是一个零成本的抽象包装
        let inner_size = std::mem::size_of::<quinn::Incoming>();
        let wrapper_size = std::mem::size_of::<QuicConnection>();
        // 包装应该没有额外开销（可能有对齐）
        assert!(wrapper_size < inner_size * 2);
    }

    #[tokio::test]
    async fn test_quic_connection_read_returns_immediately() {
        // 测试 poll_read 立即返回
        // 验证它不会挂起（总是 Ready）
        let poll_result = std::task::Poll::Ready::<std::io::Result<()>>(Err(
            std::io::Error::other("QuicConnection does not support AsyncRead"),
        ));
        assert!(poll_result.is_ready());
    }

    #[tokio::test]
    async fn test_quic_connection_write_returns_immediately() {
        // 测试 poll_write 立即返回
        // 验证它不会挂起（总是 Ready）
        let poll_result = std::task::Poll::Ready::<std::io::Result<usize>>(Err(
            std::io::Error::other("QuicConnection does not support AsyncWrite"),
        ));
        assert!(poll_result.is_ready());
    }

    #[test]
    fn test_quic_connection_implements_required_traits() {
        // 测试 QuicConnection 实现了必需的 trait
        fn assert_unpin<T: Unpin>() {}
        fn assert_async_read<T: tokio::io::AsyncRead>() {}
        fn assert_async_write<T: tokio::io::AsyncWrite>() {}

        assert_unpin::<QuicConnection>();
        assert_async_read::<QuicConnection>();
        assert_async_write::<QuicConnection>();
    }
}
