"""Handler."""

import logging
from logging import Handler

from manul.logger._functions import log_sink

_STANDARD_ATTRS = {
    'args',
    'asctime',
    'created',
    'exc_info',
    'exc_text',
    'filename',
    'funcName',
    'levelname',
    'levelno',
    'lineno',
    'module',
    'msecs',
    'message',
    'msg',
    'name',
    'pathname',
    'process',
    'processName',
    'relativeCreated',
    'stack_info',
    'thread',
    'threadName',
    'taskName',
}


class TracingHandler(Handler):
    """A logging handler that forwards Python logs to the Rust tracing system."""

    def emit(self, record: logging.LogRecord):
        """Emit a record.

        Args:
            record (logging.LogRecord): The log record to emit.

        """
        try:
            message = self.format(record)
            extra_fields = {str(k): str(v) for k, v in record.__dict__.items() if k not in _STANDARD_ATTRS}
            if not extra_fields:
                extra_fields = None

            log_sink(
                levelno=record.levelno,
                message=message,
                filename=record.pathname,
                func_name=record.funcName,
                lineno=record.lineno,
                module_name=record.module,
                extra=extra_fields,
            )
        except Exception:
            self.handleError(record)
