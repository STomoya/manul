use pyo3::prelude::*;

#[pymodule(name = "_core")]
pub mod manul_core {

    #[allow(non_upper_case_globals)]
    #[pymodule_export]
    pub const __version__: &str = env!("CARGO_PKG_VERSION");
}
