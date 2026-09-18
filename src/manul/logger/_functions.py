"""Logger."""

from __future__ import annotations

import sys
import time
from contextvars import ContextVar
from dataclasses import dataclass
from pathlib import Path
from typing import TYPE_CHECKING, Literal, Self, TypeVar

from manul._manul import _logger

if TYPE_CHECKING:
    from collections.abc import Callable
    from contextvars import Token
    from types import TracebackType

_T = TypeVar('_T')


def _resolve_enum(value: str | None, enum_cls: Callable[[str], _T], default: _T) -> _T:
    """Resolve a string option into a pyo3 enum member, falling back to a default.

    Args:
        value (str | None): The raw string option, or None to use the default.
        enum_cls (Callable[[str], _T]): The pyo3 enum class to construct from `value`.
        default (_T): The enum member to use when `value` is None.

    Returns:
        _T: The resolved enum member.

    """
    return enum_cls(value.lower()) if value is not None else default


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
    format_enum = _resolve_enum(format, _logger.LogFormat, _logger.LogFormat.Compact)
    destination_enum = _resolve_enum(destination, _logger.LayerDestination, _logger.LayerDestination.Console)

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


@dataclass(frozen=True, slots=True)
class _SpanFrame:
    """One entry in the current span stack."""

    name: str
    fields: dict


# Tracked via a ContextVar rather than a real tracing span: tracing's own span stack
# is an OS-thread-local, so holding one across an `await` would leak into whichever
# unrelated coroutine the event loop happens to interleave on the same thread. A
# ContextVar is copied per-asyncio.Task and correctly restored across await points,
# and is also correctly isolated per-thread for plain sync code.
_current_spans: ContextVar[tuple[_SpanFrame, ...]] = ContextVar('_current_spans', default=())


class SpanContext:
    """A tracing span, usable as a sync or async context manager.

    See `_current_spans` for why this doesn't use a real `tracing` span.
    """

    __slots__ = ('_frame', '_location', '_start', '_token')

    _location: tuple[str, str, int, str]
    _start: float
    _token: Token[tuple[_SpanFrame, ...]]

    def __init__(self, name: str, fields: dict) -> None:
        self._frame = _SpanFrame(name, fields)
        # Attribute the close event to wherever `span(...)` was opened, same as
        # `_log` does for direct trace/debug/info/warn/error calls.
        caller = sys._getframe(2)
        self._location = (
            caller.f_code.co_filename,
            caller.f_code.co_name,
            caller.f_lineno,
            Path(caller.f_code.co_filename).stem,
        )

    def __enter__(self) -> Self:
        self._token = _current_spans.set((*_current_spans.get(), self._frame))
        self._start = time.monotonic()
        return self

    def __exit__(
        self,
        exc_type: type[BaseException] | None,
        exc_value: BaseException | None,
        traceback: TracebackType | None,
    ) -> None:
        duration_ms = (time.monotonic() - self._start) * 1000
        filename, func_name, lineno, module_name = self._location
        log_sink(
            levelno=_LEVELS['debug'],
            message=f'{self._frame.name} closed',
            filename=filename,
            func_name=func_name,
            lineno=lineno,
            module_name=module_name,
            extra={'duration_ms': round(duration_ms, 3)},
        )
        _current_spans.reset(self._token)

    async def __aenter__(self) -> Self:
        return self.__enter__()

    async def __aexit__(
        self,
        exc_type: type[BaseException] | None,
        exc_value: BaseException | None,
        traceback: TracebackType | None,
    ) -> None:
        self.__exit__(exc_type, exc_value, traceback)


def span(name: str, **fields: object) -> SpanContext:
    """Open a span, attaching `fields` to every log made while it's open.

    Works as both `with span(...):` and `async with span(...):`.

    Args:
        name (str): The span's name.
        **fields: Arbitrary key/value data attached to the span.

    Returns:
        SpanContext: The span, as a context manager.

    """
    return SpanContext(name, fields)


def _current_spans_payload() -> list[dict] | None:
    """Build the `spans` payload for `_logger._log_sink` from the active span stack."""
    frames = _current_spans.get()
    if not frames:
        return None
    return [{'name': frame.name, 'fields': frame.fields} for frame in frames]


# Mirrors the levelno values manul_pyo3's log_sink dispatches on.
_LEVELS = {'trace': 0, 'debug': 10, 'info': 20, 'warn': 30, 'error': 40}


def _log(level: str, message: str, extra: dict | None) -> None:
    """Log a message, attributing it to the caller of the public level function.

    Args:
        level (str): One of `_LEVELS`' keys.
        message (str): The log message.
        extra (dict | None): Extra data to log.

    """
    # Frame 0 is this function, frame 1 is the public trace/debug/info/warn/error
    # wrapper, frame 2 is their caller -- the callsite we want to report.
    frame = sys._getframe(2)
    log_sink(
        levelno=_LEVELS[level],
        message=message,
        filename=frame.f_code.co_filename,
        func_name=frame.f_code.co_name,
        lineno=frame.f_lineno,
        module_name=Path(frame.f_code.co_filename).stem,
        extra=extra,
    )


def trace(message: str, extra: dict | None = None) -> None:
    """Log a trace-level message."""
    _log('trace', message, extra)


def debug(message: str, extra: dict | None = None) -> None:
    """Log a debug-level message."""
    _log('debug', message, extra)


def info(message: str, extra: dict | None = None) -> None:
    """Log an info-level message."""
    _log('info', message, extra)


def warn(message: str, extra: dict | None = None) -> None:
    """Log a warning-level message."""
    _log('warn', message, extra)


def error(message: str, extra: dict | None = None) -> None:
    """Log an error-level message."""
    _log('error', message, extra)


def log_sink(
    levelno: int,
    message: str,
    filename: str,
    func_name: str,
    lineno: int,
    module_name: str,
    extra: dict | None = None,
    exception: dict | None = None,
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
        exception (dict | None, optional): `{type, message, traceback}` for the
            currently-handled exception, e.g. from `logging`'s `exc_info`. Defaults to
            None.

    """
    _logger._log_sink(
        levelno=levelno,
        message=message,
        filename=filename,
        func_name=func_name,
        lineno=lineno,
        module_name=module_name,
        extra=extra,
        spans=_current_spans_payload(),
        exception=exception,
    )
