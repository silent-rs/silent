use serde::Serialize;
use serde_json::{self, Error};
use std::fmt;
use std::fmt::Write;
use std::time::Duration;

// Server-sent event data type
#[derive(Debug)]
enum DataType {
    Text(String),
    Json(String),
}

/// Server-sent event
#[derive(Default, Debug)]
pub struct SSEEvent {
    id: Option<String>,
    data: Option<DataType>,
    event: Option<String>,
    comment: Option<String>,
    retry: Option<Duration>,
}

impl SSEEvent {
    /// Set Server-sent event data
    /// data field(s) ("data:<content>")
    pub fn data<T: Into<String>>(mut self, data: T) -> SSEEvent {
        self.data = Some(DataType::Text(data.into()));
        self
    }

    /// Set Server-sent event data
    /// data field(s) ("data:<content>")
    pub fn json_data<T: Serialize>(mut self, data: T) -> Result<SSEEvent, Error> {
        self.data = Some(DataType::Json(serde_json::to_string(&data)?));
        Ok(self)
    }

    /// Set Server-sent event comment
    /// Comment field (":<comment-text>")
    pub fn comment<T: Into<String>>(mut self, comment: T) -> SSEEvent {
        self.comment = Some(comment.into());
        self
    }

    /// Set Server-sent events
    /// SSEEvent name field ("event:<event-name>")
    pub fn event<T: Into<String>>(mut self, event: T) -> SSEEvent {
        self.event = Some(event.into());
        self
    }

    /// Set Server-sent event retry duration
    /// Retry timeout field ("retry:<timeout>")
    pub fn retry(mut self, duration: Duration) -> SSEEvent {
        self.retry = Some(duration);
        self
    }

    /// Set Server-sent event id
    /// Identifier field ("id:<identifier>")
    pub fn id<T: Into<String>>(mut self, id: T) -> SSEEvent {
        self.id = Some(id.into());
        self
    }
}

