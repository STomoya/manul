use pyo3::prelude::*;

pub mod funtext;
mod utils;

#[pymodule(name = "_core")]
pub mod manul_core {

    #[allow(non_upper_case_globals)]
    #[pymodule_export]
    pub const __version__: &str = env!("CARGO_PKG_VERSION");

    #[pymodule_export]
    pub use super::utils::{PyPathType, PySortStrategy, find_paths};
}
