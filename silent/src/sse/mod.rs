//! Server-Sent Events (SSE)
//!
//! # Example
//!
//! ```
//!
//! use std::time::Duration;
//! use std::convert::Infallible;
//! use silent::{Method, prelude::Route, prelude::HandlerAppend, Request, Response};
//! use futures_util::{stream::iter, Stream};
//! use silent::prelude::{HandlerGetter, sse_reply, SSEEvent, Result};
//!
//! fn sse_events() -> impl Stream<Item = Result<SSEEvent>> + Send + 'static {
//!     iter(vec![
//!         Ok(SSEEvent::default().data("unnamed event")),
//!         Ok(
//!             SSEEvent::default().event("chat")
//!             .data("chat message")
//!         ),
//!         Ok(
//!             SSEEvent::default().id(13.to_string())
//!             .event("chat")
//!             .data("other chat message\nwith next line")
//!             .retry(Duration::from_millis(5000))
//!         ),
//!     ])
//! }
//!
//! let route = Route::new("push-notifications")
//!     .get(|req: Request| async {
//!         sse_reply(sse_events())
//!     });
//! ```
//!
//! Each field already is event which can be sent to client.
//! The events with multiple fields can be created by combining fields using tuples.
//!
//! See also the [EventSource](https://developer.mozilla.org/en-US/docs/Web/API/EventSource) API,
//! which specifies the expected behavior of Server Sent Events.
//!

mod event;
mod keep_alive;
mod reply;

pub use event::SSEEvent;
pub use keep_alive::KeepAlive;
pub use reply::sse_reply;
