use std::str::FromStr;
use std::sync::Once;

use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::PyDict;

use tracing_appender::non_blocking::WorkerGuard;
// use tracing_log::LogTracer;
use tracing_subscriber::{
    EnvFilter, Layer, Registry,
    fmt::{self, format::FmtSpan, writer::BoxMakeWriter},
    layer::SubscriberExt,
    util::SubscriberInitExt,
};

static INIT: Once = Once::new();

/// Defines the output format of the logs.
#[pyclass(name = "LogFormat", from_py_object)]
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum PyLogFormat {
    #[default]
    Compact,
    Pretty,
    Json,
}

#[pymethods]
impl PyLogFormat {
    #[new]
    fn py_new(value: &str) -> PyResult<Self> {
        Self::from_str(value).map_err(|e: String| PyValueError::new_err(e))
    }

    fn __str__(&self) -> String {
        match self {
            PyLogFormat::Compact => "compact".to_string(),
            PyLogFormat::Pretty => "pretty".to_string(),
            PyLogFormat::Json => "json".to_string(),
        }
    }

    fn __repr__(&self) -> String {
        let self_string = self.__str__();
        format!("<LogFormat.{}: '{}'>", self_string, self_string)
    }
}

impl FromStr for PyLogFormat {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "json" => Ok(PyLogFormat::Json),
            "pretty" => Ok(PyLogFormat::Pretty),
            "compact" => Ok(PyLogFormat::Compact),
            _ => Err(format!("Unknown log format: {}", s)),
        }
    }
}

/// Defines where a layer should write its logs.
#[pyclass(name = "LayerDestination", from_py_object)]
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub enum PyLayerDestination {
    #[default]
    Console,
    File,
}

#[pymethods]
impl PyLayerDestination {
    #[new]
    fn py_new(value: &str) -> PyResult<Self> {
        Self::from_str(value).map_err(|e: String| PyValueError::new_err(e))
    }

    fn __str__(&self) -> String {
        match self {
            PyLayerDestination::Console => "console".to_string(),
            PyLayerDestination::File => "file".to_string(),
        }
    }

    fn __repr__(&self) -> String {
        let self_string = self.__str__();
        format!("LayerDestination(\"{}\")", self_string)
    }
}

impl FromStr for PyLayerDestination {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "console" => Ok(PyLayerDestination::Console),
            "file" => Ok(PyLayerDestination::File),
            _ => Err(format!("Unknown log destination: {}", s)),
        }
    }
}

/// Configuration for a single tracing layer.
/// Each layer can have its own independent filter, format, and destination.
#[pyclass(name = "LayerConfig", from_py_object)]
#[derive(Debug, Clone)]
pub struct PyLayerConfig {
    /// An optional friendly name for this layer (useful for config debugging).
    #[pyo3(get, set)]
    pub name: String,

    /// The name/target filter directive specific to this layer.
    /// Example: "info", "my_app::db=debug", or "hyper=trace".
    /// This allows routing specific targets (names) to specific layers.
    #[pyo3(get, set)]
    pub filter_directive: String,

    /// The formatting style of this specific layer.
    #[pyo3(get, set)]
    pub format: PyLogFormat,

    /// Where the logs for this layer should be written.
    #[pyo3(get, set)]
    pub destination: PyLayerDestination,

    /// If `destination` is `File`, this specifies the directory to write log files to.
    #[pyo3(get, set)]
    pub file_dir: Option<String>,

    /// If `destination` is `File`, this specifies the prefix for log file names.
    #[pyo3(get, set)]
    pub file_prefix: Option<String>,

    /// Whether to log when a span is closed (useful for timing).
    #[pyo3(get, set)]
    pub include_span_events: bool,
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
    _guards: Vec<WorkerGuard>,
}

// Boxed layer type
type LogLayer = Box<dyn Layer<Registry> + Send + Sync>;

