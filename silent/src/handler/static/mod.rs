mod compression;
mod directory;
mod handler;
mod options;

pub use handler::{static_handler, static_handler_with_options};
pub use options::StaticOptions;
