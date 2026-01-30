use std::collections::VecDeque;
use std::error::Error as StdError;
use std::pin::Pin;
use std::task::{Context, Poll};

use bytes::Bytes;
use futures_util::TryStreamExt;
use futures_util::stream::{BoxStream, Stream};
use http_body::{Body, Frame, SizeHint};
use hyper::body::Incoming;

use crate::error::BoxedError;

/// 响应体
pub enum ResBody {
    /// None body.
    None,
    /// Once bytes body.
    Once(Bytes),
    /// Chunks body.
    Chunks(VecDeque<Bytes>),
    /// Incoming default body.
    Incoming(Incoming),
    /// Stream body.
    Stream(BoxStream<'static, Result<Bytes, BoxedError>>),
    /// Boxed body.
    Boxed(Pin<Box<dyn Body<Data = Bytes, Error = BoxedError> + Send>>),
}

/// 转换数据为响应Body
pub fn full<T: Into<Bytes>>(chunk: T) -> ResBody {
    ResBody::Once(chunk.into())
}

/// 转换数据为响应Body
pub fn stream_body<S, O, E>(stream: S) -> ResBody
where
    S: Stream<Item = Result<O, E>> + Send + 'static,
    O: Into<Bytes> + 'static,
    E: Into<Box<dyn StdError + Send + Sync>> + 'static,
{
    let mapped = stream.map_ok(Into::into).map_err(Into::into);
    ResBody::Stream(Box::pin(mapped))
}

impl Stream for ResBody {
    type Item = Result<Bytes, BoxedError>;

    #[inline]
    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        match self.get_mut() {
            ResBody::None => Poll::Ready(None),
            ResBody::Once(bytes) => {
                if bytes.is_empty() {
                    Poll::Ready(None)
                } else {
                    let bytes = std::mem::replace(bytes, Bytes::new());
                    Poll::Ready(Some(Ok(bytes)))
                }
            }
            ResBody::Chunks(chunks) => Poll::Ready(chunks.pop_front().map(Ok)),
            ResBody::Incoming(body) => match Body::poll_frame(Pin::new(body), cx) {
                Poll::Ready(Some(Ok(frame))) => Poll::Ready(frame.into_data().map(Ok).ok()),
                Poll::Ready(Some(Err(e))) => Poll::Ready(Some(Err(e.into()))),
                Poll::Ready(None) => Poll::Ready(None),
                Poll::Pending => Poll::Pending,
            },
            ResBody::Stream(stream) => stream.as_mut().poll_next(cx).map_err(Into::into),
            ResBody::Boxed(body) => match Body::poll_frame(Pin::new(body), cx) {
                Poll::Ready(Some(Ok(frame))) => Poll::Ready(frame.into_data().map(Ok).ok()),
                Poll::Ready(Some(Err(e))) => Poll::Ready(Some(Err(e))),
                Poll::Ready(None) => Poll::Ready(None),
                Poll::Pending => Poll::Pending,
            },
        }
    }
}

impl Body for ResBody {
    type Data = Bytes;
    type Error = BoxedError;

    fn poll_frame(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Result<Frame<Self::Data>, Self::Error>>> {
        match self.get_mut() {
            ResBody::None => Poll::Ready(None),
            ResBody::Once(bytes) => {
                if bytes.is_empty() {
                    Poll::Ready(None)
                } else {
                    let bytes = std::mem::replace(bytes, Bytes::new());
                    Poll::Ready(Some(Ok(Frame::data(bytes))))
                }
            }
            ResBody::Chunks(chunks) => {
                Poll::Ready(chunks.pop_front().map(|bytes| Ok(Frame::data(bytes))))
            }
            ResBody::Incoming(body) => Body::poll_frame(Pin::new(body), cx).map_err(Into::into),
            ResBody::Stream(stream) => stream.as_mut().poll_next(cx).map_ok(Frame::data),
            ResBody::Boxed(body) => Body::poll_frame(Pin::new(body), cx),
        }
    }

    fn is_end_stream(&self) -> bool {
        match self {
            ResBody::None => true,
            ResBody::Once(bytes) => bytes.is_empty(),
            ResBody::Chunks(chunks) => chunks.is_empty(),
            ResBody::Incoming(body) => body.is_end_stream(),
            ResBody::Boxed(body) => body.is_end_stream(),
            ResBody::Stream(_) => false,
        }
    }

    fn size_hint(&self) -> SizeHint {
        match self {
            ResBody::None => SizeHint::with_exact(0),
            ResBody::Once(bytes) => SizeHint::with_exact(bytes.len() as u64),
            ResBody::Chunks(chunks) => {
                let size = chunks.iter().map(|bytes| bytes.len() as u64).sum();
                SizeHint::with_exact(size)
            }
            ResBody::Incoming(recv) => recv.size_hint(),
            ResBody::Boxed(recv) => recv.size_hint(),
            ResBody::Stream(_) => SizeHint::default(),
        }
    }
}

impl From<Bytes> for ResBody {
    fn from(value: Bytes) -> ResBody {
        ResBody::Once(value)
    }
}

impl From<Incoming> for ResBody {
    fn from(value: Incoming) -> ResBody {
        ResBody::Incoming(value)
    }
}

impl From<String> for ResBody {
    #[inline]
    fn from(value: String) -> ResBody {
        ResBody::Once(value.into())
    }
}

impl From<&'static [u8]> for ResBody {
    fn from(value: &'static [u8]) -> ResBody {
        ResBody::Once(value.into())
    }
}

