mod configs;
#[cfg(feature = "cookie")]
mod cookie;
/// The `silent` library.
mod core;
mod error;
pub mod extractor;
#[cfg(feature = "grpc")]
mod grpc;
mod handler;
mod log;
pub mod middleware;
pub mod prelude;
#[cfg(feature = "server")]
pub use crate::server::protocol;
mod route;
#[cfg(feature = "scheduler")]
mod scheduler;
#[cfg(feature = "security")]
mod security;
#[cfg(feature = "server")]
mod server;
#[cfg(feature = "session")]
mod session;
#[cfg(feature = "sse")]
mod sse;
#[cfg(feature = "template")]
mod templates;
#[cfg(feature = "upgrade")]
pub mod ws;

// use silent_multer as multer;
#[cfg(feature = "multipart")]
#[allow(unused_imports)]
#[allow(clippy::single_component_path_imports)]
use multer;

pub use crate::configs::Configs;
#[cfg(feature = "cookie")]
pub use crate::cookie::cookie_ext::CookieExt;
#[cfg(feature = "server")]
pub use crate::core::socket_addr::SocketAddr;
pub use crate::core::{next::Next, request::Request, response::Response};
#[cfg(feature = "grpc")]
pub use crate::grpc::{GrpcHandler, GrpcRegister};
pub use crate::middleware::{MiddleWareHandler, middlewares};
#[cfg(feature = "server")]
pub use crate::server::RouteConnectionService;
#[cfg(feature = "server")]
pub use crate::server::connection::{BoxedConnection, Connection};
#[cfg(feature = "server")]
pub use crate::server::listener::{AcceptFuture, Listen, Listener, Listeners, ListenersBuilder};
#[cfg(feature = "server")]
pub use crate::server::net_server::{NetServer, RateLimiterConfig};
#[cfg(feature = "server")]
pub use crate::server::protocol::Protocol;
#[cfg(feature = "quic")]
pub use crate::server::quic;
#[cfg(feature = "quic")]
pub use crate::server::quic::{HybridListener, QuicEndpointListener};
#[cfg(all(feature = "server", feature = "tls"))]
pub use crate::server::tls::ReloadableCertificateStore;
#[cfg(feature = "server")]
pub use crate::server::{BoxError, ConnectionFuture, ConnectionService, Server};
#[cfg(all(feature = "server", feature = "tls"))]
pub use crate::server::{CertificateStore, CertificateStoreBuilder};
#[cfg(feature = "server")]
pub use crate::server::{ConnectionLimits, ServerConfig};
pub use error::SilentError;
pub use error::SilentResult as Result;
pub use handler::Handler;
pub use handler::HandlerWrapper;
pub use headers;
pub use hyper::{Method, StatusCode, header};
#[cfg(feature = "scheduler")]
pub use scheduler::{ProcessTime, SCHEDULER, Scheduler, SchedulerExt, Task};
