use manul_logger::logger::{
    LayerConfig, LayerDestination, LogFormat, init_tracing as core_init_tracing, log_sink,
    set_filter as core_set_filter,
};
use pyo3::exceptions::{PyRuntimeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList};
use std::str::FromStr;

#[pymodule(name = "_logger")]
pub mod logger_bindings {
    #[allow(non_upper_case_globals)]
    #[pymodule_export]
    pub const __version__: &str = ::manul_logger::VERSION;

    #[pymodule_export]
    pub use super::{
        _log_sink, PyLayerConfig, PyLayerDestination, PyLogFormat, PyTracingGuard, init_tracing,
        set_filter,
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
    #[pyo3(get, set)]
    pub max_log_files: Option<usize>,
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
            value.max_log_files,
        )
    }
}

#[pymethods]
impl PyLayerConfig {
    #[new]
    #[pyo3(signature = (name, filter_directive, format=PyLogFormat::Compact, destination=PyLayerDestination::Console, file_dir=None, file_prefix=None, include_span_events=false, max_log_files=None))]
    #[allow(clippy::too_many_arguments)]
    fn py_new(
        name: String,
        filter_directive: String,
        format: PyLogFormat,
        destination: PyLayerDestination,
        file_dir: Option<String>,
        file_prefix: Option<String>,
        include_span_events: bool,
        max_log_files: Option<usize>,
    ) -> Self {
        Self {
            name,
            filter_directive,
            format,
            destination,
            file_dir,
            file_prefix,
            include_span_events,
            max_log_files,
        }
    }