// HELPME: This function is not tested because it is not possible to initialize tracing multiple times.
//         Though testing on the python side _is_ possible, we still have a problem on multiple inits...
// NOTE: Keeping the coverage(off) attr disabled until it gets stable.
/// The main entry point for Python to initialize tracing.
#[pyfunction]
#[pyo3(signature = (layers))]
pub fn init_tracing(layers: Vec<PyLayerConfig>) -> PyResult<PyTracingGuard> {
    // 1. Redirect standard `log` macros to `tracing`.
    // LogTracer::init().map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;
    if INIT.is_completed() {
        tracing::warn!(
            "Tracing has already been initialized. Calling init_tracing multiple times is not supported and may lead to unexpected behavior."
        );
        return Ok(PyTracingGuard {
            _guards: Vec::new(),
        });
    }

    let mut guards = Vec::new();
    let mut subscriber_layers: Vec<LogLayer> = Vec::new();
    let mut initialized_info = Vec::new();

    // 2. Build each layer
    for config in layers {
        let (layer, guard) = build_layer_internal(&config).map_err(|e| {
            pyo3::exceptions::PyValueError::new_err(format!(
                "Failed to initialize layer '{}': {}",
                config.name, e
            ))
        })?;
        subscriber_layers.push(layer);
        if let Some(g) = guard {
            guards.push(g);
        }
        initialized_info.push((config.name.clone(), config.filter_directive.clone()));
    }

    // Use Once to ensure the global state is set exactly once
    // This is thread-safe and more idiomatic for global state
    let mut init_err = None;
    INIT.call_once(|| {
        if let Err(e) = Registry::default().with(subscriber_layers).try_init() {
            init_err = Some(e.to_string());
        }
    });

    if let Some(err_msg) = init_err {
        return Err(pyo3::exceptions::PyRuntimeError::new_err(err_msg));
    }

    // Success log
    for (name, filter) in initialized_info {
        tracing::debug!(layer_name = %name, filter = %filter, "Tracing layer successfully attached.");
    }

    Ok(PyTracingGuard { _guards: guards })
}

/// Helper to build a single layer based on configuration.
fn build_layer_internal(config: &PyLayerConfig) -> Result<(LogLayer, Option<WorkerGuard>), String> {
    let env_filter = EnvFilter::new(&config.filter_directive);
    let span_events = if config.include_span_events {
        FmtSpan::CLOSE
    } else {
        FmtSpan::NONE
    };

    match config.destination {
        PyLayerDestination::Console => {
            let writer = BoxMakeWriter::new(std::io::stdout);
            let layer = build_fmt_layer(writer, config.format, span_events, true)
                .with_filter(env_filter)
                .boxed();
            Ok((layer, None))
        }
        PyLayerDestination::File => {
            let dir = config
                .file_dir
                .clone()
                .unwrap_or_else(|| "./logs".to_string());
            let prefix = config
                .file_prefix
                .clone()
                .unwrap_or_else(|| "app".to_string());

            let file_appender = tracing_appender::rolling::daily(dir, prefix);
            let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);

            let writer = BoxMakeWriter::new(non_blocking);
            let layer = build_fmt_layer(writer, config.format, span_events, false)
                .with_filter(env_filter)
                .boxed();
            Ok((layer, Some(guard)))
        }
    }
}

