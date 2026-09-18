use std::str::FromStr;
use std::sync::Once;

use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::{
    EnvFilter, Layer, Registry,
    fmt::{self, format::FmtSpan, writer::BoxMakeWriter},
    layer::SubscriberExt,
    util::SubscriberInitExt,
};

static INIT: Once = Once::new();

/// The output format of the logs.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum LogFormat {
    #[default]
    Compact,
    Pretty,
    Json,
}

impl LogFormat {
    pub fn to_str(&self) -> &'static str {
        match self {
            LogFormat::Compact => "compact",
            LogFormat::Pretty => "pretty",
            LogFormat::Json => "json",
        }
    }
}

impl FromStr for LogFormat {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "json" => Ok(LogFormat::Json),
            "pretty" => Ok(LogFormat::Pretty),
            "compact" => Ok(LogFormat::Compact),
            _ => Err(format!("Unknown log format: {}", s)),
        }
    }
}

/// Where a layer should write its logs.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub enum LayerDestination {
    #[default]
    Console,
    File,
}

impl LayerDestination {
    pub fn to_str(&self) -> &'static str {
        match self {
            LayerDestination::Console => "console",
            LayerDestination::File => "file",
        }
    }
}

impl FromStr for LayerDestination {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "console" => Ok(LayerDestination::Console),
            "file" => Ok(LayerDestination::File),
            _ => Err(format!("Unknown log destination: {}", s)),
        }
    }
}

/// Configuration for a single tracing layer.
#[derive(Debug, Clone)]
pub struct LayerConfig {
    pub name: String,
    pub filter_directive: String,
    pub format: LogFormat,
    pub destination: LayerDestination,
    pub file_dir: Option<String>,
    pub file_prefix: Option<String>,
    pub include_span_events: bool,
}

impl LayerConfig {
    pub fn new(
        name: String,
        filter_directive: String,
        format: LogFormat,
        destination: LayerDestination,
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

/// Holds the background logging worker guards. Dropping this stops file logging.
pub struct TracingGuards {
    pub guards: Vec<WorkerGuard>,
}

/// Distinguishes which stage of tracing initialization failed, so callers
/// (e.g. the pyo3 wrapper) can map each to a different exception type.
#[derive(Debug)]
pub enum TracingInitError {
    LayerBuild(String),
    RegistryInit(String),
}

impl std::fmt::Display for TracingInitError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TracingInitError::LayerBuild(msg) => write!(f, "{}", msg),
            TracingInitError::RegistryInit(msg) => write!(f, "{}", msg),
        }
    }
}

type LogLayer = Box<dyn Layer<Registry> + Send + Sync>;

