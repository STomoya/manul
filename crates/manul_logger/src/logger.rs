use std::cell::RefCell;
use std::collections::HashMap;
use std::io;
use std::str::FromStr;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Mutex, Once, OnceLock};

use tracing_appender::non_blocking::WorkerGuard;
use tracing_appender::rolling::{RollingFileAppender, Rotation};
use tracing_subscriber::{
    EnvFilter, Layer, Registry,
    filter::{FilterExt, dynamic_filter_fn},
    fmt::{
        self,
        format::FmtSpan,
        writer::{BoxMakeWriter, MakeWriter},
    },
    layer::SubscriberExt,
    reload,
    util::SubscriberInitExt,
};

static INIT: Once = Once::new();

/// A per-layer filter that can be swapped at runtime via `set_filter`.
type FilterHandle = reload::Handle<EnvFilter, Registry>;

/// Layer name -> its reload handle, populated once `init_tracing` successfully
/// installs the global subscriber. Layers are looked up by the `name` given in
/// their `LayerConfig`.
static FILTER_HANDLES: OnceLock<Mutex<HashMap<String, FilterHandle>>> = OnceLock::new();

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
    pub max_log_files: Option<usize>,
    pub sample_directive: Option<String>,
    pub use_local_time: bool,
}

impl LayerConfig {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        name: String,
        filter_directive: String,
        format: LogFormat,
        destination: LayerDestination,
        file_dir: Option<String>,
        file_prefix: Option<String>,
        include_span_events: bool,
        max_log_files: Option<usize>,
        sample_directive: Option<String>,
        use_local_time: bool,
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
            sample_directive,
            use_local_time,
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
    let mut filter_handles = HashMap::new();

    for config in &layers {
        let (layer, guard, filter_handle) = build_layer_internal(config);
        subscriber_layers.push(layer);
        if let Some(g) = guard {
            guards.push(g);
        }
        filter_handles.insert(config.name.clone(), filter_handle);
        initialized_info.push((config.name.clone(), config.filter_directive.clone()));
    }

