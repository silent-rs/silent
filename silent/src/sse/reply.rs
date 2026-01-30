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

#[cfg(test)]
mod tests {
    use super::*;
    use futures_util::stream;

    // ==================== 基本功能测试 ====================

    #[test]
    fn test_sse_reply_basic() {
        // 创建一个简单的 SSE 事件流
        let event = SSEEvent::default().data("test message");
        let stream = stream::iter(vec![Ok(event)]);

        let result = sse_reply(stream);
        assert!(result.is_ok());
    }

    // ==================== 响应头测试 ====================

    #[test]
    fn test_sse_reply_headers() {
        let event = SSEEvent::default().data("test");
        let stream = stream::iter(vec![Ok(event)]);

        let result = sse_reply(stream);
        assert!(result.is_ok());

        let response = result.unwrap();
        let headers = response.headers();

        // 验证 Content-Type
        assert_eq!(
            headers.get(CONTENT_TYPE),
            Some(&HeaderValue::from_static("text/event-stream"))
        );

        // 验证 Cache-Control
        assert_eq!(
            headers.get(CACHE_CONTROL),
            Some(&HeaderValue::from_static("no-cache"))
        );
    }

    // ==================== 空流测试 ====================

    #[test]
    fn test_sse_reply_empty_stream() {
        let stream: Vec<Result<SSEEvent>> = vec![];
        let event_stream = stream::iter(stream);

        let result = sse_reply(event_stream);
        assert!(result.is_ok());
    }

    // ==================== 多事件流测试 ====================

    #[test]
    fn test_sse_reply_multiple_events() {
        let events = vec![
            Ok(SSEEvent::default().data("message 1")),
            Ok(SSEEvent::default().data("message 2")),
            Ok(SSEEvent::default().data("message 3")),
        ];

        let stream = stream::iter(events);
        let result = sse_reply(stream);
        assert!(result.is_ok());
    }

    // ==================== 事件类型测试 ====================

    #[test]
    fn test_sse_reply_with_event_type() {
        let event = SSEEvent::default().event("chat").id("123").data("hello");

        let stream = stream::iter(vec![Ok(event)]);
        let result = sse_reply(stream);
        assert!(result.is_ok());

        let response = result.unwrap();
        assert_eq!(
            response.headers().get(CONTENT_TYPE),
            Some(&HeaderValue::from_static("text/event-stream"))
        );
    }

    // ==================== JSON 数据测试 ====================

    #[test]
    fn test_sse_reply_with_json() {
        #[derive(serde::Serialize)]
        struct TestData {
            message: &'static str,
        }

        let data = TestData { message: "test" };
        let event = SSEEvent::default().json_data(&data).unwrap();
        let stream = stream::iter(vec![Ok(event)]);

        let result = sse_reply(stream);
        assert!(result.is_ok());
    }

    // ==================== 带重试的测试 ====================

    #[test]
    fn test_sse_reply_with_retry() {
        let event = SSEEvent::default()
            .data("test")
            .retry(std::time::Duration::from_secs(5));

        let stream = stream::iter(vec![Ok(event)]);
        let result = sse_reply(stream);
        assert!(result.is_ok());
    }

    // ==================== 带注释的测试 ====================

    #[test]
    fn test_sse_reply_with_comment() {
        let event = SSEEvent::default().data("test").comment("keep-alive");

        let stream = stream::iter(vec![Ok(event)]);
        let result = sse_reply(stream);
        assert!(result.is_ok());
    }

    // ==================== Unicode 数据测试 ====================

    #[test]
    fn test_sse_reply_unicode() {
        let event = SSEEvent::default().data("你好世界 🌍");
        let stream = stream::iter(vec![Ok(event)]);

        let result = sse_reply(stream);
        assert!(result.is_ok());
    }

    // ==================== 多行数据测试 ====================

    #[test]
    fn test_sse_reply_multiline() {
        let event = SSEEvent::default().data("line1\nline2\nline3");
        let stream = stream::iter(vec![Ok(event)]);

        let result = sse_reply(stream);
        assert!(result.is_ok());
    }
}
