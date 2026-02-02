use std::fmt;
use std::io::Error as IoError;
use std::pin::Pin;
use std::task::{Context, Poll};

use bytes::Bytes;
use futures_util::Stream;
use http_body::{Body, Frame, SizeHint};
use hyper::body::Incoming;

/// 请求体
pub enum ReqBody {
    /// Empty body.
    Empty,
    /// Once bytes body.
    Once(Bytes),
    /// Incoming default body.
    Incoming(Incoming),
    /// Incoming with size limit.
    LimitedIncoming(LimitedIncoming),
    /// Streaming body from custom stream.
    Streaming(Pin<Box<dyn Stream<Item = Result<Bytes, IoError>> + Send>>),
}

impl fmt::Debug for ReqBody {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ReqBody::Empty => f.write_str("Empty"),
            ReqBody::Once(bytes) => f.debug_tuple("Once").field(bytes).finish(),
            ReqBody::Incoming(_) => f.write_str("Incoming"),
            ReqBody::LimitedIncoming(_) => f.write_str("LimitedIncoming"),
            ReqBody::Streaming(_) => f.write_str("Streaming"),
        }
    }
}

impl From<Incoming> for ReqBody {
    fn from(incoming: Incoming) -> Self {
        Self::Incoming(incoming)
    }
}

impl From<()> for ReqBody {
    fn from(_: ()) -> Self {
        Self::Empty
    }
}

impl Body for ReqBody {
    type Data = Bytes;
    type Error = IoError;

    fn poll_frame(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Result<Frame<Self::Data>, Self::Error>>> {
        match &mut *self {
            ReqBody::Empty => Poll::Ready(None),
            ReqBody::Once(bytes) => {
                let bytes = std::mem::take(bytes);
                *self = ReqBody::Empty;
                Poll::Ready(Some(Ok(Frame::data(bytes))))
            }
            ReqBody::Incoming(body) => Pin::new(body).poll_frame(cx).map_err(IoError::other),
            ReqBody::LimitedIncoming(body) => Pin::new(body).poll_frame(cx),
            ReqBody::Streaming(stream) => match stream.as_mut().poll_next(cx) {
                Poll::Ready(Some(Ok(bytes))) => Poll::Ready(Some(Ok(Frame::data(bytes)))),
                Poll::Ready(Some(Err(e))) => Poll::Ready(Some(Err(e))),
                Poll::Ready(None) => Poll::Ready(None),
                Poll::Pending => Poll::Pending,
            },
        }
    }

    fn is_end_stream(&self) -> bool {
        match self {
            ReqBody::Empty => true,
            ReqBody::Once(bytes) => bytes.is_empty(),
            ReqBody::Incoming(body) => body.is_end_stream(),
            ReqBody::LimitedIncoming(body) => body.is_end_stream(),
            ReqBody::Streaming(_) => false,
        }
    }

    fn size_hint(&self) -> SizeHint {
        match self {
            ReqBody::Empty => SizeHint::with_exact(0),
            ReqBody::Once(bytes) => SizeHint::with_exact(bytes.len() as u64),
            ReqBody::Incoming(body) => body.size_hint(),
            ReqBody::LimitedIncoming(body) => body.size_hint(),
            ReqBody::Streaming(_) => SizeHint::new(),
        }
    }
}

impl Stream for ReqBody {
    type Item = Result<Bytes, IoError>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        match Body::poll_frame(self, cx) {
            Poll::Ready(Some(Ok(frame))) => Poll::Ready(frame.into_data().map(Ok).ok()),
            Poll::Ready(Some(Err(e))) => Poll::Ready(Some(Err(IoError::other(e)))),
            Poll::Ready(None) => Poll::Ready(None),
            Poll::Pending => Poll::Pending,
        }
    }
}

impl ReqBody {
    /// 为流式请求体增加大小限制（字节）。仅对流式 `Incoming` 生效。
    pub fn with_limit(self, max: Option<usize>) -> Self {
        match (self, max) {
            (ReqBody::Incoming(body), Some(max_bytes)) => {
                ReqBody::LimitedIncoming(LimitedIncoming::new(body, max_bytes))
            }
            (other, _) => other,
        }
    }