/// Helper to configure the formatting layer with common production settings.
/// Uses BoxMakeWriter to avoid complex Higher-Rank Trait Bound (HRTB) issues with generics.
fn build_fmt_layer(
    writer: BoxMakeWriter,
    format: PyLogFormat,
    span_events: FmtSpan,
    ansi: bool,
) -> LogLayer {
    match format {
        PyLogFormat::Json => fmt::layer()
            .with_timer(fmt::time::LocalTime::rfc_3339())
            .with_writer(writer)
            .with_ansi(ansi)
            .with_span_events(span_events)
            .json()
            .flatten_event(true)
            .with_current_span(true)
            .with_target(false)
            .boxed(),
        PyLogFormat::Pretty => fmt::layer()
            .with_timer(fmt::time::LocalTime::rfc_3339())
            .with_writer(writer)
            .with_ansi(ansi)
            .with_span_events(span_events)
            .pretty()
            .with_target(false)
            .boxed(),
        PyLogFormat::Compact => fmt::layer()
            .with_timer(fmt::time::LocalTime::rfc_3339())
            .with_writer(writer)
            .with_ansi(ansi)
            .with_span_events(span_events)
            .compact()
            .with_target(false)
            .boxed(),
    }
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
    _log_sink(20, message, None, None, None, None, extra);
}

#[pyfunction(name = "warn")]
#[pyo3(signature = (message, extra=None))]
pub fn warn(message: &str, extra: Option<Bound<'_, PyDict>>) {
    _log_sink(30, message, None, None, None, None, extra);
}

#[pyfunction(name = "error")]
#[pyo3(signature = (message, extra=None))]
pub fn error(message: &str, extra: Option<Bound<'_, PyDict>>) {
    _log_sink(40, message, None, None, None, None, extra);
}

#[pyfunction(name = "debug")]
#[pyo3(signature = (message, extra=None))]
pub fn debug(message: &str, extra: Option<Bound<'_, PyDict>>) {
    _log_sink(10, message, None, None, None, None, extra);
}

#[pyfunction(name = "trace")]
#[pyo3(signature = (message, extra=None))]
pub fn trace(message: &str, extra: Option<Bound<'_, PyDict>>) {
    _log_sink(0, message, None, None, None, None, extra);
}

macro_rules! dispatch_log {
    ($level:expr, $msg:expr, $location:expr, $extra:expr) => {
        match ($location, $extra) {
            (Some(loc), Some(e)) => {
                match $level {
                    0..=9 => tracing::trace!(location = %loc, extra = %e, "{}", $msg),
                    10..=19 => tracing::debug!(location = %loc, extra = %e, "{}", $msg),
                    20..=29 => tracing::info!(location = %loc, extra = %e, "{}", $msg),
                    30..=39 => tracing::warn!(location = %loc, extra = %e, "{}", $msg),
                    _ => tracing::error!(location = %loc, extra = %e, "{}", $msg),
                }
            }
            (Some(loc), None) => {
                match $level {
                    0..=9 => tracing::trace!(location = %loc, "{}", $msg),
                    10..=19 => tracing::debug!(location = %loc, "{}", $msg),
                    20..=29 => tracing::info!(location = %loc, "{}", $msg),
                    30..=39 => tracing::warn!(location = %loc, "{}", $msg),
                    _ => tracing::error!(location = %loc, "{}", $msg),
                }
            }
            (None, Some(e)) => {
                match $level {
                    0..=9 => tracing::trace!(extra = %e, "{}", $msg),
                    10..=19 => tracing::debug!(extra = %e, "{}", $msg),
                    20..=29 => tracing::info!(extra = %e, "{}", $msg),
                    30..=39 => tracing::warn!(extra = %e, "{}", $msg),
                    _ => tracing::error!(extra = %e, "{}", $msg),
                }
            }
            (None, None) => {
                match $level {
                    0..=9 => tracing::trace!("{}", $msg),
                    10..=19 => tracing::debug!("{}", $msg),
                    20..=29 => tracing::info!("{}", $msg),
                    30..=39 => tracing::warn!("{}", $msg),
                    _ => tracing::error!("{}", $msg),
                }
            }
        }
    };
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
    let extra_str = extra.map(|d| dict_to_string(d));

    // Group metadata into a single packed string if any part is present
    let location_str = if let Some(ref f) = filename {
        Some(format!(
            "{}.{} in {}:{}",
            module_name.as_deref().unwrap_or("?"),
            func_name.as_deref().unwrap_or("?"),
            f,
            lineno.unwrap_or(0),
        ))
    } else if func_name.is_some() || module_name.is_some() {
        Some(format!(
            "{}.{} in {}",
            module_name.as_deref().unwrap_or("?"),
            func_name.as_deref().unwrap_or("?"),
            lineno.unwrap_or(0)
        ))
    } else {
        None
    };

    dispatch_log!(levelno, message, location_str, extra_str);
}

