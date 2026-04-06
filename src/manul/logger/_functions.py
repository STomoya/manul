"""Logger."""

from typing import Any, Literal, TypeVar

from manul._manul import _logger  # ty: ignore[unresolved-import]

ConfigT = TypeVar('ConfigT', bound=_logger.LayerConfig)


def build_layer_config(
    name: str,
    filter_directive: str,
    format: Literal['compact', 'pretty', 'json'] | None = None,
    destination: Literal['console', 'file'] | None = None,
    file_dir: str | None = None,
    file_prefix: str | None = None,
    include_span_events: bool = False,
) -> ConfigT:
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
        ConfigT: The layer configuration.

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


def init_tracing(layers: list[ConfigT]) -> _logger.TracingGuard:
    """Initialize the tracing system.

    Args:
        layers (list[ConfigT]): A list of layer configurations.

    Returns:
        _logger.TracingGuard:

    """
    return _logger.init_tracing(layers)


def info(message: str, extra: dict[str, Any] | None = None) -> None:
    """Info level log.

    Args:
        message (str): Message to log.
        extra (dict[str, Any]): Extra data to log.

    """
    _logger.info(message=message, extra=extra)


def debug(message: str, extra: dict[str, Any] | None = None) -> None:
    """Debug level log.

    Args:
        message (str): Message to log.
        extra (dict[str, Any]): Extra data to log.

    """
    _logger.debug(message=message, extra=extra)


def warn(message: str, extra: dict[str, Any] | None = None) -> None:
    """Warn level log.

    Args:
        message (str): Message to log.
        extra (dict[str, Any]): Extra data to log.

    """
    _logger.warn(message=message, extra=extra)


def error(message: str, extra: dict[str, Any] | None = None) -> None:
    """Error level log.

    Args:
        message (str): Message to log.
        extra (dict[str, Any]): Extra data to log.

    """
    _logger.error(message=message, extra=extra)


def trace(message: str, extra: dict[str, Any] | None = None) -> None:
    """Trace level log.

    Args:
        message (str): Message to log.
        extra (dict[str, Any]): Extra data to log.

    """
    _logger.trace(message=message, extra=extra)


def log_sink(
    levelno: int,
    message: str,
    filename: str,
    func_name: str,
    lineno: int,
    module_name: str,
    extra: dict | None = None,
):
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
