use std::io::Error as IoError;
use std::pin::Pin;
use std::task::{Context, Poll};

use bytes::Bytes;
use futures_util::Stream;
use http_body::{Body, Frame, SizeHint};
use hyper::body::Incoming;

#[derive(Debug)]
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
            ReqBody::Once(bytes) => Poll::Ready(Some(Ok(Frame::data(bytes.clone())))),
            ReqBody::Incoming(body) => Pin::new(body).poll_frame(cx).map_err(IoError::other),
            ReqBody::LimitedIncoming(body) => Pin::new(body).poll_frame(cx),
        }
    }

    fn is_end_stream(&self) -> bool {
        match self {
            ReqBody::Empty => true,
            ReqBody::Once(bytes) => bytes.is_empty(),
            ReqBody::Incoming(body) => body.is_end_stream(),
            ReqBody::LimitedIncoming(body) => body.is_end_stream(),
        }
    }

    fn size_hint(&self) -> SizeHint {
        match self {
            ReqBody::Empty => SizeHint::with_exact(0),
            ReqBody::Once(bytes) => SizeHint::with_exact(bytes.len() as u64),
            ReqBody::Incoming(body) => body.size_hint(),
            ReqBody::LimitedIncoming(body) => body.size_hint(),
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
