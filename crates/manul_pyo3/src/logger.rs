use manul_logger::logger::{
    LayerConfig, LayerDestination, LogFormat, init_tracing as core_init_tracing, log_sink,
};
use pyo3::exceptions::{PyRuntimeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::PyDict;
use std::str::FromStr;

#[pymodule(name = "_logger")]
pub mod logger_bindings {
    #[allow(non_upper_case_globals)]
    #[pymodule_export]
    pub const __version__: &str = ::manul_logger::VERSION;

    #[pymodule_export]
    pub use super::{
        _log_sink, PyLayerConfig, PyLayerDestination, PyLogFormat, PyTracingGuard, debug, error,
        info, init_tracing, trace, warn,
    };
}

/// Python-facing `LogFormat` enum. Mirrors `manul_logger::logger::LogFormat`.
#[pyclass(name = "LogFormat", from_py_object)]
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum PyLogFormat {
    #[default]
    Compact,
    Pretty,
    Json,
}

impl From<LogFormat> for PyLogFormat {
    fn from(value: LogFormat) -> Self {
        match value {
            LogFormat::Compact => PyLogFormat::Compact,
            LogFormat::Pretty => PyLogFormat::Pretty,
            LogFormat::Json => PyLogFormat::Json,
        }
    }
}

impl From<PyLogFormat> for LogFormat {
    fn from(value: PyLogFormat) -> Self {
        match value {
            PyLogFormat::Compact => LogFormat::Compact,
            PyLogFormat::Pretty => LogFormat::Pretty,
            PyLogFormat::Json => LogFormat::Json,
        }
    }
}

#[pymethods]
impl PyLogFormat {
    #[new]
    fn py_new(value: &str) -> PyResult<Self> {
        LogFormat::from_str(value)
            .map(PyLogFormat::from)
            .map_err(PyValueError::new_err)
    }

    fn __str__(&self) -> String {
        LogFormat::from(*self).to_str().to_string()
    }

    fn __repr__(&self) -> String {
        let s = self.__str__();
        format!("<LogFormat.{}: '{}'>", s, s)
    }
}

/// Python-facing `LayerDestination` enum. Mirrors `manul_logger::logger::LayerDestination`.
#[pyclass(name = "LayerDestination", from_py_object)]
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub enum PyLayerDestination {
    #[default]
    Console,
    File,
}

impl From<LayerDestination> for PyLayerDestination {
    fn from(value: LayerDestination) -> Self {
        match value {
            LayerDestination::Console => PyLayerDestination::Console,
            LayerDestination::File => PyLayerDestination::File,
        }
    }
}

impl From<PyLayerDestination> for LayerDestination {
    fn from(value: PyLayerDestination) -> Self {
        match value {
            PyLayerDestination::Console => LayerDestination::Console,
            PyLayerDestination::File => LayerDestination::File,
        }
    }
}

#[pymethods]
impl PyLayerDestination {
    #[new]
    fn py_new(value: &str) -> PyResult<Self> {
        LayerDestination::from_str(value)
            .map(PyLayerDestination::from)
            .map_err(PyValueError::new_err)
    }

    fn __str__(&self) -> String {
        LayerDestination::from(self.clone()).to_str().to_string()
    }

    fn __repr__(&self) -> String {
        format!("LayerDestination(\"{}\")", self.__str__())
    }
}

/// Python-facing layer configuration. Mirrors `manul_logger::logger::LayerConfig`.
#[pyclass(name = "LayerConfig", from_py_object)]
#[derive(Debug, Clone)]
pub struct PyLayerConfig {
    #[pyo3(get, set)]
    pub name: String,
    #[pyo3(get, set)]
    pub filter_directive: String,
    #[pyo3(get, set)]
    pub format: PyLogFormat,
    #[pyo3(get, set)]
    pub destination: PyLayerDestination,
    #[pyo3(get, set)]
    pub file_dir: Option<String>,
    #[pyo3(get, set)]
    pub file_prefix: Option<String>,
    #[pyo3(get, set)]
    pub include_span_events: bool,
}

impl From<&PyLayerConfig> for LayerConfig {
    fn from(value: &PyLayerConfig) -> Self {
        LayerConfig::new(
            value.name.clone(),
            value.filter_directive.clone(),
            LogFormat::from(value.format),
            LayerDestination::from(value.destination.clone()),
            value.file_dir.clone(),
            value.file_prefix.clone(),
            value.include_span_events,
        )
    }
}

#[pymethods]
impl PyLayerConfig {
    #[new]
    #[pyo3(signature = (name, filter_directive, format=PyLogFormat::Compact, destination=PyLayerDestination::Console, file_dir=None, file_prefix=None, include_span_events=false))]
    fn py_new(
        name: String,
        filter_directive: String,
        format: PyLogFormat,
        destination: PyLayerDestination,
        file_dir: Option<String>,
        file_prefix: Option<String>,
        include_span_events: bool,
    ) -> Self {
        Self {
            name,
            filter_directive,
            format,
            destination,
            file_dir,
            file_prefix,
            include_span_events,
        }
    }

    fn __repr__(&self) -> String {
        format!(
            "LayerConfig(name={}, filter_directive={}, format={}, destination={}, file_dir={}, file_prefix={}, include_span_events={})",
            self.name,
            self.filter_directive,
            self.format.__str__(),
            self.destination.__str__(),
            if let Some(dir) = &self.file_dir {
                dir.to_string()
            } else {
                "None".to_string()
            },
            if let Some(prefix) = &self.file_prefix {
                prefix.to_string()
            } else {
                "None".to_string()
            },
            self.include_span_events
        )
    }
}

/// A guard object that keeps background logging threads alive.
/// In Python, if this object is garbage collected, file logging will stop.
#[pyclass(name = "TracingGuard")]
pub struct PyTracingGuard {
    _guards: manul_logger::logger::TracingGuards,
}

#[pyfunction]
#[pyo3(signature = (layers))]
pub fn init_tracing(layers: Vec<PyLayerConfig>) -> PyResult<PyTracingGuard> {
    let core_layers: Vec<LayerConfig> = layers.iter().map(LayerConfig::from).collect();
    core_init_tracing(core_layers)
        .map(|guards| PyTracingGuard { _guards: guards })
        .map_err(|e| match e {
            manul_logger::logger::TracingInitError::LayerBuild(msg) => PyValueError::new_err(msg),
            manul_logger::logger::TracingInitError::RegistryInit(msg) => {
                PyRuntimeError::new_err(msg)
            }
        })
}

/// Converts a Python dictionary into a human-readable string: "key=val, key1=val1"
fn dict_to_string(extras: Bound<'_, PyDict>) -> String {
    let mut parts = Vec::new();
    for (key, value) in extras {
        parts.push(format!("{}={}", key, value));
    }
    parts.join(", ")
}

#[pyfunction(name = "info")]
#[pyo3(signature = (message, extra=None))]
pub fn info(message: &str, extra: Option<Bound<'_, PyDict>>) {
    let extra_str = extra.map(dict_to_string);
    log_sink(20, message, None, None, None, None, extra_str.as_deref());
}

#[pyfunction(name = "warn")]
#[pyo3(signature = (message, extra=None))]
pub fn warn(message: &str, extra: Option<Bound<'_, PyDict>>) {
    let extra_str = extra.map(dict_to_string);
    log_sink(30, message, None, None, None, None, extra_str.as_deref());
}

#[pyfunction(name = "error")]
#[pyo3(signature = (message, extra=None))]
pub fn error(message: &str, extra: Option<Bound<'_, PyDict>>) {
    let extra_str = extra.map(dict_to_string);
    log_sink(40, message, None, None, None, None, extra_str.as_deref());
}

#[pyfunction(name = "debug")]
#[pyo3(signature = (message, extra=None))]
pub fn debug(message: &str, extra: Option<Bound<'_, PyDict>>) {
    let extra_str = extra.map(dict_to_string);
    log_sink(10, message, None, None, None, None, extra_str.as_deref());
}

#[pyfunction(name = "trace")]
#[pyo3(signature = (message, extra=None))]
pub fn trace(message: &str, extra: Option<Bound<'_, PyDict>>) {
    let extra_str = extra.map(dict_to_string);
    log_sink(0, message, None, None, None, None, extra_str.as_deref());
}

#[pyfunction(name = "_log_sink")]
#[pyo3(signature = (levelno, message, filename=None, func_name=None, lineno=None, module_name=None, extra=None))]
pub fn _log_sink(
    levelno: u8,
    message: &str,
    filename: Option<String>,
    func_name: Option<String>,
    lineno: Option<usize>,
    module_name: Option<String>,
    extra: Option<Bound<'_, PyDict>>,
) {
    let extra_str = extra.map(dict_to_string);
    log_sink(
        levelno,
        message,
        filename,
        func_name,
        lineno,
        module_name,
        extra_str.as_deref(),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use pyo3::Python;

    #[test]
    fn test_py_log_format_conversions_and_repr() {
        Python::initialize();
        let value = PyLogFormat::py_new("json").unwrap();
        assert_eq!(value, PyLogFormat::Json);
        assert_eq!(value.__str__(), "json");
        assert_eq!(value.__repr__(), "<LogFormat.json: 'json'>");
        assert!(PyLogFormat::py_new("bogus").is_err());
    }

    #[test]
    fn test_py_layer_destination_conversions_and_repr() {
        Python::initialize();
        let value = PyLayerDestination::py_new("file").unwrap();
        assert_eq!(value, PyLayerDestination::File);
        assert_eq!(value.__str__(), "file");
        assert_eq!(value.__repr__(), "LayerDestination(\"file\")");
    }

    #[test]
    fn test_py_layer_config_repr_and_conversion() {
        let config = PyLayerConfig::py_new(
            "test_layer".to_string(),
            "info".to_string(),
            PyLogFormat::Compact,
            PyLayerDestination::Console,
            None,
            None,
            false,
        );
        assert_eq!(
            config.__repr__(),
            "LayerConfig(name=test_layer, filter_directive=info, format=compact, destination=console, file_dir=None, file_prefix=None, include_span_events=false)"
        );

        let core_config = LayerConfig::from(&config);
        assert_eq!(core_config.name, "test_layer");
        assert_eq!(core_config.format, LogFormat::Compact);
    }

    #[test]
    fn test_dict_to_string() {
        Python::initialize();
        Python::attach(|py| {
            let dict = PyDict::new(py);
            dict.set_item("key1", "value1").unwrap();
            let result = dict_to_string(dict);
            assert_eq!(result, "key1=value1");
        });
    }

    #[test]
    fn test_level_wrappers_smoke() {
        Python::initialize();
        Python::attach(|py| {
            let extra = PyDict::new(py);
            extra.set_item("k", "v").unwrap();
            info("hello", Some(extra.clone()));
            warn("hello", None);
            debug("hello", None);
            error("hello", None);
            trace("hello", None);
            _log_sink(20, "hello", None, None, None, None, Some(extra));
        });
    }
}
