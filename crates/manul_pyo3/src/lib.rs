use pyo3::prelude::*;

/// A Python module implemented in Rust.
#[pymodule]
mod _manul {

    #[pymodule_export]
    pub use manul_core::manul_core;

    #[pymodule_export]
    pub use manul_logger::manul_logger;
}
