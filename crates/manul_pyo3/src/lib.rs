use pyo3::prelude::*;

/// A Python module implemented in Rust.
#[pymodule]
mod _manul {
    use ::manul_core::funtext;
    use pyo3::prelude::*;

    #[pymodule_export]
    pub use manul_core::manul_core;

    #[pymodule_export]
    pub use manul_logger::manul_logger;

    #[pymodule_init]
    fn init(m: &Bound<'_, PyModule>) -> PyResult<()> {
        let (logo_text, short_logo_text) = funtext::build_logo(vec![
            ("manul_core", manul_core::__version__),
            ("manul_logger", manul_logger::__version__),
        ]);
        m.add("__logo__", short_logo_text)?;
        m.add("__detailed_logo__", logo_text)?;
        Ok(())
    }
}
