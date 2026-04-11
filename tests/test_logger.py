"""Tests for manul.logger module.

Does not test rust code directly, but tests the Python wrapper functions in manul.logger._functions.
Rust calls are mocked to verify that the correct parameters are passed from Python to Rust.
"""

import logging
from typing import Callable, Literal

import pytest
from pytest_mock import MockerFixture, MockType

from manul._manul import _logger  # ty: ignore[unresolved-import]
from manul.logger import _functions
from manul.logger.handler import TracingHandler


class TestBuildLayerConfig:
    """Tests for the build_layer_config function."""

    @pytest.mark.parametrize(
        'format, expected_format',
        [
            ('json', _logger.LogFormat.Json),
            ('compact', _logger.LogFormat.Compact),
            (None, _logger.LogFormat.Compact),
        ],
        ids=['json', 'compact', 'default'],
    )
    def test_format(
        self, format: Literal['compact', 'pretty', 'json'] | None, expected_format: _logger.LogFormat
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
        'destination, expected_destination',
        [
            ('console', _logger.LayerDestination.Console),
            ('file', _logger.LayerDestination.File),
            (None, _logger.LayerDestination.Console),
        ],
        ids=['console', 'file', 'default'],
    )
    def test_destination(
        self, destination: Literal['console', 'file'] | None, expected_destination: _logger.LayerDestination
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


class TestLogFunctions:
    """Tests for the logging functions (info, debug, warn, error, trace, log_sink)."""

    @pytest.fixture
    def mock_log_fn(self, mocker: MockerFixture) -> Callable[..., MockType]:
        """Mock the underlying log functions in the _logger module."""

        def factory(level: str) -> MockType:
            """Create a mock for the specified log level."""
            return mocker.patch(f'manul._manul._logger.{level}', autospec=True)

        return factory

    def test_info(self, mock_log_fn: Callable[..., MockType]) -> None:
        """Test the info function."""
        mock_log = mock_log_fn('info')
        _functions.info('test info message', extra={'key': 'value'})
        mock_log.assert_called_once_with(
            message='test info message',
            extra={'key': 'value'},
        )

    def test_debug(self, mock_log_fn: Callable[..., MockType]) -> None:
        """Test the debug function."""
        mock_log = mock_log_fn('debug')
        _functions.debug('test debug message', extra={'key': 'value'})
        mock_log.assert_called_once_with(
            message='test debug message',
            extra={'key': 'value'},
        )

    def test_warn(self, mock_log_fn: Callable[..., MockType]) -> None:
        """Test the warn function."""
        mock_log = mock_log_fn('warn')
        _functions.warn('test warn message', extra={'key': 'value'})
        mock_log.assert_called_once_with(
            message='test warn message',
            extra={'key': 'value'},
        )

    def test_error(self, mock_log_fn: Callable[..., MockType]) -> None:
        """Test the error function."""
        mock_log = mock_log_fn('error')
        _functions.error('test error message', extra={'key': 'value'})
        mock_log.assert_called_once_with(
            message='test error message',
            extra={'key': 'value'},
        )

    def test_trace(self, mock_log_fn: Callable[..., MockType]) -> None:
        """Test the trace function."""
        mock_log = mock_log_fn('trace')
        _functions.trace('test trace message', extra={'key': 'value'})
        mock_log.assert_called_once_with(
            message='test trace message',
            extra={'key': 'value'},
        )

    def test_log_sink(self, mock_log_fn: Callable[..., MockType]) -> None:
        """Test the log_sink function."""
        mock_log = mock_log_fn('_log_sink')
        _functions.log_sink(
            levelno=20,
            message='test sink message',
            filename='test.py',
            func_name='test_func',
            lineno=10,
            module_name='test_mod',
            extra={'key': 'value'},
        )
        mock_log.assert_called_once_with(
            levelno=20,
            message='test sink message',
            filename='test.py',
            func_name='test_func',
            lineno=10,
            module_name='test_mod',
            extra={'key': 'value'},
        )


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
        )

    def test_handle_error(self, mocker: MockerFixture, mock_record: logging.LogRecord) -> None:
        """Test the handleError method of TracingHandler."""
        # Mock the rust function.
        mock_log_sink = mocker.patch(
            'manul.logger.handler.log_sink', autospec=True, side_effect=Exception('test error')
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
        )
        mock_handle_error.assert_called_once_with(handler, mock_record)
