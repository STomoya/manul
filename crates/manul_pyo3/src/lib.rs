use pyo3::prelude::*;

mod core;

/// A Python module implemented in Rust.
#[pymodule]
mod _manul {
    use pyo3::prelude::*;

    #[pymodule_export]
    pub use crate::core::core_bindings;

    #[pymodule_export]
    pub use manul_logger::manul_logger;

    #[pymodule_init]
    fn init(m: &Bound<'_, PyModule>) -> PyResult<()> {
        let (logo_text, short_logo_text) = manul_core::funtext::build_logo(vec![
            ("manul_core", manul_core::VERSION),
            ("manul_logger", manul_logger::__version__),
        ]);
        m.add("__logo__", short_logo_text)?;
        m.add("__detailed_logo__", logo_text)?;
        Ok(())
    }
}
