use crate::log::error;
use crate::sse::SSEEvent;
use crate::{Result, SilentError, StatusCode};
use async_io::Timer;
use futures_util::{Stream, TryStream};
use pin_project::pin_project;
use std::borrow::Cow;
use std::error::Error as StdError;
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Duration;
use std::time::Instant;

/// Configure the interval between keep-alive messages, the content
/// of each message, and the associated stream.
#[derive(Debug)]
pub struct KeepAlive {
    comment_text: Cow<'static, str>,
    max_interval: Duration,
}

impl Default for KeepAlive {
    fn default() -> Self {
        Self {
            comment_text: Cow::Borrowed(""),
            max_interval: Duration::from_secs(15),
        }
    }
}

impl KeepAlive {
    pub fn new() -> Self {
        Self::default()
    }

    /// Customize the interval between keep-alive messages.
    ///
    /// Default is 15 seconds.
    pub fn interval(mut self, time: Duration) -> Self {
        self.max_interval = time;
        self
    }

    /// Customize the text of the keep-alive message.
    ///
    /// Default is an empty comment.
    pub fn comment_text(mut self, text: impl Into<Cow<'static, str>>) -> Self {
        self.comment_text = text.into();
        self
    }

    /// Wrap an event stream with keep-alive functionality.
    ///
    /// See [`keep_alive`](keep_alive) for more.
    pub fn stream<S>(
        self,
        event_stream: S,
    ) -> impl TryStream<Ok = SSEEvent, Error = impl StdError + Send + Sync + 'static> + Send + 'static
    where
        S: TryStream<Ok = SSEEvent> + Send + 'static,
        S::Error: StdError + Send + Sync + 'static,
    {
        let alive_timer = Timer::after(self.max_interval);
        SseKeepAlive {
            event_stream,
            comment_text: self.comment_text,
            max_interval: self.max_interval,
            alive_timer,
        }
    }
}

#[pin_project]
struct SseKeepAlive<S> {
    #[pin]
    event_stream: S,
    comment_text: Cow<'static, str>,
    max_interval: Duration,
    #[pin]
    alive_timer: Timer,
}

impl<S> Stream for SseKeepAlive<S>
where
    S: TryStream<Ok = SSEEvent> + Send + 'static,
    S::Error: StdError + Send + Sync + 'static,
{
    type Item = Result<SSEEvent>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let mut pin = self.project();
        match pin.event_stream.try_poll_next(cx) {
            Poll::Pending => match Pin::new(&mut pin.alive_timer).poll(cx) {
                Poll::Pending => Poll::Pending,
                Poll::Ready(_) => {
                    // restart timer
                    let next = Instant::now() + *pin.max_interval;
                    *pin.alive_timer = Timer::at(next);
                    let comment_str = pin.comment_text.clone();
                    let event = SSEEvent::default().comment(comment_str);
                    Poll::Ready(Some(Ok(event)))
                }
            },
            Poll::Ready(Some(Ok(event))) => {
                // restart timer
                let next = Instant::now() + *pin.max_interval;
                *pin.alive_timer = Timer::at(next);
                Poll::Ready(Some(Ok(event)))
            }
            Poll::Ready(None) => Poll::Ready(None),
            Poll::Ready(Some(Err(error))) => {
                error!("sse::keep error: {}", error);
                Poll::Ready(Some(Err(SilentError::BusinessError {
                    code: StatusCode::INTERNAL_SERVER_ERROR,
                    msg: "sse::keep error".to_string(),
                })))
            }
        }
    }
}
