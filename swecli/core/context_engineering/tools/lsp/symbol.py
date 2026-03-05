"""Symbol representation and name path matching.

This module provides:
- Symbol: A wrapper around LSP symbol data with name paths and hierarchy
- SymbolKind: Enum for symbol types
- NamePathMatcher: Flexible pattern matching for symbol names
"""

from __future__ import annotations

import fnmatch
import re
from dataclasses import dataclass, field
from enum import IntEnum
from pathlib import Path
from typing import Any


class SymbolKind(IntEnum):
    """LSP symbol kinds.

    These values match the LSP specification.
    """

    FILE = 1
    MODULE = 2
    NAMESPACE = 3
    PACKAGE = 4
    CLASS = 5
    METHOD = 6
    PROPERTY = 7
    FIELD = 8
    CONSTRUCTOR = 9
    ENUM = 10
    INTERFACE = 11
    FUNCTION = 12
    VARIABLE = 13
    CONSTANT = 14
    STRING = 15
    NUMBER = 16
    BOOLEAN = 17
    ARRAY = 18
    OBJECT = 19
    KEY = 20
    NULL = 21
    ENUM_MEMBER = 22
    STRUCT = 23
    EVENT = 24
    OPERATOR = 25
    TYPE_PARAMETER = 26

    @classmethod
    def from_value(cls, value: int) -> "SymbolKind":
        """Get SymbolKind from integer value."""
        try:
            return cls(value)
        except ValueError:
            return cls.VARIABLE  # Default fallback

    def is_container(self) -> bool:
        """Check if this symbol kind can contain other symbols."""
        return self in {
            SymbolKind.FILE,
            SymbolKind.MODULE,
            SymbolKind.NAMESPACE,
            SymbolKind.PACKAGE,
            SymbolKind.CLASS,
            SymbolKind.INTERFACE,
            SymbolKind.ENUM,
            SymbolKind.STRUCT,
        }


@dataclass
class Symbol:
    """Represents a code symbol from LSP.

    A symbol wraps LSP SymbolInformation or DocumentSymbol data
    with additional utilities like name paths and body extraction.
    """

    name: str
    kind: SymbolKind
    file_path: str
    start_line: int  # 0-indexed
    start_character: int
    end_line: int
    end_character: int
    container_name: str | None = None
    children: list["Symbol"] = field(default_factory=list)
    parent: "Symbol | None" = field(default=None, repr=False)

    # Cached name path
    _name_path: str | None = field(default=None, repr=False)

    @classmethod
    def from_document_symbol(
        cls,
        data: dict[str, Any],
        file_path: str,
        parent: "Symbol | None" = None,
    ) -> "Symbol":
        """Create Symbol from LSP DocumentSymbol.

        Args:
            data: DocumentSymbol data from LSP
            file_path: Path to the file containing the symbol
            parent: Parent symbol if nested

        Returns:
            Symbol instance
        """
        range_data = data.get("range", data.get("location", {}).get("range", {}))
        start = range_data.get("start", {})
        end = range_data.get("end", {})

        symbol = cls(
            name=data["name"],
            kind=SymbolKind.from_value(data.get("kind", 13)),
            file_path=file_path,
            start_line=start.get("line", 0),
            start_character=start.get("character", 0),
            end_line=end.get("line", 0),
            end_character=end.get("character", 0),
            container_name=data.get("containerName"),
            parent=parent,
        )

        # Process children
        for child_data in data.get("children", []):
            child = cls.from_document_symbol(child_data, file_path, parent=symbol)
            symbol.children.append(child)

        return symbol

    @classmethod
    def from_symbol_information(
        cls,
        data: dict[str, Any],
    ) -> "Symbol":
        """Create Symbol from LSP SymbolInformation.

        Args:
            data: SymbolInformation data from LSP

        Returns:
            Symbol instance
        """
        location = data.get("location", {})
        uri = location.get("uri", "")
        range_data = location.get("range", {})
        start = range_data.get("start", {})
        end = range_data.get("end", {})

        # Convert URI to file path
        file_path = uri
        if file_path.startswith("file://"):
            file_path = file_path[7:]

        return cls(
            name=data["name"],
            kind=SymbolKind.from_value(data.get("kind", 13)),
            file_path=file_path,
            start_line=start.get("line", 0),
            start_character=start.get("character", 0),
            end_line=end.get("line", 0),
            end_character=end.get("character", 0),
            container_name=data.get("containerName"),
        )

    @property
    def name_path(self) -> str:
        """Get the full name path (e.g., 'module.Class.method').

        Returns:
            Dot-separated path from root to this symbol
        """
        if self._name_path is not None:
            return self._name_path

        parts = []
        current: Symbol | None = self
        while current is not None:
            parts.append(current.name)
            current = current.parent

        self._name_path = ".".join(reversed(parts))
        return self._name_path

    @property
    def short_name(self) -> str:
        """Get the short name (last component of name path)."""
        return self.name

    @property
    def line_number(self) -> int:
        """Get 1-indexed line number for display."""
        return self.start_line + 1

    @property
    def kind_name(self) -> str:
        """Get human-readable kind name."""
        return self.kind.name.lower().replace("_", " ")

    def get_body(self, file_content: str | None = None) -> str:
        """Extract the symbol's body from file content.

        Args:
            file_content: Content of the file. If None, reads from file_path.

        Returns:
            The symbol's source code body
        """
        if file_content is None:
            try:
                file_content = Path(self.file_path).read_text()
            except Exception:
                return ""

        lines = file_content.splitlines(keepends=True)

        # Handle single-line symbols
        if self.start_line == self.end_line:
            if self.start_line < len(lines):
                line = lines[self.start_line]
                return line[self.start_character : self.end_character]
            return ""

        # Multi-line symbol
        body_lines = []

        for i in range(self.start_line, min(self.end_line + 1, len(lines))):
            line = lines[i]

            if i == self.start_line:
                # First line - start from character offset
                body_lines.append(line[self.start_character :])
            elif i == self.end_line:
                # Last line - end at character offset
                body_lines.append(line[: self.end_character])
            else:
                # Middle lines - include whole line
                body_lines.append(line)

        return "".join(body_lines)

    def contains_position(self, line: int, character: int) -> bool:
        """Check if a position is within this symbol's range.

        Args:
            line: 0-indexed line number
            character: 0-indexed character offset

        Returns:
            True if position is within symbol
        """
        if line < self.start_line or line > self.end_line:
            return False

        if line == self.start_line and character < self.start_character:
            return False

        if line == self.end_line and character > self.end_character:
            return False

        return True

    def find_child_at_position(self, line: int, character: int) -> "Symbol | None":
        """Find the most specific child containing a position.

        Args:
            line: 0-indexed line number
            character: 0-indexed character offset

        Returns:
            Most specific symbol containing position, or None
        """
        if not self.contains_position(line, character):
            return None

        # Check children for more specific match
        for child in self.children:
            result = child.find_child_at_position(line, character)
            if result is not None:
                return result

        return self

    def iter_descendants(self) -> "list[Symbol]":
        """Iterate over all descendants (depth-first).

        Yields:
            All descendant symbols
        """
        result = []
        for child in self.children:
            result.append(child)
            result.extend(child.iter_descendants())
        return result

    def to_dict(self) -> dict[str, Any]:
        """Convert to dictionary representation.

        Returns:
            Dictionary with symbol information
        """
        return {
            "name": self.name,
            "name_path": self.name_path,
            "kind": self.kind_name,
            "file": self.file_path,
            "line": self.line_number,
            "start_character": self.start_character,
            "end_line": self.end_line + 1,
            "end_character": self.end_character,
            "container": self.container_name,
            "children_count": len(self.children),
        }


