"""Tests for manul.logger module.

Does not test rust code directly, but tests the Python wrapper functions in manul.logger._functions.
Rust calls are mocked to verify that the correct parameters are passed from Python to Rust.
"""

from __future__ import annotations

import asyncio
import logging
import sys
from typing import TYPE_CHECKING, Literal
from unittest.mock import ANY

import pytest

from manul._manul import _logger
from manul.logger import _functions
from manul.logger.handler import TracingHandler

if TYPE_CHECKING:
    from pytest_mock import MockerFixture, MockType


class TestBuildLayerConfig:
    """Tests for the build_layer_config function."""

    @pytest.mark.parametrize(
        ('format', 'expected_format'),
        [
            ('json', _logger.LogFormat.Json),
            ('compact', _logger.LogFormat.Compact),
            (None, _logger.LogFormat.Compact),
        ],
        ids=['json', 'compact', 'default'],
    )
    def test_format(
        self,
        format: Literal['compact', 'pretty', 'json'] | None,  # noqa: A002 -- mirrors the pyo3 LayerConfig kwarg
        expected_format: _logger.LogFormat,
    ) -> None:
        """Test the format parameter."""
        config = _functions.build_layer_config(
            name='test',
            filter_directive='',
            format=format,
            destination=None,
            file_dir=None,
            file_prefix=None,
            include_span_events=False,
        )
        assert config.format == expected_format

    @pytest.mark.parametrize(
        ('destination', 'expected_destination'),
        [
            ('console', _logger.LayerDestination.Console),
            ('file', _logger.LayerDestination.File),
            (None, _logger.LayerDestination.Console),
        ],
        ids=['console', 'file', 'default'],
    )
    def test_destination(
        self,
        destination: Literal['console', 'file'] | None,
        expected_destination: _logger.LayerDestination,
    ) -> None:
        """Test the destination parameter."""
        config = _functions.build_layer_config(
            name='test',
            filter_directive='trace',
            format=None,
            destination=destination,
            file_dir=None,
            file_prefix=None,
            include_span_events=False,
        )
        assert config.destination == expected_destination

    def test_max_log_files(self) -> None:
        """Test that max_log_files is passed through to the LayerConfig."""
        expected_max_log_files = 5
        config = _functions.build_layer_config(
            name='test',
            filter_directive='trace',
            destination='file',
            max_log_files=expected_max_log_files,
        )
        assert config.max_log_files == expected_max_log_files


class TestInitTracing:
    """Tests for the init_tracing function."""

    @pytest.fixture
    def mock_init_tracing(self, mocker: MockerFixture) -> MockType:
        """Mock the init_tracing function."""
        return mocker.patch('manul._manul._logger.init_tracing', autospec=True)

    def test_init_tracing(self, mock_init_tracing: MockType) -> None:
        """Test the init_tracing function."""
        mock_init_tracing.return_value = 'mock_guard'
        config = _functions.build_layer_config(
            name='test',
            filter_directive='trace',
            format=None,
            destination=None,
            file_dir=None,
            file_prefix=None,
            include_span_events=False,
        )
        guard = _functions.init_tracing([config])
        assert guard == 'mock_guard'
        mock_init_tracing.assert_called_once_with([config])


class TestSetFilter:
    """Tests for the set_filter function."""

    def test_set_filter_forwards_arguments(self, mocker: MockerFixture) -> None:
        """Test that set_filter forwards its arguments to the pyo3 wrapper unchanged."""
        mock_set_filter = mocker.patch.object(_logger, 'set_filter', autospec=True)
        _functions.set_filter('my_layer', 'debug')
        mock_set_filter.assert_called_once_with('my_layer', 'debug')


