mod handler_fn;
/// Handler module
mod handler_trait;
mod handler_wrapper;
#[cfg(feature = "static")]
mod r#static;

pub use handler_fn::HandlerFn;
pub use handler_trait::Handler;
pub use handler_wrapper::HandlerWrapper;
#[cfg(feature = "static")]
pub use r#static::{StaticOptions, static_handler, static_handler_with_options};