    fn __repr__(&self) -> String {
        format!(
            "LayerConfig(name={}, filter_directive={}, format={}, destination={}, file_dir={}, file_prefix={}, include_span_events={}, max_log_files={})",
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
            self.include_span_events,
            if let Some(n) = self.max_log_files {
                n.to_string()
            } else {
                "None".to_string()
            }
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
        .map_err(|e| PyRuntimeError::new_err(e.to_string()))
}

/// Change a layer's filter directive at runtime, without restarting the process.
///
/// `layer_name` must match a `name` given to one of the `LayerConfig`s passed to
/// `init_tracing`.
#[pyfunction]
#[pyo3(signature = (layer_name, filter_directive))]
pub fn set_filter(layer_name: &str, filter_directive: &str) -> PyResult<()> {
    core_set_filter(layer_name, filter_directive)
        .map_err(|e| PyRuntimeError::new_err(e.to_string()))
}

/// Converts a Python dictionary into a human-readable string: "key=val, key1=val1"
fn dict_to_string(extras: &Bound<'_, PyDict>) -> String {
    let mut parts = Vec::new();
    for (key, value) in extras.iter() {
        parts.push(format!("{}={}", key, value));
    }
    parts.join(", ")
}

/// Converts a Python dictionary into a `serde_json::Value`, preserving each value's
/// native type instead of stringifying it, for `LayerConfig`'s JSON destination.
fn dict_to_json(extras: &Bound<'_, PyDict>) -> serde_json::Value {
    let mut map = serde_json::Map::new();
    for (key, value) in extras.iter() {
        map.insert(key.to_string(), py_to_json(&value));
    }
    serde_json::Value::Object(map)
}

/// Converts a Python list of `{"name": str, "fields": dict}` span-frame dicts (as
/// built by `manul.logger._functions._current_spans_payload`) into a human-readable
/// "name{k=v, ...} > name2{...}" string, for Compact/Pretty formats.
fn spans_to_string(spans: &Bound<'_, PyList>) -> String {
    spans
        .iter()
        .map(|frame| {
            let frame = frame
                .cast::<PyDict>()
                .expect("span frame must be a dict, see _current_spans_payload");
            let name: String = frame
                .get_item("name")
                .expect("span frame must have a 'name' key")
                .expect("span frame must have a 'name' key")
                .extract()
                .expect("span frame 'name' must be a str");
            let fields = frame
                .get_item("fields")
                .expect("span frame must have a 'fields' key")
                .and_then(|f| f.cast_into::<PyDict>().ok());
            match fields {
                Some(fields) if !fields.is_empty() => {
                    format!("{}{{{}}}", name, dict_to_string(&fields))
                }
                _ => name,
            }
        })
        .collect::<Vec<_>>()
        .join(" > ")
}

/// Converts a single Python value into a `serde_json::Value`, recursing into lists and
/// dicts. Anything not natively JSON-representable falls back to its `str()` form.
fn py_to_json(value: &Bound<'_, PyAny>) -> serde_json::Value {
    if let Ok(v) = value.extract::<bool>() {
        serde_json::Value::Bool(v)
    } else if let Ok(v) = value.extract::<i64>() {
        serde_json::Value::from(v)
    } else if let Ok(v) = value.extract::<f64>() {
        serde_json::Value::from(v)
    } else if let Ok(v) = value.extract::<String>() {
        serde_json::Value::from(v)
    } else if value.is_none() {
        serde_json::Value::Null
    } else if let Ok(list) = value.cast::<PyList>() {
        serde_json::Value::Array(list.iter().map(|item| py_to_json(&item)).collect())
    } else if let Ok(dict) = value.cast::<PyDict>() {
        dict_to_json(dict)
    } else {
        serde_json::Value::from(value.to_string())
    }
}

#[pyfunction(name = "_log_sink")]
#[pyo3(signature = (levelno, message, filename=None, func_name=None, lineno=None, module_name=None, extra=None, spans=None, exception=None))]
#[allow(clippy::too_many_arguments)]
pub fn _log_sink(
    levelno: u8,
    message: &str,
    filename: Option<String>,
    func_name: Option<String>,
    lineno: Option<usize>,
    module_name: Option<String>,
    extra: Option<Bound<'_, PyDict>>,
    spans: Option<Bound<'_, PyList>>,
    exception: Option<Bound<'_, PyDict>>,
) {
    let extra_str = extra.as_ref().map(dict_to_string);
    let attributes = extra.as_ref().map(dict_to_json);
    let spans_str = spans.as_ref().map(spans_to_string);
    let spans_json = spans.as_ref().map(|s| py_to_json(s.as_any()));
    let exception_json = exception.as_ref().map(dict_to_json);
    log_sink(
        levelno,
        message,
        filename,
        func_name,
        lineno,
        module_name,
        extra_str.as_deref(),
        attributes,
        spans_str.as_deref(),
        spans_json,
        exception_json,
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

        let compact = PyLogFormat::py_new("compact").unwrap();
        assert_eq!(compact, PyLogFormat::Compact);
        assert_eq!(compact.__str__(), "compact");

        let pretty = PyLogFormat::py_new("pretty").unwrap();
        assert_eq!(pretty, PyLogFormat::Pretty);
        assert_eq!(pretty.__str__(), "pretty");
    }

    #[test]
    fn test_py_layer_destination_conversions_and_repr() {
        Python::initialize();
        let value = PyLayerDestination::py_new("file").unwrap();
        assert_eq!(value, PyLayerDestination::File);
        assert_eq!(value.__str__(), "file");
        assert_eq!(value.__repr__(), "LayerDestination(\"file\")");

        let console = PyLayerDestination::py_new("console").unwrap();
        assert_eq!(console, PyLayerDestination::Console);
        assert_eq!(console.__str__(), "console");
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
            None,
        );
        assert_eq!(
            config.__repr__(),
            "LayerConfig(name=test_layer, filter_directive=info, format=compact, destination=console, file_dir=None, file_prefix=None, include_span_events=false, max_log_files=None)"
        );

        let core_config = LayerConfig::from(&config);
        assert_eq!(core_config.name, "test_layer");
        assert_eq!(core_config.format, LogFormat::Compact);
    }

    #[test]
    fn test_py_layer_config_repr_with_file_settings() {
        let config = PyLayerConfig::py_new(
            "file_layer".to_string(),
            "debug".to_string(),
            PyLogFormat::Json,
            PyLayerDestination::File,
            Some("./logs".to_string()),
            Some("app".to_string()),
            true,
            Some(3),
        );
        assert_eq!(
            config.__repr__(),
            "LayerConfig(name=file_layer, filter_directive=debug, format=json, destination=file, file_dir=./logs, file_prefix=app, include_span_events=true, max_log_files=3)"
        );
    }

    #[test]
    fn test_dict_to_string() {
        Python::initialize();
        Python::attach(|py| {
            let dict = PyDict::new(py);
            dict.set_item("key1", "value1").unwrap();
            let result = dict_to_string(&dict);
            assert_eq!(result, "key1=value1");
        });
    }

    #[test]
    fn test_dict_to_json_preserves_native_types() {
        Python::initialize();
        Python::attach(|py| {
            let dict = PyDict::new(py);
            dict.set_item("count", 42i64).unwrap();
            dict.set_item("ratio", 1.5f64).unwrap();
            dict.set_item("enabled", true).unwrap();
            dict.set_item("name", "bob").unwrap();
            dict.set_item("nothing", py.None()).unwrap();
            dict.set_item("tags", vec!["a", "b"]).unwrap();

            let nested = PyDict::new(py);
            nested.set_item("inner", 1i64).unwrap();
            dict.set_item("nested", &nested).unwrap();

            let value = dict_to_json(&dict);
            assert_eq!(value["count"], serde_json::json!(42));
            assert_eq!(value["ratio"], serde_json::json!(1.5));
            assert_eq!(value["enabled"], serde_json::json!(true));
            assert_eq!(value["name"], serde_json::json!("bob"));
            assert_eq!(value["nothing"], serde_json::Value::Null);
            assert_eq!(value["tags"], serde_json::json!(["a", "b"]));
            assert_eq!(value["nested"], serde_json::json!({"inner": 1}));
        });
    }

    #[test]
    fn test_init_tracing_wrapper_success() {
        Python::initialize();
        let config = PyLayerConfig::py_new(
            "wrapper_test".to_string(),
            "off".to_string(),
            PyLogFormat::Compact,
            PyLayerDestination::Console,
            None,
            None,
            false,
            None,
        );
        let result = init_tracing(vec![config]);
        assert!(result.is_ok());
    }

    #[test]
    fn test_log_sink_wrapper_smoke() {
        Python::initialize();
        Python::attach(|py| {
            let extra = PyDict::new(py);
            extra.set_item("k", "v").unwrap();
            _log_sink(20, "hello", None, None, None, None, Some(extra), None, None);
        });
    }

    #[test]
    fn test_spans_to_string_formats_frames_with_and_without_fields() {
        Python::initialize();
        Python::attach(|py| {
            let with_fields = PyDict::new(py);
            with_fields.set_item("name", "request").unwrap();
            let fields = PyDict::new(py);
            fields.set_item("request_id", 42).unwrap();
            with_fields.set_item("fields", &fields).unwrap();

            let without_fields = PyDict::new(py);
            without_fields.set_item("name", "outer").unwrap();
            without_fields.set_item("fields", PyDict::new(py)).unwrap();

            let spans = PyList::new(py, [without_fields, with_fields]).unwrap();
            assert_eq!(spans_to_string(&spans), "outer > request{request_id=42}");
        });
    }

    #[test]
    fn test_log_sink_wrapper_smoke_with_spans() {
        Python::initialize();
        Python::attach(|py| {
            let frame = PyDict::new(py);
            frame.set_item("name", "request").unwrap();
            let fields = PyDict::new(py);
            fields.set_item("request_id", 42).unwrap();
            frame.set_item("fields", fields).unwrap();
            let spans = PyList::new(py, [frame]).unwrap();

            _log_sink(20, "hello", None, None, None, None, None, Some(spans), None);
        });
    }

    #[test]
    fn test_log_sink_wrapper_smoke_with_exception() {
        Python::initialize();
        Python::attach(|py| {
            let exception = PyDict::new(py);
            exception.set_item("type", "ValueError").unwrap();
            exception.set_item("message", "oops").unwrap();
            exception.set_item("traceback", "Traceback...").unwrap();

            _log_sink(
                40,
                "boom",
                None,
                None,
                None,
                None,
                None,
                None,
                Some(exception),
            );
        });
    }
}