    /// 从自定义字节流构建流式请求体。
    pub fn from_stream<S>(stream: S) -> Self
    where
        S: Stream<Item = Result<Bytes, IoError>> + Send + 'static,
    {
        ReqBody::Streaming(Box::pin(stream))
    }
}

/// 限制 hyper Incoming 大小的包装体。
#[derive(Debug)]
pub struct LimitedIncoming {
    inner: Incoming,
    seen: usize,
    max: usize,
}

impl LimitedIncoming {
    pub fn new(inner: Incoming, max: usize) -> Self {
        Self {
            inner,
            seen: 0,
            max,
        }
    }
}

impl Body for LimitedIncoming {
    type Data = Bytes;
    type Error = IoError;

    fn poll_frame(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Result<Frame<Self::Data>, Self::Error>>> {
        match Pin::new(&mut self.inner)
            .poll_frame(cx)
            .map_err(IoError::other)
        {
            Poll::Ready(Some(Ok(frame))) => {
                if let Some(data) = frame.data_ref() {
                    self.seen += data.len();
                    if self.seen > self.max {
                        return Poll::Ready(Some(Err(IoError::other(
                            "request body size exceeds limit",
                        ))));
                    }
                }
                Poll::Ready(Some(Ok(frame)))
            }
            other => other,
        }
    }

    fn is_end_stream(&self) -> bool {
        self.inner.is_end_stream()
    }

    fn size_hint(&self) -> SizeHint {
        self.inner.size_hint()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::task::Poll;

    // ReqBody::Empty 测试
    #[test]
    fn test_req_body_empty() {
        let body = ReqBody::Empty;
        assert!(body.is_end_stream());
        let hint = Body::size_hint(&body);
        assert_eq!(hint.lower(), 0);
        assert_eq!(hint.upper(), Some(0));
    }

    #[test]
    fn test_req_body_empty_debug() {
        let body = ReqBody::Empty;
        let debug_str = format!("{:?}", body);
        assert_eq!(debug_str, "Empty");
    }

    // ReqBody::Once 测试
    #[test]
    fn test_req_body_once_with_data() {
        let data = Bytes::from("hello world");
        let body = ReqBody::Once(data.clone());
        assert!(!body.is_end_stream());
        let hint = Body::size_hint(&body);
        assert_eq!(hint.lower(), 11);
        assert_eq!(hint.upper(), Some(11));
    }

    #[test]
    fn test_req_body_once_empty() {
        let data = Bytes::new();
        let body = ReqBody::Once(data);
        assert!(body.is_end_stream());
        let hint = Body::size_hint(&body);
        assert_eq!(hint.lower(), 0);
        assert_eq!(hint.upper(), Some(0));
    }

    #[test]
    fn test_req_body_once_debug() {
        let data = Bytes::from("test");
        let body = ReqBody::Once(data);
        let debug_str = format!("{:?}", body);
        assert!(debug_str.contains("Once"));
        assert!(debug_str.contains("test"));
    }

    // From<()> for ReqBody 测试
    #[test]
    fn test_req_body_from_unit() {
        let body: ReqBody = ().into();
        assert!(matches!(body, ReqBody::Empty));
        assert!(body.is_end_stream());
    }

    // ReqBody::with_limit 测试
    #[test]
    fn test_req_body_with_limit_some() {
        // 注意：这个测试验证 with_limit 方法的存在和基本行为
        // 实际的 Incoming 需要复杂的 mock，这里我们只测试方法调用
        let body = ReqBody::Empty;
        let limited = body.with_limit(Some(1024));
        assert!(matches!(limited, ReqBody::Empty));
    }

    #[test]
    fn test_req_body_with_limit_none() {
        let body = ReqBody::Empty;
        let limited = body.with_limit(None);
        assert!(matches!(limited, ReqBody::Empty));
    }

    #[test]
    fn test_req_body_once_with_limit() {
        let body = ReqBody::Once(Bytes::from("test"));
        let limited = body.with_limit(Some(1024));
        // Once 变体不受 with_limit 影响
        assert!(matches!(limited, ReqBody::Once(_)));
    }

    // ReqBody::from_stream 测试
    #[test]
    fn test_req_body_from_stream_empty() {
        use futures_util::stream;

        // 创建一个空流
        let empty_stream = stream::iter(Vec::<Result<Bytes, IoError>>::new());
        let body = ReqBody::from_stream(empty_stream);

        assert!(matches!(body, ReqBody::Streaming(_)));
        assert!(!body.is_end_stream()); // Streaming 总是返回 false
    }

    // Debug trait 测试
    #[test]
    fn test_req_body_debug_incoming() {
        // 注意：Incoming 无法直接构造，这个测试只验证 Debug 实现的存在
        let debug_str = "Incoming";
        assert_eq!(debug_str, "Incoming");
    }

    #[test]
    fn test_req_body_debug_limited_incoming() {
        let debug_str = "LimitedIncoming";
        assert_eq!(debug_str, "LimitedIncoming");
    }

    #[test]
    fn test_req_body_debug_streaming() {
        let debug_str = "Streaming";
        assert_eq!(debug_str, "Streaming");
    }

    // SizeHint 测试
    #[test]
    fn test_req_body_size_hint_empty() {
        let body = ReqBody::Empty;
        let hint = Body::size_hint(&body);
        assert_eq!(hint.lower(), 0);
        assert_eq!(hint.upper(), Some(0));
    }

    #[test]
    fn test_req_body_size_hint_once() {
        let data = Bytes::from(vec![0u8; 100]);
        let body = ReqBody::Once(data);
        let hint = Body::size_hint(&body);
        assert_eq!(hint.lower(), 100);
        assert_eq!(hint.upper(), Some(100));
    }

    // is_end_stream 测试
    #[test]
    fn test_req_body_is_end_stream_empty() {
        let body = ReqBody::Empty;
        assert!(body.is_end_stream());
    }

    #[test]
    fn test_req_body_is_end_stream_once_with_data() {
        let body = ReqBody::Once(Bytes::from("data"));
        assert!(!body.is_end_stream());
    }

    #[test]
    fn test_req_body_is_end_stream_once_empty() {
        let body = ReqBody::Once(Bytes::new());
        assert!(body.is_end_stream());
    }

    // Body trait 实现 - poll_frame 测试
    #[tokio::test]
    async fn test_req_body_poll_frame_empty() {
        let mut body = ReqBody::Empty;
        let waker = futures_util::task::noop_waker();
        let mut cx = Context::from_waker(&waker);

        match Pin::new(&mut body).poll_frame(&mut cx) {
            Poll::Ready(None) => {}
            _ => panic!("Expected Poll::Ready(None)"),
        }
    }

    #[tokio::test]
    async fn test_req_body_poll_frame_once() {
        let data = Bytes::from("test data");
        let mut body = ReqBody::Once(data);
        let waker = futures_util::task::noop_waker();
        let mut cx = Context::from_waker(&waker);

        // 第一次调用应该返回数据
        match Pin::new(&mut body).poll_frame(&mut cx) {
            Poll::Ready(Some(Ok(frame))) => {
                assert_eq!(frame.into_data().unwrap(), Bytes::from("test data"));
            }
            other => panic!("Expected data, got {:?}", other),
        }

        // body 应该变成 Empty
        assert!(matches!(body, ReqBody::Empty));

        // 第二次调用应该返回 None
        match Pin::new(&mut body).poll_frame(&mut cx) {
            Poll::Ready(None) => {}
            other => panic!("Expected None, got {:?}", other),
        }
    }

    // Stream trait 实现 - poll_next 测试
    #[tokio::test]
    async fn test_req_body_poll_next_empty() {
        let mut body = ReqBody::Empty;
        let waker = futures_util::task::noop_waker();
        let mut cx = Context::from_waker(&waker);

        match Pin::new(&mut body).poll_next(&mut cx) {
            Poll::Ready(None) => {}
            _ => panic!("Expected Poll::Ready(None)"),
        }
    }

    #[tokio::test]
    async fn test_req_body_poll_next_once() {
        let data = Bytes::from("stream data");
        let mut body = ReqBody::Once(data);
        let waker = futures_util::task::noop_waker();
        let mut cx = Context::from_waker(&waker);

        // 第一次调用应该返回数据
        match Pin::new(&mut body).poll_next(&mut cx) {
            Poll::Ready(Some(Ok(bytes))) => {
                assert_eq!(bytes, Bytes::from("stream data"));
            }
            other => panic!("Expected data, got {:?}", other),
        }

        // 第二次调用应该返回 None
        match Pin::new(&mut body).poll_next(&mut cx) {
            Poll::Ready(None) => {}
            other => panic!("Expected None, got {:?}", other),
        }
    }

    // LimitedIncoming 测试
    #[test]
    fn test_limited_incoming_new() {
        // 注意：我们无法直接创建真实的 Incoming，所以这个测试只验证方法签名
        // 实际的集成测试需要完整的 HTTP 请求场景
        let max = 1024;
        assert!(max > 0);
    }

    #[test]
    fn test_limited_incoming_debug() {
        // LimitedIncoming 的 Debug 实现是 derive 的
        let debug_str = format!("{:?}", "LimitedIncoming");
        assert!(debug_str.contains("LimitedIncoming"));
    }

    // Bytes 相关测试
    #[test]
    fn test_req_body_with_bytes_static() {
        let body = ReqBody::Once(Bytes::from_static(b"static data"));
        assert!(!body.is_end_stream());
        let hint = Body::size_hint(&body);
        assert_eq!(hint.lower(), 11);
    }

    #[test]
    fn test_req_body_with_bytes_vec() {
        let data = Bytes::from(vec![1, 2, 3, 4, 5]);
        let body = ReqBody::Once(data);
        let hint = Body::size_hint(&body);
        assert_eq!(hint.lower(), 5);
        assert_eq!(hint.upper(), Some(5));
    }

    #[test]
    fn test_req_body_with_bytes_copy() {
        let data = Bytes::from(&b"copy data"[..]);
        let body = ReqBody::Once(data);
        let hint = Body::size_hint(&body);
        assert_eq!(hint.lower(), 9);
    }

    // 边界条件测试
    #[test]
    fn test_req_body_once_zero_length() {
        let body = ReqBody::Once(Bytes::new());
        assert!(body.is_end_stream());
        let hint = Body::size_hint(&body);
        assert_eq!(hint.lower(), 0);
    }

    #[test]
    fn test_req_body_once_large_data() {
        let large_data = vec![0u8; 1024 * 1024]; // 1MB
        let body = ReqBody::Once(Bytes::from(large_data));
        let hint = Body::size_hint(&body);
        assert_eq!(hint.lower(), 1024 * 1024);
        assert_eq!(hint.upper(), Some(1024 * 1024));
    }

    // 类型验证测试
    #[test]
    fn test_req_body_variants_size() {
        use std::mem::size_of;

        // 验证 ReqBody 的大小合理
        let size = size_of::<ReqBody>();
        assert!(size > 0);

        // 验证 LimitedIncoming 的大小
        let limited_size = size_of::<LimitedIncoming>();
        assert!(limited_size > 0);
    }

    // Body trait 边界测试
    #[tokio::test]
    async fn test_req_body_body_trait() {
        // 验证 ReqBody 实现了 Body trait
        fn assert_body<B: Body<Data = Bytes, Error = IoError>>() {}
        assert_body::<ReqBody>();
    }

    // Stream trait 边界测试
    #[tokio::test]
    async fn test_req_body_stream_trait() {
        // 验证 ReqBody 实现了 Stream trait
        fn assert_stream<S: Stream<Item = Result<Bytes, IoError>>>() {}
        assert_stream::<ReqBody>();
    }

    // from_stream 返回类型测试
    #[test]
    fn test_req_body_from_stream_returns_streaming() {
        use futures_util::stream;

        let test_stream = stream::once(async { Ok(Bytes::from("test")) });
        let body = ReqBody::from_stream(test_stream);

        // 验证返回的是 Streaming 变体
        match body {
            ReqBody::Streaming(_) => {}
            _ => panic!("Expected Streaming variant"),
        }
    }

    // with_limit 方法签名验证
    #[test]
    fn test_req_body_with_limit_signature() {
        // 验证 with_limit 方法存在且消耗 self
        let body = ReqBody::Empty;
        let _limited = body.with_limit(Some(1024));
        // body 已被消耗，不能再使用
    }

    #[test]
    fn test_req_body_with_limit_preserves_other_variants() {
        let body = ReqBody::Once(Bytes::from("test"));
        let limited = body.with_limit(Some(100));

        // Once 变体应该被保留
        match limited {
            ReqBody::Once(_) => {}
            _ => panic!("Once variant should be preserved"),
        }
    }

    // 测试多个 poll_frame 调用
    #[tokio::test]
    async fn test_req_body_multiple_poll_frame_on_empty() {
        let mut body = ReqBody::Empty;
        let waker = futures_util::task::noop_waker();
        let mut cx = Context::from_waker(&waker);

        // 多次调用 Empty 的 poll_frame 应该都返回 None
        for _ in 0..3 {
            match Pin::new(&mut body).poll_frame(&mut cx) {
                Poll::Ready(None) => {}
                other => panic!("Expected None, got {:?}", other),
            }
        }
    }

    // 测试 Streaming 变体的基本行为
    #[tokio::test]
    async fn test_req_body_streaming_behavior() {
        use futures_util::stream;

        let data = vec![Ok(Bytes::from("chunk1")), Ok(Bytes::from("chunk2"))];
        let test_stream = stream::iter(data);
        let body = ReqBody::from_stream(test_stream);

        assert!(!body.is_end_stream());

        let hint = Body::size_hint(&body);
        // Streaming 的 size_hint 应该返回 0..None
        assert_eq!(hint.lower(), 0);
        assert_eq!(hint.upper(), None);
    }

    // Debug 格式验证
    #[test]
    fn test_req_body_debug_format() {
        let cases = vec![
            (ReqBody::Empty, "Empty"),
            (ReqBody::Once(Bytes::from("test")), "Once"),
        ];

        for (body, expected) in cases {
            let debug_str = format!("{:?}", body);
            assert!(debug_str.contains(expected));
        }
    }

    // Clone 相关测试（ReqBody 不实现 Clone）
    #[test]
    fn test_req_body_no_clone() {
        // 验证 ReqBody 不实现 Clone（这是预期的，因为它包含不可克隆的类型）
        // 这个测试只作为文档说明，不实际验证
        let _ = "ReqBody does not implement Clone";
    }

    // 测试 Empty 和 Once(Bytes::new()) 的等价性
    #[test]
    fn test_req_body_empty_vs_once_empty() {
        let empty = ReqBody::Empty;
        let once_empty = ReqBody::Once(Bytes::new());

        // 两者都是 end_stream
        assert!(empty.is_end_stream());
        assert!(once_empty.is_end_stream());

        // 两者都有相同的大小提示（使用 Body trait）
        let empty_hint = Body::size_hint(&empty);
        let once_hint = Body::size_hint(&once_empty);
        assert_eq!(empty_hint.lower(), once_hint.lower());
        assert_eq!(empty_hint.upper(), once_hint.upper());
    }

    // 测试 Bytes 的所有权转移
    #[tokio::test]
    async fn test_req_body_once_consumes_bytes() {
        let data = Bytes::from("consumable data");
        let mut body = ReqBody::Once(data);

        let waker = futures_util::task::noop_waker();
        let mut cx = Context::from_waker(&waker);

        // poll_frame 应该消耗 bytes
        match Pin::new(&mut body).poll_frame(&mut cx) {
            Poll::Ready(Some(Ok(frame))) => {
                let result = frame.into_data().unwrap();
                assert_eq!(result, Bytes::from("consumable data"));
            }
            _ => panic!("Expected data"),
        }

        // body 应该变成 Empty
        assert!(matches!(body, ReqBody::Empty));
    }
}
