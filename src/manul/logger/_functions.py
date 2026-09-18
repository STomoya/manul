"""Logger."""

from typing import Literal

from manul._manul import _logger


def build_layer_config(
    name: str,
    filter_directive: str,
    *,
    format: Literal['compact', 'pretty', 'json'] | None = None,  # noqa: A002 -- mirrors the pyo3 LayerConfig kwarg
    destination: Literal['console', 'file'] | None = None,
    file_dir: str | None = None,
    file_prefix: str | None = None,
    include_span_events: bool = False,
) -> _logger.LayerConfig:
    """Build a layer configuration.

    Args:
        name (str): Friendly name for the layer, used in log messages.
        filter_directive (str): RUST_LOG style filter (e.g. 'info' or 'my_crate=debug').
        format (Literal['compact', 'pretty', 'json'] | None, optional): The log formatting style (compact, pretty, or
            json). Defaults to None.
        destination (Literal['console', 'file'] | None, optional): Where to send logs (Console or File).
            Defaults to None.
        file_dir (str | None, optional): Directory for logs. If None, defaults to "./logs". Defaults to None.
        file_prefix (str | None, optional): Filename prefix for rolling logs. If None, defaults to "app".
            Defaults to None.
        include_span_events (bool, optional): Whether to log timing for span closures. Defaults to False.

    Returns:
        _logger.LayerConfig: The layer configuration.

    """
    if isinstance(format, str):
        format_enum = format.lower()
        format_enum = _logger.LogFormat(format_enum)
    elif format is None:
        format_enum = _logger.LogFormat.Compact

    if isinstance(destination, str):
        destination_enum = destination.lower()
        destination_enum = _logger.LayerDestination(destination_enum)
    elif destination is None:
        destination_enum = _logger.LayerDestination.Console

    return _logger.LayerConfig(
        name=name,
        filter_directive=filter_directive,
        format=format_enum,
        destination=destination_enum,
        file_dir=file_dir,
        file_prefix=file_prefix,
        include_span_events=include_span_events,
    )


def init_tracing(layers: list[_logger.LayerConfig]) -> _logger.TracingGuard:
    """Initialize the tracing system.

    Args:
        layers (list[_logger.LayerConfig]): A list of layer configurations.

    Returns:
        _logger.TracingGuard:

    """
    return _logger.init_tracing(layers)


debug = _logger.debug
error = _logger.error
info = _logger.info
trace = _logger.trace
warn = _logger.warn


def log_sink(
    levelno: int,
    message: str,
    filename: str,
    func_name: str,
    lineno: int,
    module_name: str,
    extra: dict | None = None,
) -> None:
    """Receive log messages from Python and forward them to Rust.

    Args:
        levelno (int): The log level.
        message (str): The log message.
        filename (str): The file name.
        func_name (str): The function name.
        lineno (int): The line number.
        module_name (str): The module name.
        extra (dict | None, optional): Extra data to log. Defaults to None.

    """
    _logger._log_sink(
        levelno=levelno,
        message=message,
        filename=filename,
        func_name=func_name,
        lineno=lineno,
        module_name=module_name,
        extra=extra,
    )