#[cfg(test)]
mod tests {
    use super::*;
    use pyo3::Python;
    use pyo3::types::PyDict;
    use tracing_test::traced_test;

    #[test]
    fn test_log_format_from_str() {
        assert_eq!(
            PyLogFormat::from_str("compact").unwrap(),
            PyLogFormat::Compact
        );
        assert_eq!(
            PyLogFormat::from_str("pretty").unwrap(),
            PyLogFormat::Pretty
        );
        assert_eq!(PyLogFormat::from_str("json").unwrap(), PyLogFormat::Json);
        assert_eq!(PyLogFormat::from_str("JSON").unwrap(), PyLogFormat::Json);
        assert!(PyLogFormat::from_str("unknown").is_err());
    }

    #[test]
    fn test_log_format_new() {
        assert_eq!(
            PyLogFormat::py_new("compact").unwrap(),
            PyLogFormat::Compact
        );
        assert!(PyLogFormat::py_new("unknown").is_err());
    }

    #[test]
    fn test_log_format_str_repr() {
        let format = PyLogFormat::Json;
        assert_eq!(format.__str__(), "json");
        assert_eq!(format.__repr__(), "<LogFormat.json: 'json'>");
    }

    #[test]
    fn test_layer_destination_from_str() {
        assert_eq!(
            PyLayerDestination::from_str("console").unwrap(),
            PyLayerDestination::Console
        );
        assert_eq!(
            PyLayerDestination::from_str("file").unwrap(),
            PyLayerDestination::File
        );
        assert_eq!(
            PyLayerDestination::from_str("FILE").unwrap(),
            PyLayerDestination::File
        );
        assert!(PyLayerDestination::from_str("unknown").is_err());
    }

    #[test]
    fn test_layer_destination_new() {
        assert_eq!(
            PyLayerDestination::py_new("console").unwrap(),
            PyLayerDestination::Console
        );
        assert!(PyLayerDestination::py_new("unknown").is_err());
    }

    #[test]
    fn test_layer_destination_str_repr() {
        let dest = PyLayerDestination::File;
        assert_eq!(dest.__str__(), "file");
        assert_eq!(dest.__repr__(), "LayerDestination(\"file\")");
    }

    #[test]
    fn test_layer_config_repr() {
        let config = PyLayerConfig::py_new(
            "test_layer".to_string(),
            "info".to_string(),
            PyLogFormat::Compact,
            PyLayerDestination::Console,
            None,
            None,
            false,
        );
        let repr = config.__repr__();
        assert_eq!(
            repr,
            "LayerConfig(name=test_layer, filter_directive=info, format=compact, destination=console, file_dir=None, file_prefix=None, include_span_events=false)"
        );

        let config2 = PyLayerConfig::py_new(
            "test_layer".to_string(),
            "info".to_string(),
            PyLogFormat::Json,
            PyLayerDestination::File,
            Some("/tmp".to_string()),
            Some("app.log".to_string()),
            true,
        );
        let repr2 = config2.__repr__();
        assert_eq!(
            repr2,
            "LayerConfig(name=test_layer, filter_directive=info, format=json, destination=file, file_dir=/tmp, file_prefix=app.log, include_span_events=true)"
        );
    }

    #[test]
    fn test_dict_to_string() {
        Python::initialize();
        Python::attach(|py| {
            let dict = PyDict::new(py);
            dict.set_item("key1", "value1").unwrap();
            dict.set_item("key2", 42).unwrap();
            let result = dict_to_string(dict);
            assert!(result == "key1=value1, key2=42" || result == "key2=42, key1=value1");
        });
    }