class NamePathMatcher:
    """Flexible pattern matching for symbol name paths.

    Supports:
    - Exact match: "MyClass.my_method"
    - Partial path: "MyClass.my_method" matches "module.MyClass.my_method"
    - Name only: "my_method" matches any symbol with that name
    - Wildcards: "MyClass.*" matches all members of MyClass
    - Glob patterns: "My*" matches "MyClass", "MyFunction", etc.
    """

    def __init__(self, pattern: str) -> None:
        """Initialize the matcher.

        Args:
            pattern: Name path pattern to match
        """
        self.pattern = pattern
        self._parts = pattern.split(".")
        self._has_wildcard = "*" in pattern or "?" in pattern

    def matches(self, symbol: Symbol) -> bool:
        """Check if a symbol matches this pattern.

        Args:
            symbol: Symbol to check

        Returns:
            True if symbol matches pattern
        """
        return self.matches_name_path(symbol.name_path)

    def matches_name_path(self, name_path: str) -> bool:
        """Check if a name path matches this pattern.

        Args:
            name_path: Dot-separated name path

        Returns:
            True if name path matches pattern
        """
        # Handle wildcards with fnmatch
        if self._has_wildcard:
            # Try full path match
            if fnmatch.fnmatch(name_path, self.pattern):
                return True

            # Try matching just the name (last component)
            name = name_path.rsplit(".", 1)[-1]
            if fnmatch.fnmatch(name, self.pattern):
                return True

            # Try suffix match for partial paths
            path_parts = name_path.split(".")
            for i in range(len(path_parts)):
                suffix = ".".join(path_parts[i:])
                if fnmatch.fnmatch(suffix, self.pattern):
                    return True

            return False

        # Exact match
        if name_path == self.pattern:
            return True

        # Name-only match (single component pattern)
        if len(self._parts) == 1:
            name = name_path.rsplit(".", 1)[-1]
            return name == self.pattern

        # Partial path match (pattern is suffix of name_path)
        path_parts = name_path.split(".")
        pattern_len = len(self._parts)

        # Check if pattern matches end of path
        if len(path_parts) >= pattern_len:
            suffix_parts = path_parts[-pattern_len:]
            return suffix_parts == self._parts

        return False

    def __repr__(self) -> str:
        return f"NamePathMatcher({self.pattern!r})"


def find_symbols_by_pattern(
    symbols: list[Symbol],
    pattern: str,
    include_descendants: bool = True,
) -> list[Symbol]:
    """Find all symbols matching a name path pattern.

    Args:
        symbols: List of symbols to search
        pattern: Name path pattern
        include_descendants: Whether to search child symbols

    Returns:
        List of matching symbols
    """
    matcher = NamePathMatcher(pattern)
    results = []

    for symbol in symbols:
        if matcher.matches(symbol):
            results.append(symbol)

        if include_descendants:
            for descendant in symbol.iter_descendants():
                if matcher.matches(descendant):
                    results.append(descendant)

    return results
