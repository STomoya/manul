from typing import List, Optional

__version__: str
"""The version of the manul_core module."""

# region utils

class PathType:
    """An enumeration representing the type of paths to filter by. This can be 'File', 'Directory', or 'Both'."""

    File: PathType
    """Represents a file path."""

    Directory: PathType
    """Represents a directory path."""

    Both: PathType
    """Represents both file and directory paths."""

class SortStrategy:
    """An enumeration representing the strategy for sorting paths. This can be 'None', 'Standard', or 'Natural'."""

    No: SortStrategy
    """No sorting will be applied to the results."""

    Standard: SortStrategy
    """Standard lexicographical sorting will be applied to the results."""

    Natural: SortStrategy
    """Natural sorting will be applied to the results, which is more intuitive for humans."""

def find_paths(
    pattern: str,
    keyword: str | None = None,
    path_type: PathType | None = None,
    sort_strategy: SortStrategy | None = None,
    include_hidden: bool = False,
) -> List[str]:
    """Find paths matching a glob pattern with optional filtering and sorting."""

# endregion

def find_all_offsets(text: str, pattern: str) -> list[tuple[int, int]]: ...
def match_any(text: str, patterns: list[str]) -> list[int]: ...
def replace_many(text: str, replacements: dict[str, str]) -> str: ...
def sub_optimized(text: str, pattern: str, replacement: str) -> str: ...
def extract_structured(text: str, pattern: str) -> list[dict[str, str]]: ...
