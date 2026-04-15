"""Utility functions."""

from typing import Literal

from manul._manul import _core  # ty: ignore[unresolved-import]


def find_paths(
    pattern: str,
    keyword: str | None = None,
    path_type: Literal['file', 'directory', 'both', 'f', 'b'] | None = None,
    sort_strategy: Literal['none', 'standard', 'natural'] | None = None,
    include_hidden: bool = False,
) -> list[str]:
    """Find paths matching a glob pattern with optional filtering and sorting."""
    path_type_enum = None
    sort_strategy_enum = None
    if path_type is not None:
        path_type_enum = path_type.lower()
        path_type_enum = _core.PathType(path_type_enum)
    if sort_strategy is not None:
        sort_strategy_enum = sort_strategy.lower()
        sort_strategy_enum = _core.SortStrategy(sort_strategy_enum)

    return _core.find_paths(
        pattern=pattern,
        keyword=keyword,
        path_type=path_type_enum,
        sort_strategy=sort_strategy_enum,
        include_hidden=include_hidden,
    )


def find_all_offsets(text: str, pattern: str) -> list[tuple[int, int]]:
    """Find all offsets of a pattern in a text."""
    return _core.find_all_offsets(text=text, pattern=pattern)


def match_any(text: str, patterns: list[str]) -> list[int]:
    """Match any of a list of patterns in a text."""
    return _core.match_any(text=text, patterns=patterns)


def replace_many(text: str, replacements: dict[str, str]) -> str:
    """Replace many patterns in a text with their corresponding replacements."""
    return _core.replace_many(text=text, replacements=replacements)


def sub_optimized(text: str, pattern: str, replacement: str) -> str:
    """Replace texts that match the regex pattern with the replacement."""
    return _core.sub_optimized(text=text, pattern=pattern, replacement=replacement)


def extract_structured(text: str, pattern: str) -> list[dict[str, str]]:
    """Extract named groups from a text that match the regex pattern."""
    return _core.extract_structured(text=text, pattern=pattern)