class TestLogFunctions:
    """Tests for the logging functions (info, debug, warn, error, trace, log_sink)."""

    @pytest.fixture
    def mock_log_sink(self, mocker: MockerFixture) -> MockType:
        """Mock the underlying `_logger._log_sink` pyo3 function."""
        return mocker.patch.object(_logger, '_log_sink', autospec=True)

    @pytest.mark.parametrize(
        ('level', 'levelno'),
        [('trace', 0), ('debug', 10), ('info', 20), ('warn', 30), ('error', 40)],
    )
    def test_level_function_reports_caller_as_location(
        self,
        mock_log_sink: MockType,
        level: str,
        levelno: int,
    ) -> None:
        """Test that trace/debug/info/warn/error attribute the log to their caller's frame."""
        getattr(_functions, level)(f'test {level} message', extra={'key': 'value'})
        mock_log_sink.assert_called_once_with(
            levelno=levelno,
            message=f'test {level} message',
            filename=__file__,
            func_name='test_level_function_reports_caller_as_location',
            lineno=ANY,
            module_name='test_logger',
            extra={'key': 'value'},
            spans=None,
            exception=None,
        )

    def test_log_sink(self, mock_log_sink: MockType) -> None:
        """Test the log_sink function."""
        _functions.log_sink(
            levelno=20,
            message='test sink message',
            filename='test.py',
            func_name='test_func',
            lineno=10,
            module_name='test_mod',
            extra={'key': 'value'},
        )
        mock_log_sink.assert_called_once_with(
            levelno=20,
            message='test sink message',
            filename='test.py',
            func_name='test_func',
            lineno=10,
            module_name='test_mod',
            extra={'key': 'value'},
            spans=None,
            exception=None,
        )

    def test_log_sink_forwards_exception(self, mock_log_sink: MockType) -> None:
        """Test that log_sink forwards a passed exception payload as-is."""
        exception = {'type': 'ValueError', 'message': 'oops', 'traceback': 'Traceback...'}
        _functions.log_sink(
            levelno=40,
            message='boom',
            filename='test.py',
            func_name='test_func',
            lineno=10,
            module_name='test_mod',
            exception=exception,
        )
        mock_log_sink.assert_called_once_with(
            levelno=40,
            message='boom',
            filename='test.py',
            func_name='test_func',
            lineno=10,
            module_name='test_mod',
            extra=None,
            spans=None,
            exception=exception,
        )

    def test_log_sink_attaches_current_spans(self, mock_log_sink: MockType) -> None:
        """Test that log_sink picks up the currently open span stack."""
        with _functions.span('outer', request_id=1), _functions.span('inner', step='validate'):
            _functions.info('nested message')

        # Two more calls follow: the inner and outer spans' own close-timing events.
        mock_log_sink.assert_any_call(
            levelno=20,
            message='nested message',
            filename=__file__,
            func_name='test_log_sink_attaches_current_spans',
            lineno=ANY,
            module_name='test_logger',
            extra=None,
            spans=[
                {'name': 'outer', 'fields': {'request_id': 1}},
                {'name': 'inner', 'fields': {'step': 'validate'}},
            ],
            exception=None,
        )
        expected_call_count = 3  # nested message + inner span close + outer span close
        assert mock_log_sink.call_count == expected_call_count


class TestSpan:
    """Tests for the span context manager."""

    @pytest.fixture(autouse=True)
    def mock_log_sink(self, mocker: MockerFixture) -> MockType:
        """Mock the underlying `_logger._log_sink` pyo3 function.

        Autoused: span close now emits a timing event through it, so tests that
        don't care about that event still shouldn't make real pyo3 calls.
        """
        return mocker.patch.object(_logger, '_log_sink', autospec=True)

    def test_span_close_emits_debug_timing_event(self, mock_log_sink: MockType) -> None:
        """Test that leaving a span logs a debug-level close event with a duration."""
        with _functions.span('req', request_id=42):
            pass

        mock_log_sink.assert_called_once()
        kwargs = mock_log_sink.call_args.kwargs
        assert kwargs['levelno'] == _functions._LEVELS['debug']
        assert kwargs['message'] == 'req closed'
        assert kwargs['spans'] == [{'name': 'req', 'fields': {'request_id': 42}}]
        assert isinstance(kwargs['extra']['duration_ms'], float)

    def test_async_span_close_emits_debug_timing_event(self, mock_log_sink: MockType) -> None:
        """Test that the timing close event also fires for `async with`."""

        async def run() -> None:
            async with _functions.span('job'):
                await asyncio.sleep(0)

        asyncio.run(run())

        mock_log_sink.assert_called_once()
        kwargs = mock_log_sink.call_args.kwargs
        assert kwargs['levelno'] == _functions._LEVELS['debug']
        assert kwargs['message'] == 'job closed'
        assert isinstance(kwargs['extra']['duration_ms'], float)

    def test_span_is_scoped_to_with_block(self) -> None:
        """Test that the span stack is empty before, populated during, and empty after."""
        assert _functions._current_spans_payload() is None
        with _functions.span('req', request_id=42) as ctx:
            assert isinstance(ctx, _functions.SpanContext)
            assert _functions._current_spans_payload() == [{'name': 'req', 'fields': {'request_id': 42}}]
        assert _functions._current_spans_payload() is None

    def test_span_resets_on_exception(self) -> None:
        """Test that the span is popped even if the block raises."""
        error_message = 'boom'
        with pytest.raises(ValueError, match=error_message), _functions.span('req'):
            raise ValueError(error_message)
        assert _functions._current_spans_payload() is None

    def test_nested_spans_stack_in_order(self) -> None:
        """Test that nested spans append to, rather than replace, the current stack."""
        with _functions.span('outer', a=1):
            with _functions.span('inner', b=2):
                assert _functions._current_spans_payload() == [
                    {'name': 'outer', 'fields': {'a': 1}},
                    {'name': 'inner', 'fields': {'b': 2}},
                ]
            assert _functions._current_spans_payload() == [{'name': 'outer', 'fields': {'a': 1}}]

    def test_span_works_as_async_context_manager(self) -> None:
        """Test that `async with` pushes and pops the span like the sync path."""

        async def run() -> None:
            assert _functions._current_spans_payload() is None
            async with _functions.span('req', request_id=42) as ctx:
                assert isinstance(ctx, _functions.SpanContext)
                assert _functions._current_spans_payload() == [{'name': 'req', 'fields': {'request_id': 42}}]
            assert _functions._current_spans_payload() is None

        asyncio.run(run())

    def test_span_is_isolated_per_asyncio_task(self) -> None:
        """Test that concurrent tasks don't see each other's span stacks."""
        results = {}

        async def worker(name: str, delay: float) -> None:
            async with _functions.span(name):
                await asyncio.sleep(delay)
                results[name] = _functions._current_spans_payload()

        async def run() -> None:
            await asyncio.gather(worker('first', 0.02), worker('second', 0.0))

        asyncio.run(run())

        assert results['first'] == [{'name': 'first', 'fields': {}}]
        assert results['second'] == [{'name': 'second', 'fields': {}}]


