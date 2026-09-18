pub mod funtext;
pub mod utils;

/// The version of this crate, exposed to consumers (e.g. Python bindings) that want to report it.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