    #[test]
    fn test_build_layer_internal_console() {
        // Compact
        let config = PyLayerConfig::py_new(
            "console_layer".to_string(),
            "info".to_string(),
            PyLogFormat::Compact,
            PyLayerDestination::Console,
            None,
            None,
            false,
        );
        let result = build_layer_internal(&config);
        assert!(result.is_ok());
        let (_layer, guard) = result.unwrap();
        assert!(guard.is_none());

        // Json
        let config = PyLayerConfig::py_new(
            "console_layer".to_string(),
            "info".to_string(),
            PyLogFormat::Json,
            PyLayerDestination::Console,
            None,
            None,
            false,
        );
        let result = build_layer_internal(&config);
        assert!(result.is_ok());
        let (_layer, guard) = result.unwrap();
        assert!(guard.is_none());

        // Pretty
        let config = PyLayerConfig::py_new(
            "console_layer".to_string(),
            "info".to_string(),
            PyLogFormat::Pretty,
            PyLayerDestination::Console,
            None,
            None,
            false,
        );
        let result = build_layer_internal(&config);
        assert!(result.is_ok());
        let (_layer, guard) = result.unwrap();
        assert!(guard.is_none());
    }

    #[test]
    fn test_build_layer_internal_file() {
        // Json
        // We only test for Json for file dests because format related configuration calls the same function.
        let config = PyLayerConfig::py_new(
            "file_layer".to_string(),
            "debug".to_string(),
            PyLogFormat::Json,
            PyLayerDestination::File,
            Some("./logs".to_string()),
            Some("test_app".to_string()),
            true,
        );
        let result = build_layer_internal(&config);
        assert!(result.is_ok());
        let (_layer, guard) = result.unwrap();
        assert!(guard.is_some()); // File layers yield a non-blocking worker guard
    }

    /// Create a list of log level and corresponding log names for testing.
    fn get_log_level_and_names() -> Vec<(u8, String)> {
        vec![
            (0, "TRACE".to_string()),
            (10, "DEBUG".to_string()),
            (20, "INFO".to_string()),
            (30, "WARN".to_string()),
            (40, "ERROR".to_string()),
        ]
    }

    #[test]
    #[traced_test]
    fn test_log_sink() {
        Python::initialize();
        Python::attach(|py| {
            let extra = PyDict::new(py);
            extra.set_item("test", "data").unwrap();

            for (level, name) in get_log_level_and_names() {
                _log_sink(
                    level,
                    "test message",
                    Some("test.py".to_string()),
                    Some("test_func".to_string()),
                    Some(42),
                    Some("test_module".to_string()),
                    Some(extra.clone()),
                );

                assert!(logs_contain(&name));
                assert!(logs_contain("test message"));
                assert!(logs_contain("location="));
                assert!(logs_contain("extra="));
            }
        });
    }

    #[test]
    #[traced_test]
    fn test_log_sink_no_filename() {
        Python::initialize();
        Python::attach(|py| {
            let extra = PyDict::new(py);
            extra.set_item("test", "data").unwrap();

            for (level, name) in get_log_level_and_names() {
                _log_sink(
                    level,
                    "test message",
                    None,
                    Some("test_func".to_string()),
                    Some(42),
                    Some("test_module".to_string()),
                    Some(extra.clone()),
                );

                assert!(logs_contain(&name));
                assert!(logs_contain("test message"));
                assert!(logs_contain("location="));
                assert!(logs_contain("extra="));
            }
        });
    }

    #[test]
    #[traced_test]
    fn test_log_sink_no_location() {
        Python::initialize();
        Python::attach(|py| {
            let extra = PyDict::new(py);
            extra.set_item("test", "data").unwrap();

            for (level, name) in get_log_level_and_names() {
                _log_sink(
                    level,
                    "test message",
                    None,
                    None,
                    None,
                    None,
                    Some(extra.clone()),
                );

                assert!(logs_contain(&name));
                assert!(logs_contain("test message"));
                assert!(!logs_contain("location="));
                assert!(logs_contain("extra="));
            }
        });
    }