class TestTracingHandler:
    """Tests for the TracingHandler class."""

    @pytest.fixture
    def mock_record(self) -> logging.LogRecord:
        """Mock a log record."""
        record = logging.LogRecord(
            name='test_logger',
            level=logging.INFO,
            pathname='test.py',
            lineno=10,
            msg='test log message',
            args=(),
            exc_info=None,
        )
        record.module = 'test'
        record.funcName = 'test_func'
        return record

    def test_emit(self, mocker: MockerFixture, mock_record: logging.LogRecord) -> None:
        """Test the emit method of TracingHandler."""
        # Mock the rust function.
        mock_log_sink = mocker.patch('manul.logger.handler.log_sink', autospec=True)
        # Create handler.
        handler = TracingHandler()

        # Create a log record.
        mock_record.extra_key = 'extra_value'  # Add an extra field to the record

        # Act
        handler.emit(mock_record)

        # Assert
        mock_log_sink.assert_called_once_with(
            levelno=logging.INFO,
            message='test log message',
            filename='test.py',
            func_name='test_func',
            lineno=10,
            module_name='test',
            extra={'extra_key': 'extra_value'},
            exception=None,
        )

    def test_no_extra(self, mocker: MockerFixture, mock_record: logging.LogRecord) -> None:
        """Test the emit method of TracingHandler with no extra fields."""
        # Mock the rust function.
        mock_log_sink = mocker.patch('manul.logger.handler.log_sink', autospec=True)
        # Create handler.
        handler = TracingHandler()

        handler.emit(mock_record)

        mock_log_sink.assert_called_once_with(
            levelno=logging.INFO,
            message='test log message',
            filename='test.py',
            func_name='test_func',
            lineno=10,
            module_name='test',
            extra=None,
            exception=None,
        )

    def test_emit_with_exc_info_builds_exception_payload(
        self,
        mocker: MockerFixture,
        mock_record: logging.LogRecord,
    ) -> None:
        """Test that emit captures exc_info as a typed {type, message, traceback} dict."""
        mock_log_sink = mocker.patch('manul.logger.handler.log_sink', autospec=True)
        handler = TracingHandler()

        error_message = 'oops'

        def _raise() -> None:
            raise ValueError(error_message)

        try:
            _raise()
        except ValueError:
            mock_record.exc_info = sys.exc_info()

        handler.emit(mock_record)

        mock_log_sink.assert_called_once()
        exception = mock_log_sink.call_args.kwargs['exception']
        assert exception['type'] == 'ValueError'
        assert exception['message'] == error_message
        assert 'Traceback' in exception['traceback']
        assert 'ValueError: oops' in exception['traceback']

    def test_handle_error(self, mocker: MockerFixture, mock_record: logging.LogRecord) -> None:
        """Test the handleError method of TracingHandler."""
        # Mock the rust function.
        mock_log_sink = mocker.patch(
            'manul.logger.handler.log_sink',
            autospec=True,
            side_effect=Exception('test error'),
        )
        mock_handle_error = mocker.patch.object(TracingHandler, 'handleError', autospec=True)
        # Create handler.
        handler = TracingHandler()

        # Act
        handler.emit(mock_record)

        # Assert
        mock_log_sink.assert_called_once_with(
            levelno=logging.INFO,
            message='test log message',
            filename='test.py',
            func_name='test_func',
            lineno=10,
            module_name='test',
            extra=None,
            exception=None,
        )
        mock_handle_error.assert_called_once_with(handler, mock_record)