impl From<&'static str> for ResBody {
    fn from(value: &'static str) -> ResBody {
        ResBody::Once(value.into())
    }
}

impl From<Vec<u8>> for ResBody {
    fn from(value: Vec<u8>) -> ResBody {
        ResBody::Once(value.into())
    }
}

impl From<Box<[u8]>> for ResBody {
    fn from(value: Box<[u8]>) -> ResBody {
        ResBody::Once(value.into())
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::needless_borrow)]

    use super::*;
    use futures_util::stream;

    // ==================== full() 函数测试 ====================

    #[test]
    fn test_full_from_bytes() {
        let bytes = Bytes::from("hello");
        let body = full(bytes.clone());
        match body {
            ResBody::Once(b) => assert_eq!(b, bytes),
            _ => panic!("Expected Once variant"),
        }
    }

    #[test]
    fn test_full_from_str() {
        let body = full("hello world");
        match body {
            ResBody::Once(b) => assert_eq!(b, Bytes::from("hello world")),
            _ => panic!("Expected Once variant"),
        }
    }

    #[test]
    fn test_full_from_string() {
        let body = full(String::from("test"));
        match body {
            ResBody::Once(b) => assert_eq!(b, Bytes::from("test")),
            _ => panic!("Expected Once variant"),
        }
    }

    #[test]
    fn test_full_from_vec() {
        let body = full(vec![1, 2, 3, 4]);
        match body {
            ResBody::Once(b) => assert_eq!(b, Bytes::from(vec![1, 2, 3, 4])),
            _ => panic!("Expected Once variant"),
        }
    }

    #[test]
    fn test_full_empty() {
        let body = full(Bytes::new());
        match body {
            ResBody::Once(b) => assert!(b.is_empty()),
            _ => panic!("Expected Once variant"),
        }
    }

    // ==================== stream_body() 函数测试 ====================

    #[test]
    fn test_stream_body_from_stream() {
        let data: Vec<Result<Bytes, BoxedError>> =
            vec![Ok(Bytes::from("chunk1")), Ok(Bytes::from("chunk2"))];
        let s = stream::iter(data);
        let body = stream_body(s);

        match body {
            ResBody::Stream(_) => {}
            _ => panic!("Expected Stream variant"),
        }
    }

    #[test]
    fn test_stream_body_with_error() {
        use std::io;

        let data: Vec<Result<Bytes, io::Error>> =
            vec![Ok(Bytes::from("data")), Err(io::Error::other("test error"))];
        let s = stream::iter(data);
        let body = stream_body(s);

        match body {
            ResBody::Stream(_) => {}
            _ => panic!("Expected Stream variant"),
        }
    }

    // ==================== From trait 测试 ====================

    #[test]
    fn test_from_bytes() {
        let bytes = Bytes::from("test data");
        let body: ResBody = bytes.clone().into();
        match body {
            ResBody::Once(b) => assert_eq!(b, bytes),
            _ => panic!("Expected Once variant"),
        }
    }

    #[test]
    fn test_from_string() {
        let s = String::from("string data");
        let body: ResBody = s.clone().into();
        match body {
            ResBody::Once(b) => assert_eq!(b, Bytes::from(s)),
            _ => panic!("Expected Once variant"),
        }
    }

    #[test]
    fn test_from_static_str() {
        let body: ResBody = "static str".into();
        match body {
            ResBody::Once(b) => assert_eq!(b, Bytes::from("static str")),
            _ => panic!("Expected Once variant"),
        }
    }

    #[test]
    fn test_from_static_slice() {
        let body: ResBody = b"static bytes".as_ref().into();
        match body {
            ResBody::Once(b) => assert_eq!(b, Bytes::from(b"static bytes".as_ref())),
            _ => panic!("Expected Once variant"),
        }
    }

    #[test]
    fn test_from_vec_u8() {
        let data = vec![1, 2, 3, 4, 5];
        let body: ResBody = data.clone().into();
        match body {
            ResBody::Once(b) => assert_eq!(b, Bytes::from(data)),
            _ => panic!("Expected Once variant"),
        }
    }

    #[test]
    fn test_from_boxed_slice() {
        let data: Box<[u8]> = vec![10, 20, 30].into_boxed_slice();
        let body: ResBody = data.clone().into();
        match body {
            ResBody::Once(b) => assert_eq!(b, Bytes::from(data)),
            _ => panic!("Expected Once variant"),
        }
    }

    // ==================== Stream::poll_next() 测试 ====================

    #[test]
    fn test_poll_next_none() {
        let mut body = ResBody::None;
        let waker = std::task::Waker::noop();
        let mut cx = Context::from_waker(&waker);

        match Pin::new(&mut body).poll_next(&mut cx) {
            Poll::Ready(None) => {}
            _ => panic!("Expected Ready(None)"),
        }
    }

    #[test]
    fn test_poll_next_once_with_data() {
        let data = Bytes::from("test data");
        let mut body = ResBody::Once(data.clone());
        let waker = std::task::Waker::noop();
        let mut cx = Context::from_waker(&waker);

        // First poll should return data
        match Pin::new(&mut body).poll_next(&mut cx) {
            Poll::Ready(Some(Ok(bytes))) => assert_eq!(bytes, data),
            _ => panic!("Expected Ready(Some(Ok(bytes)))"),
        }

        // Second poll should return None
        match Pin::new(&mut body).poll_next(&mut cx) {
            Poll::Ready(None) => {}
            _ => panic!("Expected Ready(None) on second poll"),
        }
    }

    #[test]
    fn test_poll_next_once_empty() {
        let mut body = ResBody::Once(Bytes::new());
        let waker = std::task::Waker::noop();
        let mut cx = Context::from_waker(&waker);

        match Pin::new(&mut body).poll_next(&mut cx) {
            Poll::Ready(None) => {}
            _ => panic!("Expected Ready(None) for empty Once"),
        }
    }

    #[test]
    fn test_poll_next_chunks() {
        let mut chunks = VecDeque::new();
        chunks.push_back(Bytes::from("chunk1"));
        chunks.push_back(Bytes::from("chunk2"));
        chunks.push_back(Bytes::from("chunk3"));

        let mut body = ResBody::Chunks(chunks);
        let waker = std::task::Waker::noop();
        let mut cx = Context::from_waker(&waker);

        // Poll all chunks
        match Pin::new(&mut body).poll_next(&mut cx) {
            Poll::Ready(Some(Ok(bytes))) => assert_eq!(bytes, Bytes::from("chunk1")),
            _ => panic!("Expected data"),
        }

        match Pin::new(&mut body).poll_next(&mut cx) {
            Poll::Ready(Some(Ok(bytes))) => assert_eq!(bytes, Bytes::from("chunk2")),
            _ => panic!("Expected data"),
        }

        match Pin::new(&mut body).poll_next(&mut cx) {
            Poll::Ready(Some(Ok(bytes))) => assert_eq!(bytes, Bytes::from("chunk3")),
            _ => panic!("Expected data"),
        }

        // Fourth poll should return None
        match Pin::new(&mut body).poll_next(&mut cx) {
            Poll::Ready(None) => {}
            _ => panic!("Expected Ready(None) after all chunks"),
        }
    }

    #[test]
    fn test_poll_next_chunks_empty() {
        let mut body = ResBody::Chunks(VecDeque::new());
        let waker = std::task::Waker::noop();
        let mut cx = Context::from_waker(&waker);

        match Pin::new(&mut body).poll_next(&mut cx) {
            Poll::Ready(None) => {}
            _ => panic!("Expected Ready(None) for empty chunks"),
        }
    }

    #[test]
    fn test_poll_next_stream() {
        let data: Vec<Result<Bytes, BoxedError>> =
            vec![Ok(Bytes::from("s1")), Ok(Bytes::from("s2"))];
        let s = stream::iter(data);
        let mut body = ResBody::Stream(Box::pin(s));
        let waker = std::task::Waker::noop();
        let mut cx = Context::from_waker(&waker);

        match Pin::new(&mut body).poll_next(&mut cx) {
            Poll::Ready(Some(Ok(bytes))) => assert_eq!(bytes, Bytes::from("s1")),
            _ => panic!("Expected data"),
        }

        match Pin::new(&mut body).poll_next(&mut cx) {
            Poll::Ready(Some(Ok(bytes))) => assert_eq!(bytes, Bytes::from("s2")),
            _ => panic!("Expected data"),
        }

        match Pin::new(&mut body).poll_next(&mut cx) {
            Poll::Ready(None) => {}
            _ => panic!("Expected Ready(None) after stream ends"),
        }
    }

    // ==================== Body::poll_frame() 测试 ====================

    #[test]
    fn test_poll_frame_none() {
        let mut body = ResBody::None;
        let waker = std::task::Waker::noop();
        let mut cx = Context::from_waker(&waker);

        match Pin::new(&mut body).poll_frame(&mut cx) {
            Poll::Ready(None) => {}
            _ => panic!("Expected Ready(None)"),
        }
    }

    #[test]
    fn test_poll_frame_once_with_data() {
        let data = Bytes::from("frame data");
        let mut body = ResBody::Once(data.clone());
        let waker = std::task::Waker::noop();
        let mut cx = Context::from_waker(&waker);

        match Pin::new(&mut body).poll_frame(&mut cx) {
            Poll::Ready(Some(Ok(frame))) => {
                assert_eq!(frame.into_data().unwrap(), data);
            }
            _ => panic!("Expected Ready(Some(Ok(frame)))"),
        }

        match Pin::new(&mut body).poll_frame(&mut cx) {
            Poll::Ready(None) => {}
            _ => panic!("Expected Ready(None) on second poll"),
        }
    }

    #[test]
    fn test_poll_frame_once_empty() {
        let mut body = ResBody::Once(Bytes::new());
        let waker = std::task::Waker::noop();
        let mut cx = Context::from_waker(&waker);

        match Pin::new(&mut body).poll_frame(&mut cx) {
            Poll::Ready(None) => {}
            _ => panic!("Expected Ready(None) for empty Once"),
        }
    }

    #[test]
    fn test_poll_frame_chunks() {
        let mut chunks = VecDeque::new();
        chunks.push_back(Bytes::from("frame1"));
        chunks.push_back(Bytes::from("frame2"));

        let mut body = ResBody::Chunks(chunks);
        let waker = std::task::Waker::noop();
        let mut cx = Context::from_waker(&waker);

        match Pin::new(&mut body).poll_frame(&mut cx) {
            Poll::Ready(Some(Ok(frame))) => {
                assert_eq!(frame.into_data().unwrap(), Bytes::from("frame1"));
            }
            _ => panic!("Expected data"),
        }

        match Pin::new(&mut body).poll_frame(&mut cx) {
            Poll::Ready(Some(Ok(frame))) => {
                assert_eq!(frame.into_data().unwrap(), Bytes::from("frame2"));
            }
            _ => panic!("Expected data"),
        }

        match Pin::new(&mut body).poll_frame(&mut cx) {
            Poll::Ready(None) => {}
            _ => panic!("Expected Ready(None) after all chunks"),
        }
    }

    #[test]
    fn test_poll_frame_chunks_empty() {
        let mut body = ResBody::Chunks(VecDeque::new());
        let waker = std::task::Waker::noop();
        let mut cx = Context::from_waker(&waker);

        match Pin::new(&mut body).poll_frame(&mut cx) {
            Poll::Ready(None) => {}
            _ => panic!("Expected Ready(None) for empty chunks"),
        }
    }

    #[test]
    fn test_poll_frame_stream() {
        let data: Vec<Result<Bytes, BoxedError>> =
            vec![Ok(Bytes::from("stream1")), Ok(Bytes::from("stream2"))];
        let s = stream::iter(data);
        let mut body = ResBody::Stream(Box::pin(s));
        let waker = std::task::Waker::noop();
        let mut cx = Context::from_waker(&waker);

        match Pin::new(&mut body).poll_frame(&mut cx) {
            Poll::Ready(Some(Ok(frame))) => {
                assert_eq!(frame.into_data().unwrap(), Bytes::from("stream1"));
            }
            _ => panic!("Expected data"),
        }

        match Pin::new(&mut body).poll_frame(&mut cx) {
            Poll::Ready(Some(Ok(frame))) => {
                assert_eq!(frame.into_data().unwrap(), Bytes::from("stream2"));
            }
            _ => panic!("Expected data"),
        }

        match Pin::new(&mut body).poll_frame(&mut cx) {
            Poll::Ready(None) => {}
            _ => panic!("Expected Ready(None) after stream ends"),
        }
    }

    // ==================== is_end_stream() 测试 ====================

    #[test]
    fn test_is_end_stream_none() {
        let body = ResBody::None;
        assert!(Body::is_end_stream(&body));
    }

    #[test]
    fn test_is_end_stream_once_empty() {
        let body = ResBody::Once(Bytes::new());
        assert!(Body::is_end_stream(&body));
    }

    #[test]
    fn test_is_end_stream_once_with_data() {
        let body = ResBody::Once(Bytes::from("data"));
        assert!(!Body::is_end_stream(&body));
    }

    #[test]
    fn test_is_end_stream_chunks_empty() {
        let body = ResBody::Chunks(VecDeque::new());
        assert!(Body::is_end_stream(&body));
    }

    #[test]
    fn test_is_end_stream_chunks_with_data() {
        let mut chunks = VecDeque::new();
        chunks.push_back(Bytes::from("data"));
        let body = ResBody::Chunks(chunks);
        assert!(!Body::is_end_stream(&body));
    }

    #[test]
    fn test_is_end_stream_stream() {
        let data: Vec<Result<Bytes, BoxedError>> = vec![Ok(Bytes::from("data"))];
        let s = stream::iter(data);
        let body = ResBody::Stream(Box::pin(s));
        // Stream always returns false
        assert!(!Body::is_end_stream(&body));
    }

    // ==================== size_hint() 测试 ====================

    #[test]
    fn test_size_hint_none() {
        let body = ResBody::None;
        let hint = Body::size_hint(&body);
        assert_eq!(hint.lower(), 0);
        assert_eq!(hint.upper(), Some(0));
    }

    #[test]
    fn test_size_hint_once() {
        let data = Bytes::from("hello world");
        let body = ResBody::Once(data.clone());
        let hint = Body::size_hint(&body);
        assert_eq!(hint.lower(), data.len() as u64);
        assert_eq!(hint.upper(), Some(data.len() as u64));
    }

    #[test]
    fn test_size_hint_once_empty() {
        let body = ResBody::Once(Bytes::new());
        let hint = Body::size_hint(&body);
        assert_eq!(hint.lower(), 0);
        assert_eq!(hint.upper(), Some(0));
    }

    #[test]
    fn test_size_hint_chunks() {
        let mut chunks = VecDeque::new();
        chunks.push_back(Bytes::from("chunk1"));
        chunks.push_back(Bytes::from("chunk2"));
        chunks.push_back(Bytes::from("chunk3"));

        let body = ResBody::Chunks(chunks);
        let hint = Body::size_hint(&body);
        assert_eq!(hint.lower(), 6 + 6 + 6); // "chunk1" + "chunk2" + "chunk3"
        assert_eq!(hint.upper(), Some(18));
    }

    #[test]
    fn test_size_hint_chunks_empty() {
        let body = ResBody::Chunks(VecDeque::new());
        let hint = Body::size_hint(&body);
        assert_eq!(hint.lower(), 0);
        assert_eq!(hint.upper(), Some(0));
    }

    #[test]
    fn test_size_hint_stream() {
        let data: Vec<Result<Bytes, BoxedError>> = vec![Ok(Bytes::from("data"))];
        let s = stream::iter(data);
        let body = ResBody::Stream(Box::pin(s));
        let hint = Body::size_hint(&body);
        // Stream has no size information
        assert_eq!(hint.lower(), 0);
        assert_eq!(hint.upper(), None);
    }

    // ==================== 边界条件和特殊情况测试 ====================

    #[test]
    fn test_large_bytes() {
        let large_data = vec![0u8; 1024 * 1024]; // 1MB
        let body: ResBody = large_data.clone().into();
        match body {
            ResBody::Once(b) => {
                assert_eq!(b.len(), 1024 * 1024);
                assert_eq!(b, large_data.as_slice());
            }
            _ => panic!("Expected Once variant"),
        }
    }

    #[test]
    fn test_unicode_string() {
        let unicode = "Hello 世界 🌍";
        let body: ResBody = String::from(unicode).into();
        match body {
            ResBody::Once(b) => assert_eq!(b, Bytes::from(unicode)),
            _ => panic!("Expected Once variant"),
        }
    }

    #[test]
    fn test_multiple_chunks() {
        let mut chunks = VecDeque::new();
        for i in 0..100 {
            chunks.push_back(Bytes::from(format!("chunk{}", i)));
        }

        let body = ResBody::Chunks(chunks.clone());
        let hint = Body::size_hint(&body);
        let total_size: usize = chunks.iter().map(|b| b.len()).sum();
        assert_eq!(hint.lower(), total_size as u64);
    }

    #[test]
    fn test_empty_static_str() {
        let body: ResBody = "".into();
        match body {
            ResBody::Once(b) => assert!(b.is_empty()),
            _ => panic!("Expected Once variant"),
        }
    }

    #[test]
    fn test_empty_static_slice() {
        let body: ResBody = b"".as_ref().into();
        match body {
            ResBody::Once(b) => assert!(b.is_empty()),
            _ => panic!("Expected Once variant"),
        }
    }

    #[test]
    fn test_vec_with_zeros() {
        let data = vec![0u8; 100];
        let body: ResBody = data.clone().into();
        match body {
            ResBody::Once(b) => {
                assert_eq!(b.len(), 100);
                assert_eq!(b, data.as_slice());
            }
            _ => panic!("Expected Once variant"),
        }
    }

    #[test]
    fn test_chunk_consumption() {
        let mut chunks = VecDeque::new();
        chunks.push_back(Bytes::from("first"));
        chunks.push_back(Bytes::from("second"));
        chunks.push_back(Bytes::from("third"));

        let mut body = ResBody::Chunks(chunks);
        let waker = std::task::Waker::noop();
        let mut cx = Context::from_waker(&waker);

        // Consume all chunks via poll_next
        let mut results = Vec::new();
        loop {
            match Pin::new(&mut body).poll_next(&mut cx) {
                Poll::Ready(Some(Ok(bytes))) => results.push(bytes),
                Poll::Ready(None) => break,
                _ => panic!("Unexpected poll result"),
            }
        }

        assert_eq!(results.len(), 3);
        assert_eq!(results[0], Bytes::from("first"));
        assert_eq!(results[1], Bytes::from("second"));
        assert_eq!(results[2], Bytes::from("third"));

        // After consumption, is_end_stream should be true
        assert!(Body::is_end_stream(&body));
    }

    #[test]
    fn test_once_consumption() {
        let mut body = ResBody::Once(Bytes::from("single"));
        let waker = std::task::Waker::noop();
        let mut cx = Context::from_waker(&waker);

        // First poll returns data
        match Pin::new(&mut body).poll_next(&mut cx) {
            Poll::Ready(Some(Ok(bytes))) => assert_eq!(bytes, Bytes::from("single")),
            _ => panic!("Expected data"),
        }

        // Second poll returns None
        match Pin::new(&mut body).poll_next(&mut cx) {
            Poll::Ready(None) => {}
            _ => panic!("Expected Ready(None)"),
        }

        // After consumption, is_end_stream should be true
        assert!(Body::is_end_stream(&body));
    }

    // ==================== 类型验证测试 ====================

    #[test]
    fn test_res_body_none_variant() {
        let body = ResBody::None;
        assert!(matches!(body, ResBody::None));
    }

    #[test]
    fn test_res_body_once_variant() {
        let body = ResBody::Once(Bytes::from("test"));
        assert!(matches!(body, ResBody::Once(_)));
    }

    #[test]
    fn test_res_body_chunks_variant() {
        let chunks = VecDeque::from([Bytes::from("test")]);
        let body = ResBody::Chunks(chunks);
        assert!(matches!(body, ResBody::Chunks(_)));
    }

    #[test]
    fn test_res_body_stream_variant() {
        let data: Vec<Result<Bytes, BoxedError>> = vec![Ok(Bytes::from("test"))];
        let s = stream::iter(data);
        let body = ResBody::Stream(Box::pin(s));
        assert!(matches!(body, ResBody::Stream(_)));
    }
}
