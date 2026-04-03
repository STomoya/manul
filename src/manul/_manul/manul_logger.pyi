from enum import Enum

__version__: str
"""The version of the manul_logger module."""

class LogFormat(Enum):
    """Enumeration of possible log formats."""

    Compact: LogFormat
    """Log in compact format."""

    Pretty: LogFormat
    """Log in pretty format."""

    Json: LogFormat
    """Log in JSON format."""

    def __init__(self, value: str) -> None: ...

class LayerDestination(Enum):
    """Enumeration of possible layer destinations."""

    Console: LayerDestination
    """Log to the console (stdout)."""

    File: LayerDestination
    """Log to a file. The file path should be specified in the configuration."""

class LayerConfig:
    """Configuration for a single tracing layer."""

    name: str
    """The name of the layer, e.g., "my_crate". """

    filter_directive: str
    """The filter directive for this layer, e.g., "my_crate=info". """

    format: LogFormat
    """The log format for this layer, e.g., "json" or "plain". """

    destination: LayerDestination
    """The destination for this layer, e.g., console or file."""

    file_dir: str
    """The directory for log files if the destination is a file."""

    file_prefix: str
    """The prefix for log files if the destination is a file."""

    include_span_events: bool
    """Whether to include span events (enter/exit) in the logs."""

    def __init__(
        self,
        name: str,
        filter_directive: str,
        format: LogFormat = ...,
        destination: LayerDestination = ...,
        file_dir: str | None = None,
        file_prefix: str | None = None,
        include_span_events: bool = False,
    ) -> None:
        """Create a new LayerConfig.

        Args:
            name (str): Friendly name for the layer, used in log messages.
            filter_directive (str): RUST_LOG style filter (e.g. 'info' or 'my_crate=debug').
            format (LogFormat, optional): The log formatting style (Compact, Pretty, or Json). Defaults to ....
            destination (LayerDestination, optional): Where to send logs (Console or File). Defaults to ....
            file_dir (str | None, optional): Directory for logs (required if destination is File). Defaults to None.
            file_prefix (str | None, optional): Filename prefix for rolling logs. Defaults to None.
            include_span_events (bool, optional): Whether to log timing for span closures. Defaults to False.

        """

class TracingGuard:
    """A guard object that keeps the tracing subscriber active."""

def init_tracing(layers: list[LayerConfig]) -> TracingGuard:
    """Initialize the tracing system."""

def info(message: str) -> None:
    """Log an info-level message."""

def debug(message: str) -> None:
    """Log a debug-level message."""

def warn(message: str) -> None:
    """Log a warning-level message."""

def error(message: str) -> None:
    """Log an error-level message."""

def trace(message: str) -> None:
    """Log a trace-level message."""

def _log_sink(
    levelno: int,
    message: str,
    filename: str,
    func_name: str,
    lineno: int,
    module_name: str,
    extra: dict | None = None,
) -> None:
    """Receive log messages from Python and forward them to Rust."""