impl fmt::Display for SSEEvent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(comment) = &self.comment {
            ":".fmt(f)?;
            comment.fmt(f)?;
            f.write_char('\n')?;
        }

        if let Some(event) = &self.event {
            "event:".fmt(f)?;
            event.fmt(f)?;
            f.write_char('\n')?;
        }

        match self.data {
            Some(DataType::Text(ref data)) => {
                for line in data.split('\n') {
                    "data:".fmt(f)?;
                    line.fmt(f)?;
                    f.write_char('\n')?;
                }
            }
            Some(DataType::Json(ref data)) => {
                "data:".fmt(f)?;
                data.fmt(f)?;
                f.write_char('\n')?;
            }
            None => {}
        }

        if let Some(id) = &self.id {
            "id:".fmt(f)?;
            id.fmt(f)?;
            f.write_char('\n')?;
        }

        if let Some(duration) = &self.retry {
            "retry:".fmt(f)?;

            let secs = duration.as_secs();
            let millis = duration.subsec_millis();

            if secs > 0 {
                // format seconds
                secs.fmt(f)?;

                // pad milliseconds
                if millis < 10 {
                    f.write_str("00")?;
                } else if millis < 100 {
                    f.write_char('0')?;
                }
            }

            // format milliseconds
            millis.fmt(f)?;

            f.write_char('\n')?;
        }

        f.write_char('\n')?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Serialize;

    // ==================== Default trait 测试 ====================

    #[test]
    fn test_sse_event_default() {
        let event = SSEEvent::default();
        assert!(event.id.is_none());
        assert!(event.data.is_none());
        assert!(event.event.is_none());
        assert!(event.comment.is_none());
        assert!(event.retry.is_none());
    }

    // ==================== data() 方法测试 ====================

    #[test]
    fn test_sse_event_data_string() {
        let event = SSEEvent::default().data("hello world");
        assert!(matches!(event.data, Some(DataType::Text(_))));
    }

    #[test]
    fn test_sse_event_data_str() {
        let event = SSEEvent::default().data("test");
        assert!(matches!(event.data, Some(DataType::Text(_))));
    }

    #[test]
    fn test_sse_event_data_empty() {
        let event = SSEEvent::default().data("");
        assert!(matches!(event.data, Some(DataType::Text(_))));
    }

    #[test]
    fn test_sse_event_data_unicode() {
        let event = SSEEvent::default().data("你好世界 🌍");
        assert!(matches!(
            event,
            SSEEvent {
                data: Some(DataType::Text(_)),
                ..
            }
        ));
    }

    // ==================== json_data() 方法测试 ====================

    #[test]
    fn test_sse_event_json_data_success() {
        #[derive(Serialize)]
        struct TestData {
            field: &'static str,
        }

        let data = TestData { field: "value" };
        let event = SSEEvent::default().json_data(&data);
        assert!(event.is_ok());
        assert!(matches!(event.unwrap().data, Some(DataType::Json(_))));
    }

    #[test]
    fn test_sse_event_json_data_complex() {
        #[derive(Serialize)]
        struct ComplexData {
            name: &'static str,
            count: i32,
            items: Vec<&'static str>,
        }

        let data = ComplexData {
            name: "test",
            count: 42,
            items: vec!["a", "b", "c"],
        };
        let event = SSEEvent::default().json_data(&data);
        assert!(event.is_ok());
    }

    // ==================== comment() 方法测试 ====================

    #[test]
    fn test_sse_event_comment() {
        let event = SSEEvent::default().comment("test comment");
        assert_eq!(event.comment, Some("test comment".to_string()));
    }

    #[test]
    fn test_sse_event_comment_empty() {
        let event = SSEEvent::default().comment("");
        assert_eq!(event.comment, Some("".to_string()));
    }

    #[test]
    fn test_sse_event_comment_unicode() {
        let event = SSEEvent::default().comment("注释测试");
        assert_eq!(event.comment, Some("注释测试".to_string()));
    }

    // ==================== event() 方法测试 ====================

    #[test]
    fn test_sse_event_event() {
        let event = SSEEvent::default().event("message");
        assert_eq!(event.event, Some("message".to_string()));
    }

    #[test]
    fn test_sse_event_event_empty() {
        let event = SSEEvent::default().event("");
        assert_eq!(event.event, Some("".to_string()));
    }

    // ==================== retry() 方法测试 ====================

    #[test]
    fn test_sse_event_retry_secs() {
        let event = SSEEvent::default().retry(std::time::Duration::from_secs(5));
        assert_eq!(event.retry, Some(std::time::Duration::from_secs(5)));
    }

    #[test]
    fn test_sse_event_retry_millis() {
        let event = SSEEvent::default().retry(std::time::Duration::from_millis(500));
        assert_eq!(event.retry, Some(std::time::Duration::from_millis(500)));
    }

    #[test]
    fn test_sse_event_retry_zero() {
        let event = SSEEvent::default().retry(std::time::Duration::ZERO);
        assert_eq!(event.retry, Some(std::time::Duration::ZERO));
    }

    // ==================== id() 方法测试 ====================

    #[test]
    fn test_sse_event_id_string() {
        let event = SSEEvent::default().id("123");
        assert_eq!(event.id, Some("123".to_string()));
    }

    #[test]
    fn test_sse_event_id_number() {
        let event = SSEEvent::default().id("456");
        assert_eq!(event.id, Some("456".to_string()));
    }

    #[test]
    fn test_sse_event_id_empty() {
        let event = SSEEvent::default().id("");
        assert_eq!(event.id, Some("".to_string()));
    }

    // ==================== 链式调用测试 ====================

    #[test]
    fn test_sse_event_chain_full() {
        let event = SSEEvent::default()
            .id("123")
            .event("message")
            .data("test data")
            .comment("keep-alive")
            .retry(std::time::Duration::from_secs(10));

        assert_eq!(event.id, Some("123".to_string()));
        assert_eq!(event.event, Some("message".to_string()));
        assert!(matches!(event.data, Some(DataType::Text(_))));
        assert_eq!(event.comment, Some("keep-alive".to_string()));
        assert_eq!(event.retry, Some(std::time::Duration::from_secs(10)));
    }

    #[test]
    fn test_sse_event_chain_multiple_data() {
        let event = SSEEvent::default().data("first line").data("second line");

        assert!(matches!(event.data, Some(DataType::Text(_))));
    }

    // ==================== Display 格式化测试 ====================

    #[test]
    fn test_sse_event_display_empty() {
        let event = SSEEvent::default();
        let formatted = format!("{}", event);
        assert_eq!(formatted, "\n");
    }

    #[test]
    fn test_sse_event_display_with_data() {
        let event = SSEEvent::default().data("test message");
        let formatted = format!("{}", event);
        assert!(formatted.contains("data:test message\n"));
        assert!(formatted.ends_with('\n'));
    }

    #[test]
    fn test_sse_event_display_multiline_data() {
        let event = SSEEvent::default().data("line1\nline2");
        let formatted = format!("{}", event);
        assert!(formatted.contains("data:line1\ndata:line2\n"));
    }

    #[test]
    fn test_sse_event_display_with_event() {
        let event = SSEEvent::default().event("chat").data("hello");
        let formatted = format!("{}", event);
        assert!(formatted.contains("event:chat\n"));
        assert!(formatted.contains("data:hello\n"));
    }

    #[test]
    fn test_sse_event_display_with_id() {
        let event = SSEEvent::default().id("123").data("test");
        let formatted = format!("{}", event);
        assert!(formatted.contains("id:123\n"));
        assert!(formatted.contains("data:test\n"));
    }

    #[test]
    fn test_sse_event_display_with_retry() {
        let event = SSEEvent::default()
            .retry(std::time::Duration::from_millis(1500))
            .data("test");

        let formatted = format!("{}", event);
        assert!(formatted.contains("retry:1500\n"));
        assert!(formatted.contains("data:test\n"));
    }

    #[test]
    fn test_sse_event_display_with_comment() {
        let event = SSEEvent::default().comment("keep-alive").data("test");
        let formatted = format!("{}", event);
        assert!(formatted.contains(":keep-alive\n"));
        assert!(formatted.contains("data:test\n"));
    }

    #[test]
    fn test_sse_event_display_complete() {
        let event = SSEEvent::default()
            .id("123")
            .event("message")
            .data("hello world")
            .retry(std::time::Duration::from_secs(5))
            .comment("keep-alive");

        let formatted = format!("{}", event);
        assert!(formatted.contains(":keep-alive\n"));
        assert!(formatted.contains("event:message\n"));
        assert!(formatted.contains("data:hello world\n"));
        assert!(formatted.contains("id:123\n"));
        assert!(formatted.contains("retry:5000\n"));
        assert!(formatted.ends_with('\n'));
    }

    #[test]
    fn test_sse_event_display_json_data() {
        #[derive(Serialize)]
        struct JsonData {
            value: i32,
        }

        let data = JsonData { value: 42 };
        let event = SSEEvent::default().json_data(&data).unwrap();
        let formatted = format!("{}", event);
        assert!(formatted.contains("data:{\"value\":42}\n"));
    }

    #[test]
    fn test_sse_event_display_retry_padding() {
        // Test millisecond padding when seconds > 0
        let event = SSEEvent::default().retry(std::time::Duration::from_millis(1050));

        let formatted = format!("{}", event);
        assert!(formatted.contains("retry:1050\n"));
    }

    #[test]
    fn test_sse_event_display_retry_no_padding() {
        // Test that milliseconds are not padded when seconds == 0
        let event = SSEEvent::default().retry(std::time::Duration::from_millis(50));

        let formatted = format!("{}", event);
        assert!(formatted.contains("retry:50\n"));
    }

    // ==================== 字段覆盖测试 ====================

    #[test]
    fn test_sse_event_override_data() {
        let event = SSEEvent::default().data("first").data("second");

        // 最后一次调用生效
        assert!(matches!(event.data, Some(DataType::Text(t)) if t == "second"));
    }

    #[test]
    fn test_sse_event_override_retry() {
        let event = SSEEvent::default()
            .retry(std::time::Duration::from_secs(1))
            .retry(std::time::Duration::from_secs(2));

        assert_eq!(event.retry, Some(std::time::Duration::from_secs(2)));
    }

    // ==================== DataType 枚举测试 ====================

    #[test]
    fn test_sse_event_data_text() {
        let event = SSEEvent::default().data("text");
        assert!(matches!(event.data, Some(DataType::Text(_))));
    }

    #[test]
    fn test_sse_event_data_json() {
        #[derive(Serialize)]
        struct Data {
            field: &'static str,
        }
        let event = SSEEvent::default()
            .json_data(&Data { field: "value" })
            .unwrap();
        assert!(matches!(event.data, Some(DataType::Json(_))));
    }
}
