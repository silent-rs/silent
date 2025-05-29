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
        }
    }

    fn is_end_stream(&self) -> bool {
        match self {
            ReqBody::Empty => true,
            ReqBody::Once(bytes) => bytes.is_empty(),
            ReqBody::Incoming(body) => body.is_end_stream(),
        }
    }

    fn size_hint(&self) -> SizeHint {
        match self {
            ReqBody::Empty => SizeHint::with_exact(0),
            ReqBody::Once(bytes) => SizeHint::with_exact(bytes.len() as u64),
            ReqBody::Incoming(body) => body.size_hint(),
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
