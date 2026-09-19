"""Handler."""

import copy
import logging
import logging.handlers
import traceback
from logging import Handler

from manul.logger._functions import _current_spans_payload, log_sink

_STANDARD_ATTRS = {
    '_spans',
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

    def emit(self, record: logging.LogRecord) -> None:
        """Emit a record.

        Args:
            record (logging.LogRecord): The log record to emit.

        """
        try:
            message = self.format(record)
            extra_fields = {k: v for k, v in record.__dict__.items() if k not in _STANDARD_ATTRS}
            if not extra_fields:
                extra_fields = None

            exception = None
            if record.exc_info:
                exc_type, exc_value, exc_tb = record.exc_info
                if exc_type is not None:
                    exception = {
                        'type': exc_type.__name__,
                        'message': str(exc_value),
                        'traceback': ''.join(traceback.format_exception(exc_type, exc_value, exc_tb)),
                    }

            log_sink(
                levelno=record.levelno,
                message=message,
                filename=record.pathname,
                func_name=record.funcName,
                lineno=record.lineno,
                module_name=record.module,
                extra=extra_fields,
                exception=exception,
                spans=getattr(record, '_spans', None),
            )
        except Exception:
            self.handleError(record)


class TracingQueueHandler(logging.handlers.QueueHandler):
    """A `QueueHandler` that preserves `exc_info` and the span stack across the queue hop.

    Pairs with `TracingHandler` via a `logging.handlers.QueueListener` to move
    logging work off the calling thread. The stdlib `QueueHandler.prepare()`
    clears `exc_info`/`exc_text` before enqueuing a record -- necessary for a
    `multiprocessing.Queue`, since traceback objects aren't picklable -- but it
    also silently drops `TracingHandler`'s structured JSON `exception` field
    once the record reaches the listener thread. This subclass keeps
    `exc_info` intact instead, so only use it with an in-process `queue.Queue`,
    never a `multiprocessing.Queue`.

    It also snapshots the calling thread/task's span stack onto the record, since
    `_current_spans` is a `ContextVar` that wouldn't otherwise survive the hop to the
    listener thread.
    """

    def prepare(self, record: logging.LogRecord) -> logging.LogRecord:
        """Merge `args` into the message, and snapshot spans, but leave `exc_info` for the listener's handler."""
        record = copy.copy(record)
        record.message = record.getMessage()
        record.msg = record.message
        record.args = None
        # _current_spans is a ContextVar: it only exists on the calling thread, so it
        # must be captured here rather than re-read once the record reaches the
        # QueueListener thread, where it would evaluate to an empty stack.
        record._spans = _current_spans_payload()
        return record