    #[test]
    #[traced_test]
    fn test_log_sink_no_extra() {
        Python::initialize();
        Python::attach(|py| {
            let extra = PyDict::new(py);
            extra.set_item("test", "data").unwrap();

            for (level, name) in get_log_level_and_names() {
                _log_sink(
                    level,
                    "test message",
                    Some("test.py".to_string()),
                    Some("test_func".to_string()),
                    Some(42),
                    Some("test_module".to_string()),
                    None,
                );

                assert!(logs_contain(&name));
                assert!(logs_contain("test message"));
                assert!(logs_contain("location="));
                assert!(!logs_contain("extra="));
            }
        });
    }

    #[test]
    #[traced_test]
    fn test_log_sink_no_details() {
        Python::initialize();
        Python::attach(|_| {
            for (level, name) in get_log_level_and_names() {
                _log_sink(level, "test message", None, None, None, None, None);

                assert!(logs_contain(&name));
                assert!(logs_contain("test message"));
                assert!(!logs_contain("location="));
                assert!(!logs_contain("extra="));
            }
        });
    }

    #[test]
    #[traced_test]
    fn test_trace_direct() {
        Python::initialize();
        Python::attach(|py| {
            let extra = PyDict::new(py);
            extra.set_item("test", "data").unwrap();

            trace("test message", None);

            // assert basic log outputs
            assert!(logs_contain("TRACE"));
            assert!(logs_contain("test message"));
            assert!(!logs_contain("extra="));

            trace("test message", Some(extra.clone()));

            // assert new log includes extra field
            assert!(logs_contain("extra="));
        });
    }

    #[test]
    #[traced_test]
    fn test_debug_direct() {
        Python::initialize();
        Python::attach(|py| {
            let extra = PyDict::new(py);
            extra.set_item("test", "data").unwrap();

            debug("test message", None);

            // assert basic log outputs
            assert!(logs_contain("DEBUG"));
            assert!(logs_contain("test message"));
            assert!(!logs_contain("extra="));

            debug("test message", Some(extra.clone()));

            // assert new log includes extra field
            assert!(logs_contain("extra="));
        });
    }

    #[test]
    #[traced_test]
    fn test_info_direct() {
        Python::initialize();
        Python::attach(|py| {
            let extra = PyDict::new(py);
            extra.set_item("test", "data").unwrap();

            info("test message", None);

            // assert basic log outputs
            assert!(logs_contain("INFO"));
            assert!(logs_contain("test message"));
            assert!(!logs_contain("extra="));

            info("test message", Some(extra.clone()));

            // assert new log includes extra field
            assert!(logs_contain("extra="));
        });
    }

    #[test]
    #[traced_test]
    fn test_warn_direct() {
        Python::initialize();
        Python::attach(|py| {
            let extra = PyDict::new(py);
            extra.set_item("test", "data").unwrap();

            warn("test message", None);

            // assert basic log outputs
            assert!(logs_contain("WARN"));
            assert!(logs_contain("test message"));
            assert!(!logs_contain("extra="));

            warn("test message", Some(extra.clone()));

            // assert new log includes extra field
            assert!(logs_contain("extra="));
        });
    }

    #[test]
    #[traced_test]
    fn test_error_direct() {
        Python::initialize();
        Python::attach(|py| {
            let extra = PyDict::new(py);
            extra.set_item("test", "data").unwrap();

            error("test message", None);

            // assert basic log outputs
            assert!(logs_contain("ERROR"));
            assert!(logs_contain("test message"));
            assert!(!logs_contain("extra="));

            error("test message", Some(extra.clone()));

            // assert new log includes extra field
            assert!(logs_contain("extra="));
        });
    }
}
