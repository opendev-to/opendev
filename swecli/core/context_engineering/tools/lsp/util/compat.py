"""Compatibility utilities replacing sensai and serena dependencies."""

from __future__ import annotations

import logging
import os
import pickle
from dataclasses import dataclass, field
from enum import StrEnum
from typing import Any, Self

import pathspec
from pathspec import PathSpec

log = logging.getLogger(__name__)


# ============================================================================
# sensai.util.pickle replacements
# ============================================================================


def load_pickle(path: str) -> Any | None:
    """Load a pickle file, returning None if it doesn't exist or fails."""
    try:
        with open(path, "rb") as f:
            return pickle.load(f)
    except (FileNotFoundError, pickle.UnpicklingError, EOFError, Exception):
        return None


def dump_pickle(obj: Any, path: str) -> None:
    """Dump an object to a pickle file."""
    os.makedirs(os.path.dirname(path), exist_ok=True)
    with open(path, "wb") as f:
        pickle.dump(obj, f)


def getstate(obj: Any) -> Any:
    """Get state of an object for pickling."""
    if hasattr(obj, "__getstate__"):
        return obj.__getstate__()
    return obj.__dict__.copy() if hasattr(obj, "__dict__") else None


# ============================================================================
# sensai.util.string.ToStringMixin replacement
# ============================================================================


class ToStringMixin:
    """Mixin class that provides a __str__ method based on instance attributes."""

    def __str__(self) -> str:
        attrs = ", ".join(f"{k}={v!r}" for k, v in self.__dict__.items())
        return f"{self.__class__.__name__}({attrs})"

    def __repr__(self) -> str:
        return self.__str__()


# ============================================================================
# sensai.util.logging.LogTime replacement
# ============================================================================


class LogTime:
    """Context manager for logging elapsed time."""

    def __init__(self, name: str, logger: logging.Logger | None = None) -> None:
        self.name = name
        self.logger = logger or log
        self.start_time: float = 0

    def __enter__(self) -> LogTime:
        import time

        self.start_time = time.time()
        return self

    def __exit__(self, *args: Any) -> None:
        import time

        elapsed = time.time() - self.start_time
        self.logger.debug(f"{self.name} took {elapsed:.2f}s")


# ============================================================================
# serena.text_utils.MatchedConsecutiveLines replacement
# ============================================================================


class LineType(StrEnum):
    """Enum for different types of lines in search results."""

    MATCH = "match"
    BEFORE_MATCH = "prefix"
    AFTER_MATCH = "postfix"


@dataclass(kw_only=True)
class TextLine:
    """Represents a line of text with information on how it relates to the match."""

    line_number: int
    line_content: str
    match_type: LineType

    def get_display_prefix(self) -> str:
        """Get the display prefix for this line based on the match type."""
        if self.match_type == LineType.MATCH:
            return "  >"
        return "..."

    def format_line(self, include_line_numbers: bool = True) -> str:
        """Format the line for display."""
        prefix = self.get_display_prefix()
        if include_line_numbers:
            line_num = str(self.line_number).rjust(4)
            prefix = f"{prefix}{line_num}"
        return f"{prefix}:{self.line_content}"


@dataclass(kw_only=True)
class MatchedConsecutiveLines:
    """Represents a collection of consecutive lines found through some criterion."""

    lines: list[TextLine]
    source_file_path: str | None = None

    # set in post-init
    lines_before_matched: list[TextLine] = field(default_factory=list)
    matched_lines: list[TextLine] = field(default_factory=list)
    lines_after_matched: list[TextLine] = field(default_factory=list)

    def __post_init__(self) -> None:
        for line in self.lines:
            if line.match_type == LineType.BEFORE_MATCH:
                self.lines_before_matched.append(line)
            elif line.match_type == LineType.MATCH:
                self.matched_lines.append(line)
            elif line.match_type == LineType.AFTER_MATCH:
                self.lines_after_matched.append(line)

        assert len(self.matched_lines) > 0, "At least one matched line is required"

    @property
    def start_line(self) -> int:
        return self.lines[0].line_number

    @property
    def end_line(self) -> int:
        return self.lines[-1].line_number

    @property
    def num_matched_lines(self) -> int:
        return len(self.matched_lines)

    def to_display_string(self, include_line_numbers: bool = True) -> str:
        return "\n".join([line.format_line(include_line_numbers) for line in self.lines])

    @classmethod
    def from_file_contents(
        cls,
        file_contents: str,
        line: int,
        context_lines_before: int = 0,
        context_lines_after: int = 0,
        source_file_path: str | None = None,
    ) -> Self:
        line_contents = file_contents.split("\n")
        start_lineno = max(0, line - context_lines_before)
        end_lineno = min(len(line_contents) - 1, line + context_lines_after)
        text_lines: list[TextLine] = []
        # before the line
        for lineno in range(start_lineno, line):
            text_lines.append(
                TextLine(line_number=lineno, line_content=line_contents[lineno], match_type=LineType.BEFORE_MATCH)
            )
        # the line
        text_lines.append(TextLine(line_number=line, line_content=line_contents[line], match_type=LineType.MATCH))
        # after the line
        for lineno in range(line + 1, end_lineno + 1):
            text_lines.append(
                TextLine(line_number=lineno, line_content=line_contents[lineno], match_type=LineType.AFTER_MATCH)
            )

        return cls(lines=text_lines, source_file_path=source_file_path)


# ============================================================================
# serena.util.file_system.match_path replacement
# ============================================================================


def match_path(relative_path: str, path_spec: PathSpec, root_path: str = "") -> bool:
    """
    Match a relative path against a given pathspec.

    :param relative_path: relative path to match against the pathspec
    :param path_spec: the pathspec to match against
    :param root_path: the root path from which the relative path is derived
    :return: True if the path matches
    """
    normalized_path = str(relative_path).replace(os.path.sep, "/")

    # Prefix with / to handle patterns like /src/...
    if not normalized_path.startswith("/"):
        normalized_path = "/" + normalized_path

    # pathspec can't handle directory matching without trailing slash
    abs_path = os.path.abspath(os.path.join(root_path, relative_path))
    if os.path.isdir(abs_path) and not normalized_path.endswith("/"):
        normalized_path = normalized_path + "/"

    return path_spec.match_file(normalized_path)