/// The main entry point for initializing tracing.
pub fn init_tracing(layers: Vec<LayerConfig>) -> Result<TracingGuards, TracingInitError> {
    if INIT.is_completed() {
        tracing::warn!(
            "Tracing has already been initialized. Calling init_tracing multiple times is not supported and may lead to unexpected behavior."
        );
        return Ok(TracingGuards { guards: Vec::new() });
    }

    let mut guards = Vec::new();
    let mut subscriber_layers: Vec<LogLayer> = Vec::new();
    let mut initialized_info = Vec::new();

    for config in &layers {
        let (layer, guard) = build_layer_internal(config).map_err(|e| {
            TracingInitError::LayerBuild(format!(
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

    let mut init_err = None;
    INIT.call_once(|| {
        if let Err(e) = Registry::default().with(subscriber_layers).try_init() {
            init_err = Some(e.to_string());
        }
    });

    if let Some(err_msg) = init_err {
        return Err(TracingInitError::RegistryInit(err_msg));
    }

    for (name, filter) in initialized_info {
        tracing::debug!(layer_name = %name, filter = %filter, "Tracing layer successfully attached.");
    }

    Ok(TracingGuards { guards })
}

/// Builds a single layer based on configuration.
fn build_layer_internal(config: &LayerConfig) -> Result<(LogLayer, Option<WorkerGuard>), String> {
    let env_filter = EnvFilter::new(&config.filter_directive);
    let span_events = if config.include_span_events {
        FmtSpan::CLOSE
    } else {
        FmtSpan::NONE
    };

    match config.destination {
        LayerDestination::Console => {
            let writer = BoxMakeWriter::new(std::io::stdout);
            let layer = build_fmt_layer(writer, config.format, span_events, true)
                .with_filter(env_filter)
                .boxed();
            Ok((layer, None))
        }
        LayerDestination::File => {
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

/// Configures the formatting layer with common production settings.
fn build_fmt_layer(
    writer: BoxMakeWriter,
    format: LogFormat,
    span_events: FmtSpan,
    ansi: bool,
) -> LogLayer {
    match format {
        LogFormat::Json => fmt::layer()
            .with_timer(fmt::time::LocalTime::rfc_3339())
            .with_writer(writer)
            .with_ansi(ansi)
            .with_span_events(span_events)
            .json()
            .flatten_event(true)
            .with_current_span(true)
            .with_target(false)
            .boxed(),
        LogFormat::Pretty => fmt::layer()
            .with_timer(fmt::time::LocalTime::rfc_3339())
            .with_writer(writer)
            .with_ansi(ansi)
            .with_span_events(span_events)
            .pretty()
            .with_target(false)
            .boxed(),
        LogFormat::Compact => fmt::layer()
            .with_timer(fmt::time::LocalTime::rfc_3339())
            .with_writer(writer)
            .with_ansi(ansi)
            .with_span_events(span_events)
            .compact()
            .with_target(false)
            .boxed(),
    }
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

/// Dispatches a single log line to `tracing`, given already-formatted metadata.
pub fn log_sink(
    levelno: u8,
    message: &str,
    filename: Option<String>,
    func_name: Option<String>,
    lineno: Option<usize>,
    module_name: Option<String>,
    extra: Option<&str>,
) {
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

    dispatch_log!(levelno, message, location_str, extra);
}

#[cfg(test)]
mod tests {
    use super::*;
    use tracing_test::traced_test;

    #[test]
    fn test_log_format_from_str() {
        assert_eq!(LogFormat::from_str("compact").unwrap(), LogFormat::Compact);
        assert_eq!(LogFormat::from_str("pretty").unwrap(), LogFormat::Pretty);
        assert_eq!(LogFormat::from_str("json").unwrap(), LogFormat::Json);
        assert_eq!(LogFormat::from_str("JSON").unwrap(), LogFormat::Json);
        assert!(LogFormat::from_str("unknown").is_err());
    }

    #[test]
    fn test_log_format_to_str() {
        assert_eq!(LogFormat::Json.to_str(), "json");
        assert_eq!(LogFormat::Pretty.to_str(), "pretty");
        assert_eq!(LogFormat::Compact.to_str(), "compact");
    }

    #[test]
    fn test_layer_destination_from_str() {
        assert_eq!(
            LayerDestination::from_str("console").unwrap(),
            LayerDestination::Console
        );
        assert_eq!(
            LayerDestination::from_str("file").unwrap(),
            LayerDestination::File
        );
        assert_eq!(
            LayerDestination::from_str("FILE").unwrap(),
            LayerDestination::File
        );
        assert!(LayerDestination::from_str("unknown").is_err());
    }

    #[test]
    fn test_tracing_init_error_display() {
        assert_eq!(
            TracingInitError::LayerBuild("layer boom".to_string()).to_string(),
            "layer boom"
        );
        assert_eq!(
            TracingInitError::RegistryInit("registry boom".to_string()).to_string(),
            "registry boom"
        );
    }

    #[test]
    fn test_layer_destination_to_str() {
        assert_eq!(LayerDestination::Console.to_str(), "console");
        assert_eq!(LayerDestination::File.to_str(), "file");
    }

    #[test]
    fn test_build_layer_internal_console() {
        for format in [LogFormat::Compact, LogFormat::Json, LogFormat::Pretty] {
            let config = LayerConfig::new(
                "console_layer".to_string(),
                "info".to_string(),
                format,
                LayerDestination::Console,
                None,
                None,
                false,
            );
            let result = build_layer_internal(&config);
            assert!(result.is_ok());
            let (_layer, guard) = result.unwrap();
            assert!(guard.is_none());
        }
    }

    #[test]
    fn test_build_layer_internal_file() {
        let config = LayerConfig::new(
            "file_layer".to_string(),
            "debug".to_string(),
            LogFormat::Json,
            LayerDestination::File,
            Some("./logs".to_string()),
            Some("test_app".to_string()),
            true,
        );
        let result = build_layer_internal(&config);
        assert!(result.is_ok());
        let (_layer, guard) = result.unwrap();
        assert!(guard.is_some());
    }

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
        for (level, name) in get_log_level_and_names() {
            log_sink(
                level,
                "test message",
                Some("test.py".to_string()),
                Some("test_func".to_string()),
                Some(42),
                Some("test_module".to_string()),
                Some("test=data"),
            );

            assert!(logs_contain(&name));
            assert!(logs_contain("test message"));
            assert!(logs_contain("location="));
            assert!(logs_contain("extra="));
        }
    }

    #[test]
    #[traced_test]
    fn test_log_sink_no_filename() {
        for (level, name) in get_log_level_and_names() {
            log_sink(
                level,
                "test message",
                None,
                Some("test_func".to_string()),
                Some(42),
                Some("test_module".to_string()),
                Some("test=data"),
            );

            assert!(logs_contain(&name));
            assert!(logs_contain("test message"));
            assert!(logs_contain("location="));
            assert!(logs_contain("extra="));
        }
    }

    #[test]
    #[traced_test]
    fn test_log_sink_no_location() {
        for (level, name) in get_log_level_and_names() {
            log_sink(
                level,
                "test message",
                None,
                None,
                None,
                None,
                Some("test=data"),
            );

            assert!(logs_contain(&name));
            assert!(logs_contain("test message"));
            assert!(!logs_contain("location="));
            assert!(logs_contain("extra="));
        }
    }

    #[test]
    #[traced_test]
    fn test_log_sink_no_extra() {
        for (level, name) in get_log_level_and_names() {
            log_sink(
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
    }

    #[test]
    #[traced_test]
    fn test_log_sink_no_details() {
        for (level, name) in get_log_level_and_names() {
            log_sink(level, "test message", None, None, None, None, None);

            assert!(logs_contain(&name));
            assert!(logs_contain("test message"));
            assert!(!logs_contain("location="));
            assert!(!logs_contain("extra="));
        }
    }
}
