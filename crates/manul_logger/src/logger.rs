use std::cell::RefCell;
use std::io;
use std::str::FromStr;
use std::sync::Once;

use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::{
    EnvFilter, Layer, Registry,
    fmt::{
        self,
        format::FmtSpan,
        writer::{BoxMakeWriter, MakeWriter},
    },
    layer::SubscriberExt,
    util::SubscriberInitExt,
};

static INIT: Once = Once::new();

thread_local! {
    /// Holds the structured `attributes` payload for the log call currently being
    /// dispatched, so `AttributeInjectingWriter` can merge it into JSON output without
    /// routing it through `tracing`'s static field system (see `log_sink`).
    static CURRENT_ATTRIBUTES: RefCell<Option<serde_json::Value>> = const { RefCell::new(None) };

    /// Holds the current span-stack payload for the log call currently being
    /// dispatched, mirroring `CURRENT_ATTRIBUTES`. Spans are tracked entirely on the
    /// Python side (a `contextvars.ContextVar` stack), not via `tracing`'s own
    /// (OS-thread-local) span registry, so they stay correctly scoped across asyncio
    /// `await` points.
    static CURRENT_SPANS: RefCell<Option<serde_json::Value>> = const { RefCell::new(None) };

    /// Holds the current `{type, message, traceback}` exception payload, mirroring
    /// `CURRENT_ATTRIBUTES`. JSON-only: text formats already get the traceback via
    /// Python's `logging.Formatter.format`, which appends it straight into `message`.
    static CURRENT_EXCEPTION: RefCell<Option<serde_json::Value>> = const { RefCell::new(None) };
}

/// A `MakeWriter` that wraps another one, splicing `CURRENT_ATTRIBUTES` into each
/// formatted JSON line as it's written, replacing the stringified `extra` field.
#[derive(Clone)]
struct AttributeInjectingMakeWriter<M> {
    inner: M,
}

impl<'a, M> MakeWriter<'a> for AttributeInjectingMakeWriter<M>
where
    M: MakeWriter<'a>,
{
    type Writer = AttributeInjectingWriter<M::Writer>;

    fn make_writer(&'a self) -> Self::Writer {
        AttributeInjectingWriter {
            inner: self.inner.make_writer(),
        }
    }
}

struct AttributeInjectingWriter<W> {
    inner: W,
}

impl<W: io::Write> io::Write for AttributeInjectingWriter<W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        match inject_attributes(buf) {
            Some(merged) => {
                self.inner.write_all(&merged)?;
                Ok(buf.len())
            }
            None => self.inner.write(buf),
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}

/// Merges the pending `CURRENT_ATTRIBUTES`/`CURRENT_SPANS` values into a formatted
/// JSON log line, replacing their stringified fields with typed ones (`extra` ->
/// `attributes`; `spans` is overwritten in place). Returns `None` (leaving `buf`
/// untouched) when there's nothing pending, or `buf` isn't a JSON object (e.g. it's
/// the bare `\n` `fmt` sometimes writes separately).
fn inject_attributes(buf: &[u8]) -> Option<Vec<u8>> {
    let attributes = CURRENT_ATTRIBUTES.with(|cell| cell.borrow().clone());
    let spans = CURRENT_SPANS.with(|cell| cell.borrow().clone());
    let exception = CURRENT_EXCEPTION.with(|cell| cell.borrow().clone());
    if attributes.is_none() && spans.is_none() && exception.is_none() {
        return None;
    }
    let mut value: serde_json::Value = serde_json::from_slice(buf).ok()?;
    let object = value.as_object_mut()?;
    if let Some(attributes) = attributes {
        object.remove("extra");
        object.insert("attributes".to_string(), attributes);
    }
    if let Some(spans) = spans {
        object.insert("spans".to_string(), spans);
    }
    if let Some(exception) = exception {
        object.insert("exception".to_string(), exception);
    }
    let mut out = serde_json::to_vec(&value).ok()?;
    out.push(b'\n');
    Some(out)
}

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

/// Returned when the global tracing subscriber could not be installed, e.g. because
/// one was already installed elsewhere, bypassing `init_tracing`'s own `Once` guard.
#[derive(Debug)]
pub struct TracingInitError(String);

impl std::fmt::Display for TracingInitError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
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
        let (layer, guard) = build_layer_internal(config);
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
        return Err(TracingInitError(err_msg));
    }

    for (name, filter) in initialized_info {
        tracing::debug!(layer_name = %name, filter = %filter, "Tracing layer successfully attached.");
    }

    Ok(TracingGuards { guards })
}

