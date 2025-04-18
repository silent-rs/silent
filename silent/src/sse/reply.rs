use crate::header::{CACHE_CONTROL, CONTENT_TYPE};
use crate::prelude::stream_body;
use crate::sse::{KeepAlive, SSEEvent};
use crate::{Response, Result, SilentError, StatusCode, headers::HeaderValue, log};
use futures_util::{Stream, TryStreamExt, future};

pub fn sse_reply<S>(stream: S) -> Result<Response>
where
    S: Stream<Item = Result<SSEEvent>> + Send + 'static,
{
    let event_stream = KeepAlive::default().stream(stream);
    let body_stream = event_stream
        .map_err(|error| {
            log::error!("sse stream error: {}", error.to_string());
            SilentError::BusinessError {
                code: StatusCode::INTERNAL_SERVER_ERROR,
                msg: "sse::keep error".to_string(),
            }
        })
        .into_stream()
        .and_then(|event| future::ready(Ok(event.to_string())));

    let mut res = Response::empty();
    res.set_body(stream_body(body_stream));
    // Set appropriate content type
    res.headers_mut()
        .insert(CONTENT_TYPE, HeaderValue::from_static("text/event-stream"));
    // Disable response body caching
    res.headers_mut()
        .insert(CACHE_CONTROL, HeaderValue::from_static("no-cache"));
    Ok(res)
}
