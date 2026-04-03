use std::str::FromStr;

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
    fn new(value: &str) -> PyResult<Self> {
        Self::from_str(value).map_err(|e: String| PyValueError::new_err(e))
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
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PyLayerDestination {
    Console,
    File,
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
}

/// A guard object that keeps background logging threads alive.
/// In Python, if this object is garbage collected, file logging will stop.
#[pyclass(name = "TracingGuard")]
pub struct PyTracingGuard {
    _guards: Vec<WorkerGuard>,
}

/// The main entry point for Python to initialize tracing.
#[pyfunction]
#[pyo3(signature = (layers))]
pub fn init_tracing(layers: Vec<PyLayerConfig>) -> PyResult<PyTracingGuard> {
    // 1. Redirect standard `log` macros to `tracing`.
    // LogTracer::init().map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;

    let mut guards = Vec::new();
    let mut subscriber_layers: Vec<Box<dyn Layer<Registry> + Send + Sync>> = Vec::new();

    // 2. Build each layer
    for config in layers {
        let (layer, guard) = build_layer_internal(&config);
        subscriber_layers.push(layer);
        if let Some(g) = guard {
            guards.push(g);
        }
    }

    // 3. Initialize Registry
    Registry::default()
        .with(subscriber_layers)
        .try_init()
        .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;

    Ok(PyTracingGuard { _guards: guards })
}

/// Helper to build a single layer based on configuration.
fn build_layer_internal(
    config: &PyLayerConfig,
) -> (Box<dyn Layer<Registry> + Send + Sync>, Option<WorkerGuard>) {
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
            (layer, None)
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
            (layer, Some(guard))
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
) -> Box<dyn Layer<Registry> + Send + Sync> {
    match format {
        PyLogFormat::Json => fmt::layer()
            .with_writer(writer)
            .with_ansi(ansi)
            .with_span_events(span_events)
            .json()
            .flatten_event(true)
            .with_current_span(true)
            .boxed(),
        PyLogFormat::Pretty => fmt::layer()
            .with_writer(writer)
            .with_ansi(ansi)
            .with_span_events(span_events)
            .pretty()
            .boxed(),
        PyLogFormat::Compact => fmt::layer()
            .with_writer(writer)
            .with_ansi(ansi)
            .with_span_events(span_events)
            .compact()
            .with_target(true)
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