    let mut init_err = None;
    INIT.call_once(|| {
        if let Err(e) = Registry::default().with(subscriber_layers).try_init() {
            init_err = Some(e.to_string());
        } else {
            let _ = FILTER_HANDLES.set(Mutex::new(filter_handles));
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

/// Decimates events for one level within one currently-open named span, so a
/// hot/noisy call site can be thinned out without affecting unrelated events
/// at the same level. The span stack it reads is `CURRENT_SPANS`, whose JSON
/// shape comes from `manul.logger._functions._current_spans_payload` -- keep
/// the `"name"` key in sync with that function if it ever changes.
struct Sampler {
    span_name: String,
    level: tracing::Level,
    n: usize,
    counter: AtomicUsize,
}

impl Sampler {
    fn new(directive: &str) -> Self {
        let (span_name, level, n) = parse_sample_directive(directive);
        Self {
            span_name,
            level,
            n,
            counter: AtomicUsize::new(0),
        }
    }

    fn should_emit(&self, level: &tracing::Level) -> bool {
        if level != &self.level {
            return true;
        }
        let in_scope = CURRENT_SPANS.with(|cell| {
            cell.borrow()
                .as_ref()
                .and_then(|value| value.as_array())
                .is_some_and(|frames| {
                    frames.iter().any(|frame| {
                        frame.get("name").and_then(|name| name.as_str())
                            == Some(self.span_name.as_str())
                    })
                })
        });
        if !in_scope {
            return true;
        }
        self.counter
            .fetch_add(1, Ordering::Relaxed)
            .is_multiple_of(self.n)
    }
}

/// Parses a `"<span_name>:<level>:<n>"` sample directive. Panics on malformed
/// input, matching `EnvFilter::new`'s own panic-on-bad-syntax behavior for
/// `filter_directive` -- this is programmer-set config, not user input.
fn parse_sample_directive(directive: &str) -> (String, tracing::Level, usize) {
    let parts: Vec<&str> = directive.rsplitn(3, ':').collect();
    let [n, level, span_name] = parts[..] else {
        panic!("Invalid sample directive \"{directive}\": expected \"<span_name>:<level>:<n>\"");
    };
    let level: tracing::Level = level.parse().unwrap_or_else(|_| {
        panic!("Invalid sample directive \"{directive}\": unknown level \"{level}\"")
    });
    let n: usize = n.parse().unwrap_or_else(|_| {
        panic!("Invalid sample directive \"{directive}\": \"{n}\" is not a positive integer")
    });
    assert!(
        n > 0,
        "Invalid sample directive \"{directive}\": n must be > 0"
    );
    (span_name.to_string(), level, n)
}

type ReloadableFilter = reload::Layer<EnvFilter, Registry>;

/// Applies the (possibly runtime-reloadable) `EnvFilter`, combined with a
/// span-scoped sampling decimator when `config.sample_directive` is set.
fn with_layer_filter(layer: LogLayer, config: &LayerConfig, filter: ReloadableFilter) -> LogLayer {
    match &config.sample_directive {
        Some(directive) => {
            let sampler = Sampler::new(directive);
            // `filter_fn` assumes a `Metadata`-only decision is cacheable per callsite
            // (`Interest::always`/`never`) and would only ever evaluate the sampler once.
            // `dynamic_filter_fn` defaults to `Interest::sometimes()`, so it's re-run for
            // every event, which the sampler's per-event counter needs.
            let sampling_filter =
                dynamic_filter_fn(move |metadata, _ctx| sampler.should_emit(metadata.level()));
            layer.with_filter(filter.and(sampling_filter)).boxed()
        }
        None => layer.with_filter(filter).boxed(),
    }
}

/// Builds a single layer based on configuration.
fn build_layer_internal(config: &LayerConfig) -> (LogLayer, Option<WorkerGuard>, FilterHandle) {
    let env_filter = EnvFilter::new(&config.filter_directive);
    let (reloadable_filter, filter_handle) = reload::Layer::new(env_filter);
    let span_events = if config.include_span_events {
        FmtSpan::CLOSE
    } else {
        FmtSpan::NONE
    };

    match config.destination {
        LayerDestination::Console => {
            let writer = make_box_writer(std::io::stdout, config.format);
            let layer = build_fmt_layer(
                writer,
                config.format,
                span_events,
                true,
                config.use_local_time,
            );
            (
                with_layer_filter(layer, config, reloadable_filter),
                None,
                filter_handle,
            )
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

            let mut builder = RollingFileAppender::builder()
                .rotation(Rotation::DAILY)
                .filename_prefix(prefix);
            if let Some(max_log_files) = config.max_log_files {
                builder = builder.max_log_files(max_log_files);
            }
            let file_appender = builder
                .build(dir)
                .expect("initializing rolling file appender failed");
            let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);

            let writer = make_box_writer(non_blocking, config.format);
            let layer = build_fmt_layer(
                writer,
                config.format,
                span_events,
                false,
                config.use_local_time,
            );
            (
                with_layer_filter(layer, config, reloadable_filter),
                Some(guard),
                filter_handle,
            )
        }
    }
}

/// Error returned when adjusting a layer's filter at runtime fails.
#[derive(Debug)]
pub struct FilterUpdateError(String);

impl std::fmt::Display for FilterUpdateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Change a layer's filter directive at runtime, without restarting the process.
///
/// `layer_name` must match a `name` given to one of the `LayerConfig`s passed to
/// `init_tracing`.
pub fn set_filter(layer_name: &str, filter_directive: &str) -> Result<(), FilterUpdateError> {
    let handles = FILTER_HANDLES
        .get()
        .ok_or_else(|| FilterUpdateError("Tracing has not been initialized.".to_string()))?
        .lock()
        .expect("filter handles mutex poisoned");

    let handle = handles
        .get(layer_name)
        .ok_or_else(|| FilterUpdateError(format!("Unknown layer: \"{layer_name}\"")))?;

    let filter =
        EnvFilter::try_new(filter_directive).map_err(|e| FilterUpdateError(e.to_string()))?;
    handle
        .reload(filter)
        .map_err(|e| FilterUpdateError(e.to_string()))
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

/// Erases the concrete timer type, so `build_fmt_layer` can pick UTC or local time at
/// runtime without duplicating each format's whole builder chain per timezone.
struct BoxedTimer(Box<dyn fmt::time::FormatTime + Send + Sync>);

impl fmt::time::FormatTime for BoxedTimer {
    fn format_time(&self, w: &mut fmt::format::Writer<'_>) -> std::fmt::Result {
        self.0.format_time(w)
    }
}

/// UTC is the de-facto standard for application logs (no DST ambiguity, correlates
/// cleanly across services); local time is opt-in via `LayerConfig.use_local_time`.
fn build_timer(use_local_time: bool) -> BoxedTimer {
    if use_local_time {
        BoxedTimer(Box::new(fmt::time::LocalTime::rfc_3339()))
    } else {
        BoxedTimer(Box::new(fmt::time::UtcTime::rfc_3339()))
    }
}

/// Configures the formatting layer with common production settings.
fn build_fmt_layer(
    writer: BoxMakeWriter,
    format: LogFormat,
    span_events: FmtSpan,
    ansi: bool,
    use_local_time: bool,
) -> LogLayer {
    let timer = build_timer(use_local_time);
    match format {
        LogFormat::Json => fmt::layer()
            .with_timer(timer)
            .with_writer(writer)
            .with_ansi(ansi)
            .with_span_events(span_events)
            .json()
            .flatten_event(true)
            .with_current_span(true)
            .with_target(false)
            .boxed(),
        LogFormat::Pretty => fmt::layer()
            .with_timer(timer)
            .with_writer(writer)
            .with_ansi(ansi)
            .with_span_events(span_events)
            .pretty()
            .with_target(false)
            .boxed(),
        LogFormat::Compact => fmt::layer()
            .with_timer(timer)
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
    use tracing_subscriber::fmt::time::FormatTime;
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
                None,
                None,
                false,
            );
            let (_layer, guard, _handle) = build_layer_internal(&config);
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
            None,
            None,
            false,
        );
        let (_layer, guard, _handle) = build_layer_internal(&config);
        assert!(guard.is_some());
    }

    #[test]
    fn test_build_layer_internal_file_with_max_log_files_cap() {
        let dir = std::env::temp_dir().join(format!(
            "manul_logger_test_max_log_files_{}",
            std::process::id()
        ));
        let config = LayerConfig::new(
            "file_layer".to_string(),
            "debug".to_string(),
            LogFormat::Json,
            LayerDestination::File,
            Some(dir.to_string_lossy().into_owned()),
            Some("test_app".to_string()),
            false,
            Some(2),
            None,
            false,
        );
        let (_layer, guard, _handle) = build_layer_internal(&config);
        assert!(guard.is_some());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_filter_update_error_display() {
        assert_eq!(FilterUpdateError("boom".to_string()).to_string(), "boom");
    }

    #[test]
    fn test_reload_handle_from_build_layer_internal_changes_what_is_captured() {
        let dir =
            std::env::temp_dir().join(format!("manul_logger_test_reload_{}", std::process::id()));
        let config = LayerConfig::new(
            "reloadable".to_string(),
            "off".to_string(),
            LogFormat::Compact,
            LayerDestination::File,
            Some(dir.to_string_lossy().into_owned()),
            Some("test".to_string()),
            false,
            None,
            None,
            false,
        );
        let (layer, guard, handle) = build_layer_internal(&config);
        let subscriber = Registry::default().with(layer);
        let default_guard = tracing::subscriber::set_default(subscriber);

        log_sink(
            20,
            "should be filtered out",
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

        handle
            .reload(EnvFilter::new("info"))
            .expect("reload should succeed while the subscriber is alive");

        log_sink(
            20,
            "should be captured",
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

        drop(default_guard);
        drop(guard); // Flush the non-blocking writer's buffered output.

        let mut contents = String::new();
        for entry in std::fs::read_dir(&dir).expect("log dir should exist") {
            contents.push_str(&std::fs::read_to_string(entry.unwrap().path()).unwrap());
        }

        assert!(!contents.contains("should be filtered out"));
        assert!(contents.contains("should be captured"));

        let _ = std::fs::remove_dir_all(&dir);
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
        let json_layer = build_fmt_layer(json_writer, LogFormat::Json, FmtSpan::NONE, false, false);

        let compact_buf = SharedBuf::default();
        let compact_writer = make_box_writer(compact_buf.clone(), LogFormat::Compact);
        let compact_layer = build_fmt_layer(
            compact_writer,
            LogFormat::Compact,
            FmtSpan::NONE,
            false,
            false,
        );

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
        let json_layer = build_fmt_layer(json_writer, LogFormat::Json, FmtSpan::NONE, false, false);

        let compact_buf = SharedBuf::default();
        let compact_writer = make_box_writer(compact_buf.clone(), LogFormat::Compact);
        let compact_layer = build_fmt_layer(
            compact_writer,
            LogFormat::Compact,
            FmtSpan::NONE,
            false,
            false,
        );

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
        let json_layer = build_fmt_layer(json_writer, LogFormat::Json, FmtSpan::NONE, false, false);

        let compact_buf = SharedBuf::default();
        let compact_writer = make_box_writer(compact_buf.clone(), LogFormat::Compact);
        let compact_layer = build_fmt_layer(
            compact_writer,
            LogFormat::Compact,
            FmtSpan::NONE,
            false,
            false,
        );

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

    #[test]
    fn test_parse_sample_directive_parses_valid_directive() {
        let (span_name, level, n) = parse_sample_directive("db_query:debug:20");
        assert_eq!(span_name, "db_query");
        assert_eq!(level, tracing::Level::DEBUG);
        assert_eq!(n, 20);
    }

    #[test]
    fn test_parse_sample_directive_preserves_colons_in_span_name() {
        let (span_name, level, n) = parse_sample_directive("db:query:debug:20");
        assert_eq!(span_name, "db:query");
        assert_eq!(level, tracing::Level::DEBUG);
        assert_eq!(n, 20);
    }

    #[test]
    #[should_panic(expected = "expected \"<span_name>:<level>:<n>\"")]
    fn test_parse_sample_directive_panics_on_missing_parts() {
        parse_sample_directive("db_query:debug");
    }

    #[test]
    #[should_panic(expected = "unknown level")]
    fn test_parse_sample_directive_panics_on_unknown_level() {
        parse_sample_directive("db_query:noisy:20");
    }

    #[test]
    #[should_panic(expected = "not a positive integer")]
    fn test_parse_sample_directive_panics_on_invalid_n() {
        parse_sample_directive("db_query:debug:many");
    }

    #[test]
    #[should_panic(expected = "n must be > 0")]
    fn test_parse_sample_directive_panics_on_zero_n() {
        parse_sample_directive("db_query:debug:0");
    }

    #[test]
    fn test_sampler_should_emit_passes_through_other_levels() {
        let sampler = Sampler::new("db_query:debug:2");
        CURRENT_SPANS.with(|cell| {
            *cell.borrow_mut() = Some(serde_json::json!([{"name": "db_query", "fields": {}}]));
        });
        assert!(sampler.should_emit(&tracing::Level::INFO));
        assert!(sampler.should_emit(&tracing::Level::INFO));
        CURRENT_SPANS.with(|cell| *cell.borrow_mut() = None);
    }

    #[test]
    fn test_sampler_should_emit_passes_through_outside_the_span() {
        let sampler = Sampler::new("db_query:debug:2");
        assert!(sampler.should_emit(&tracing::Level::DEBUG));
        assert!(sampler.should_emit(&tracing::Level::DEBUG));
        assert!(sampler.should_emit(&tracing::Level::DEBUG));
    }

    #[test]
    fn test_sampler_should_emit_decimates_within_the_span() {
        let sampler = Sampler::new("db_query:debug:3");
        CURRENT_SPANS.with(|cell| {
            *cell.borrow_mut() = Some(serde_json::json!([{"name": "db_query", "fields": {}}]));
        });
        let kept = (0..9)
            .filter(|_| sampler.should_emit(&tracing::Level::DEBUG))
            .count();
        CURRENT_SPANS.with(|cell| *cell.borrow_mut() = None);
        assert_eq!(kept, 3); // 1-in-3 of 9 events
    }

    #[test]
    fn test_with_layer_filter_thins_events_within_the_sampled_span_only() {
        use std::sync::Arc;

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

        let buf = SharedBuf::default();
        let writer = make_box_writer(buf.clone(), LogFormat::Compact);
        let layer = build_fmt_layer(writer, LogFormat::Compact, FmtSpan::NONE, false, false);

        let config = LayerConfig::new(
            "sampled".to_string(),
            "trace".to_string(),
            LogFormat::Compact,
            LayerDestination::Console,
            None,
            None,
            false,
            None,
            Some("db_query:debug:5".to_string()),
            false,
        );
        let (reloadable_filter, _handle) =
            reload::Layer::new(EnvFilter::new(&config.filter_directive));
        let filtered = with_layer_filter(layer, &config, reloadable_filter);

        let subscriber = Registry::default().with(filtered);
        let _guard = tracing::subscriber::set_default(subscriber);

        let in_scope_spans = Some(serde_json::json!([{"name": "db_query", "fields": {}}]));
        for i in 0..10 {
            log_sink(
                10,
                &format!("in-span debug {i}"),
                None,
                None,
                None,
                None,
                None,
                None,
                None,
                in_scope_spans.clone(),
                None,
            );
        }
        for i in 0..3 {
            log_sink(
                10,
                &format!("no-span debug {i}"),
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
        }
        for i in 0..3 {
            log_sink(
                20,
                &format!("in-span info {i}"),
                None,
                None,
                None,
                None,
                None,
                None,
                None,
                in_scope_spans.clone(),
                None,
            );
        }

        let output = String::from_utf8(buf.0.lock().unwrap().clone()).unwrap();
        assert_eq!(output.matches("in-span debug").count(), 2); // keeps 1-in-5 of 10
        assert_eq!(output.matches("no-span debug").count(), 3); // untouched: not in the span
        assert_eq!(output.matches("in-span info").count(), 3); // untouched: wrong level
    }

    #[test]
    fn test_build_timer_defaults_to_utc_rfc3339() {
        let timer = build_timer(false);
        let mut buf = String::new();
        let mut writer = fmt::format::Writer::new(&mut buf);
        timer.format_time(&mut writer).unwrap();
        // UtcTime::rfc_3339() always renders a zero UTC offset as "Z", regardless of the
        // host machine's local timezone -- see `time`'s Rfc3339 formatter.
        assert!(
            buf.ends_with('Z'),
            "expected an RFC3339 UTC timestamp, got {buf:?}"
        );
    }

    #[test]
    fn test_build_timer_local_time_is_selectable_without_panicking() {
        let timer = build_timer(true);
        let mut buf = String::new();
        let mut writer = fmt::format::Writer::new(&mut buf);
        // `time`'s local-offset lookup can refuse to run in a multi-threaded process (a
        // soundness guard, not a bug here) and return an error instead of a timestamp --
        // this only proves the `true` path is wired up to a different timer and doesn't
        // panic, not what the local offset actually is.
        let _ = timer.format_time(&mut writer);
    }
}
