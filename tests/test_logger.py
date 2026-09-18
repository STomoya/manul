"""Tests for manul.logger module.

Does not test rust code directly, but tests the Python wrapper functions in manul.logger._functions.
Rust calls are mocked to verify that the correct parameters are passed from Python to Rust.
"""

from __future__ import annotations

import logging
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
        )
        mock_handle_error.assert_called_once_with(handler, mock_record)