/// Builds a single layer based on configuration.
fn build_layer_internal(config: &LayerConfig) -> (LogLayer, Option<WorkerGuard>) {
    let env_filter = EnvFilter::new(&config.filter_directive);
    let span_events = if config.include_span_events {
        FmtSpan::CLOSE
    } else {
        FmtSpan::NONE
    };

    match config.destination {
        LayerDestination::Console => {
            let writer = make_box_writer(std::io::stdout, config.format);
            let layer = build_fmt_layer(writer, config.format, span_events, true)
                .with_filter(env_filter)
                .boxed();
            (layer, None)
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

            let writer = make_box_writer(non_blocking, config.format);
            let layer = build_fmt_layer(writer, config.format, span_events, false)
                .with_filter(env_filter)
                .boxed();
            (layer, Some(guard))
        }
    }
}

/// Wraps `make_writer` with `AttributeInjectingMakeWriter` for the JSON format only —
/// Compact/Pretty keep writing the stringified `extra` field as-is.
fn make_box_writer<M>(make_writer: M, format: LogFormat) -> BoxMakeWriter
where
    M: for<'a> MakeWriter<'a> + Send + Sync + 'static,
{
    if format == LogFormat::Json {
        BoxMakeWriter::new(AttributeInjectingMakeWriter { inner: make_writer })
    } else {
        BoxMakeWriter::new(make_writer)
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
    ($level:expr, $msg:expr, $location:expr, $extra:expr, $spans:expr) => {{
        // `spans` is `Option<&str>`, which implements `tracing`'s `Value` directly
        // (`impl<T: Value> Value for Option<T>`, tracing-core's field.rs) -- unlike
        // `location`/`extra`, it doesn't need a match arm per presence combination.
        macro_rules! emit {
            ($lvl:ident) => {
                match ($location, $extra) {
                    (Some(loc), Some(e)) => {
                        tracing::$lvl!(location = %loc, extra = %e, spans = $spans, "{}", $msg)
                    }
                    (Some(loc), None) => tracing::$lvl!(location = %loc, spans = $spans, "{}", $msg),
                    (None, Some(e)) => tracing::$lvl!(extra = %e, spans = $spans, "{}", $msg),
                    (None, None) => tracing::$lvl!(spans = $spans, "{}", $msg),
                }
            };
        }
        match $level {
            0..=9 => emit!(trace),
            10..=19 => emit!(debug),
            20..=29 => emit!(info),
            30..=39 => emit!(warn),
            _ => emit!(error),
        }
    }};
}

/// Dispatches a single log line to `tracing`, given already-formatted metadata.
///
/// `attributes`/`spans_json`, when set, are handed to JSON-format layers as typed
/// nested values (see `AttributeInjectingWriter`) instead of going through `extra`'s
/// and `spans`'s stringified forms. `exception`, when set, is JSON-only -- text
/// formats already carry the traceback embedded in `message` (see
/// `CURRENT_EXCEPTION`).
// Each argument mirrors one independent field of Python's `LogRecord`; there's no
// natural subgrouping that wouldn't just be a struct wrapping this same fixed list.
#[allow(clippy::too_many_arguments)]
pub fn log_sink(
    levelno: u8,
    message: &str,
    filename: Option<String>,
    func_name: Option<String>,
    lineno: Option<usize>,
    module_name: Option<String>,
    extra: Option<&str>,
    attributes: Option<serde_json::Value>,
    spans: Option<&str>,
    spans_json: Option<serde_json::Value>,
    exception: Option<serde_json::Value>,
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

    if attributes.is_some() {
        CURRENT_ATTRIBUTES.with(|cell| *cell.borrow_mut() = attributes);
    }
    if spans_json.is_some() {
        CURRENT_SPANS.with(|cell| *cell.borrow_mut() = spans_json);
    }
    if exception.is_some() {
        CURRENT_EXCEPTION.with(|cell| *cell.borrow_mut() = exception);
    }

    dispatch_log!(levelno, message, location_str, extra, spans);

    CURRENT_ATTRIBUTES.with(|cell| *cell.borrow_mut() = None);
    CURRENT_SPANS.with(|cell| *cell.borrow_mut() = None);
    CURRENT_EXCEPTION.with(|cell| *cell.borrow_mut() = None);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write as _;
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
        assert_eq!(TracingInitError("boom".to_string()).to_string(), "boom");
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
            let (_layer, guard) = build_layer_internal(&config);
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
        let (_layer, guard) = build_layer_internal(&config);
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
                None,
                None,
                None,
                None,
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
                None,
                None,
                None,
                None,
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
                None,
                None,
                None,
                None,
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
                None,
                None,
                None,
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
            log_sink(
                level,
                "test message",
                None,
                None,
                None,
                None,
                None,
                None,
                None,
                None,
                None,
            );

            assert!(logs_contain(&name));
            assert!(logs_contain("test message"));
            assert!(!logs_contain("location="));
            assert!(!logs_contain("extra="));
        }
    }

    #[test]
    fn test_inject_attributes_merges_typed_value_and_drops_extra() {
        CURRENT_ATTRIBUTES.with(|cell| {
            *cell.borrow_mut() = Some(serde_json::json!({"user_id": 42, "flag": true}));
        });

        let input = br#"{"level":"INFO","message":"hi","extra":"user_id=42, flag=true"}
"#;
        let merged = inject_attributes(input).expect("attributes were pending");
        let value: serde_json::Value = serde_json::from_slice(&merged).unwrap();

        assert_eq!(value["message"], "hi");
        assert_eq!(value["attributes"]["user_id"], 42);
        assert_eq!(value["attributes"]["flag"], true);
        assert!(value.get("extra").is_none());

        CURRENT_ATTRIBUTES.with(|cell| *cell.borrow_mut() = None);
    }

    #[test]
    fn test_inject_attributes_passthrough_when_nothing_pending() {
        CURRENT_ATTRIBUTES.with(|cell| *cell.borrow_mut() = None);
        assert!(inject_attributes(b"{\"message\":\"hi\"}\n").is_none());
    }

    #[test]
    fn test_inject_attributes_passthrough_on_non_json_buf() {
        CURRENT_ATTRIBUTES.with(|cell| {
            *cell.borrow_mut() = Some(serde_json::json!({"a": 1}));
        });
        assert!(inject_attributes(b"not json\n").is_none());
        CURRENT_ATTRIBUTES.with(|cell| *cell.borrow_mut() = None);
    }

    #[test]
    fn test_attribute_injecting_writer_delegates_when_nothing_pending() {
        CURRENT_ATTRIBUTES.with(|cell| *cell.borrow_mut() = None);
        let mut out = Vec::new();
        let mut writer = AttributeInjectingWriter { inner: &mut out };
        writer.write_all(b"{\"message\":\"hi\"}\n").unwrap();
        assert_eq!(out, b"{\"message\":\"hi\"}\n");
    }

    #[test]
    fn test_json_layer_emits_typed_attributes_and_compact_layer_is_unaffected() {
        use std::sync::{Arc, Mutex};

        #[derive(Clone, Default)]
        struct SharedBuf(Arc<Mutex<Vec<u8>>>);

        impl io::Write for SharedBuf {
            fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
                self.0.lock().unwrap().write(buf)
            }
            fn flush(&mut self) -> io::Result<()> {
                Ok(())
            }
        }

        impl<'a> MakeWriter<'a> for SharedBuf {
            type Writer = SharedBuf;

            fn make_writer(&'a self) -> Self::Writer {
                self.clone()
            }
        }

        let json_buf = SharedBuf::default();
        let json_writer = make_box_writer(json_buf.clone(), LogFormat::Json);
        let json_layer = build_fmt_layer(json_writer, LogFormat::Json, FmtSpan::NONE, false);

        let compact_buf = SharedBuf::default();
        let compact_writer = make_box_writer(compact_buf.clone(), LogFormat::Compact);
        let compact_layer =
            build_fmt_layer(compact_writer, LogFormat::Compact, FmtSpan::NONE, false);

        let subscriber = Registry::default().with(vec![json_layer, compact_layer]);
        let _guard = tracing::subscriber::set_default(subscriber);

        log_sink(
            20,
            "hi",
            None,
            None,
            None,
            None,
            Some("user_id=42"),
            Some(serde_json::json!({"user_id": 42})),
            None,
            None,
            None,
        );

        let json_output = String::from_utf8(json_buf.0.lock().unwrap().clone()).unwrap();
        let json_value: serde_json::Value = serde_json::from_str(json_output.trim()).unwrap();
        assert_eq!(json_value["attributes"]["user_id"], 42);
        assert!(json_value.get("extra").is_none());

        let compact_output = String::from_utf8(compact_buf.0.lock().unwrap().clone()).unwrap();
        assert!(compact_output.contains("extra=user_id=42"));
    }

    #[test]
    fn test_json_layer_emits_typed_spans_and_compact_layer_gets_text() {
        use std::sync::{Arc, Mutex};

        #[derive(Clone, Default)]
        struct SharedBuf(Arc<Mutex<Vec<u8>>>);

        impl io::Write for SharedBuf {
            fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
                self.0.lock().unwrap().write(buf)
            }
            fn flush(&mut self) -> io::Result<()> {
                Ok(())
            }
        }

        impl<'a> MakeWriter<'a> for SharedBuf {
            type Writer = SharedBuf;

            fn make_writer(&'a self) -> Self::Writer {
                self.clone()
            }
        }

        let json_buf = SharedBuf::default();
        let json_writer = make_box_writer(json_buf.clone(), LogFormat::Json);
        let json_layer = build_fmt_layer(json_writer, LogFormat::Json, FmtSpan::NONE, false);

        let compact_buf = SharedBuf::default();
        let compact_writer = make_box_writer(compact_buf.clone(), LogFormat::Compact);
        let compact_layer =
            build_fmt_layer(compact_writer, LogFormat::Compact, FmtSpan::NONE, false);

        let subscriber = Registry::default().with(vec![json_layer, compact_layer]);
        let _guard = tracing::subscriber::set_default(subscriber);

        log_sink(
            20,
            "hi",
            None,
            None,
            None,
            None,
            None,
            None,
            Some("request{request_id=42}"),
            Some(serde_json::json!([{"name": "request", "fields": {"request_id": 42}}])),
            None,
        );

        let json_output = String::from_utf8(json_buf.0.lock().unwrap().clone()).unwrap();
        let json_value: serde_json::Value = serde_json::from_str(json_output.trim()).unwrap();
        assert_eq!(json_value["spans"][0]["name"], "request");
        assert_eq!(json_value["spans"][0]["fields"]["request_id"], 42);

        // `spans` is recorded as a raw (unwrapped) `Option<&str>` field rather than via
        // `%` (Display) -- necessary so it's genuinely omitted when `None` rather than
        // rendered as an empty string (which would leak a bogus `"spans":""` into JSON
        // output on every span-less log line). The trade-off: `tracing-subscriber`'s
        // default text visitor debug-quotes raw `&str` fields, unlike `%`-wrapped ones.
        let compact_output = String::from_utf8(compact_buf.0.lock().unwrap().clone()).unwrap();
        assert!(compact_output.contains(r#"spans="request{request_id=42}""#));
    }

    #[test]
    fn test_json_layer_emits_typed_exception_and_compact_layer_is_unaffected() {
        use std::sync::{Arc, Mutex};

        #[derive(Clone, Default)]
        struct SharedBuf(Arc<Mutex<Vec<u8>>>);

        impl io::Write for SharedBuf {
            fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
                self.0.lock().unwrap().write(buf)
            }
            fn flush(&mut self) -> io::Result<()> {
                Ok(())
            }
        }

        impl<'a> MakeWriter<'a> for SharedBuf {
            type Writer = SharedBuf;

            fn make_writer(&'a self) -> Self::Writer {
                self.clone()
            }
        }

        let json_buf = SharedBuf::default();
        let json_writer = make_box_writer(json_buf.clone(), LogFormat::Json);
        let json_layer = build_fmt_layer(json_writer, LogFormat::Json, FmtSpan::NONE, false);

        let compact_buf = SharedBuf::default();
        let compact_writer = make_box_writer(compact_buf.clone(), LogFormat::Compact);
        let compact_layer =
            build_fmt_layer(compact_writer, LogFormat::Compact, FmtSpan::NONE, false);

        let subscriber = Registry::default().with(vec![json_layer, compact_layer]);
        let _guard = tracing::subscriber::set_default(subscriber);

        log_sink(
            40,
            "boom happened\nTraceback (most recent call last):\nValueError: oops",
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            Some(serde_json::json!({
                "type": "ValueError",
                "message": "oops",
                "traceback": "Traceback (most recent call last):\nValueError: oops",
            })),
        );

        let json_output = String::from_utf8(json_buf.0.lock().unwrap().clone()).unwrap();
        let json_value: serde_json::Value = serde_json::from_str(json_output.trim()).unwrap();
        assert_eq!(json_value["exception"]["type"], "ValueError");
        assert_eq!(json_value["exception"]["message"], "oops");

        let compact_output = String::from_utf8(compact_buf.0.lock().unwrap().clone()).unwrap();
        assert!(!compact_output.contains("exception"));
    }
}
