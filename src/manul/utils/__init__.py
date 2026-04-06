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
