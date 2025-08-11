mod configs;
#[cfg(feature = "cookie")]
mod cookie;
/// The `silent` library.
mod core;
mod error;
mod extractor;
#[cfg(feature = "grpc")]
mod grpc;
mod handler;
mod log;
pub mod middleware;
pub mod prelude;
mod route;
#[cfg(feature = "scheduler")]
mod scheduler;
#[cfg(feature = "security")]
mod security;
#[cfg(feature = "server")]
mod service;
#[cfg(feature = "session")]
mod session;
#[cfg(feature = "sse")]
mod sse;
#[cfg(feature = "template")]
mod templates;
#[cfg(feature = "upgrade")]
mod ws;

// use silent_multer as multer;
#[cfg(feature = "multipart")]
#[allow(unused_imports)]
#[allow(clippy::single_component_path_imports)]
use multer;

pub use crate::configs::Configs;
#[cfg(feature = "cookie")]
pub use crate::cookie::cookie_ext::CookieExt;
pub use crate::core::{next::Next, request::Request, response::Response};
#[cfg(feature = "grpc")]
pub use crate::grpc::{GrpcHandler, GrpcRegister};
pub use crate::middleware::{MiddleWareHandler, middlewares};
pub use error::SilentError;
pub use error::SilentResult as Result;
pub use extractor::{
    Form as ExtractForm, FromRequest, Json as ExtractJson, Path as ExtractPath,
    Query as ExtractQuery,
};
pub use handler::Handler;
pub use handler::HandlerWrapper;
pub use headers;
pub use hyper::{Method, StatusCode, header};
#[cfg(feature = "scheduler")]
pub use scheduler::{ProcessTime, SCHEDULER, Scheduler, SchedulerExt, Task};
