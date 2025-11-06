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
    /// data field(s) ("data:&lt;content&gt;")
    pub fn data<T: Into<String>>(mut self, data: T) -> SSEEvent {
        self.data = Some(DataType::Text(data.into()));
        self
    }

    /// Set Server-sent event data
    /// data field(s) ("data:&lt;content&gt;")
    pub fn json_data<T: Serialize>(mut self, data: T) -> Result<SSEEvent, Error> {
        self.data = Some(DataType::Json(serde_json::to_string(&data)?));
        Ok(self)
    }

    /// Set Server-sent event comment
    /// Comment field (":&lt;comment-text&gt;")
    pub fn comment<T: Into<String>>(mut self, comment: T) -> SSEEvent {
        self.comment = Some(comment.into());
        self
    }

    /// Set Server-sent events
    /// SSEEvent name field ("event:&lt;event-name&gt;")
    pub fn event<T: Into<String>>(mut self, event: T) -> SSEEvent {
        self.event = Some(event.into());
        self
    }

    /// Set Server-sent event retry duration
    /// Retry timeout field ("retry:&lt;timeout&gt;")
    pub fn retry(mut self, duration: Duration) -> SSEEvent {
        self.retry = Some(duration);
        self
    }

    /// Set Server-sent event id
    /// Identifier field ("id:&lt;identifier&gt;")
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
