"""Tests for manul utilities."""

from typing import Literal

import pytest
from pytest_mock import MockerFixture

from manul.utils import find_paths


class TestFindPaths:
    """Tests for the find_paths function."""

    @pytest.mark.parametrize(
        'path_type, expected_enum_val',
        [
            ('file', 'file'),
            ('directory', 'directory'),
            ('both', 'both'),
            ('f', 'f'),
            ('b', 'b'),
            (None, None),
        ],
    )
    def test_path_type_conversion(
        self,
        mocker: MockerFixture,
        path_type: Literal['file', 'directory', 'both', 'f', 'b'] | None,
        expected_enum_val: Literal['file', 'directory', 'both', 'f', 'b'] | None,
    ):
        """Test that path_type string is correctly converted to PathType enum."""
        mock_core = mocker.patch('manul.utils._core')

        # Mock the enum behavior
        if path_type:
            mock_core.PathType.return_value = expected_enum_val

        find_paths('*.txt', path_type=path_type)

        call_args = mock_core.find_paths.call_args[1]
        assert call_args['path_type'] == expected_enum_val

    def test_unknown_path_type(self):
        """Test that an unknown path_type raises a ValueError."""
        with pytest.raises(ValueError) as exc_info:
            find_paths('*.txt', path_type='unknown')  # ty: ignore[invalid-argument-type]

        assert str(exc_info.value) == 'Invalid PathType: unknown'

    @pytest.mark.parametrize(
        'sort_strategy, expected_enum_val',
        [
            ('none', 'none'),
            ('standard', 'standard'),
            ('natural', 'natural'),
            (None, None),
        ],
    )
    def test_sort_strategy_conversion(
        self,
        mocker: MockerFixture,
        sort_strategy: Literal['none', 'standard', 'natural'] | None,
        expected_enum_val: Literal['none', 'standard', 'natural'] | None,
    ):
        """Test that sort_strategy string is correctly converted to SortStrategy enum."""
        mock_core = mocker.patch('manul.utils._core')

        if sort_strategy:
            mock_core.SortStrategy.return_value = expected_enum_val

        find_paths('*.txt', sort_strategy=sort_strategy)

        call_args = mock_core.find_paths.call_args[1]
        assert call_args['sort_strategy'] == expected_enum_val

    def test_unknown_sort_strategy(self):
        """Test that an unknown sort_strategy raises a ValueError."""
        with pytest.raises(ValueError) as exc_info:
            find_paths('*.txt', sort_strategy='unknown')  # ty: ignore[invalid-argument-type]

        assert str(exc_info.value) == 'Invalid SortStrategy: unknown'

    def test_find_paths_parameters(self, mocker: MockerFixture):
        """Test that all parameters are passed correctly to the core function."""
        mock_core = mocker.patch('manul.utils._core')

        find_paths(pattern='**/*.py', keyword='test', include_hidden=True)

        mock_core.find_paths.assert_called_once_with(
            pattern='**/*.py',
            keyword='test',
            include_hidden=True,
            path_type=None,
            sort_strategy=None,
        )
