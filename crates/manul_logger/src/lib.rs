use pyo3::prelude::*;

mod logger;

#[pymodule(name = "_logger")]
pub mod manul_logger {

    #[allow(non_upper_case_globals)]
    #[pymodule_export]
    pub const __version__: &str = env!("CARGO_PKG_VERSION");

    #[pymodule_export]
    pub use super::logger::{
        _log_sink, PyLayerConfig, PyLayerDestination, PyLogFormat, PyTracingGuard, debug, error,
        info, init_tracing, trace, warn,
    };
}
