"""High-level symbol retrieval API.

This module provides the SymbolRetriever class which combines
LSP client functionality with symbol abstractions for easy use.
"""

from __future__ import annotations

import logging
from pathlib import Path
from typing import Any

from .wrapper import LSPServerWrapper, get_lsp_wrapper
from .symbol import Symbol, NamePathMatcher, find_symbols_by_pattern

logger = logging.getLogger(__name__)


class SymbolRetriever:
    """High-level API for symbol retrieval and manipulation.

    This class provides easy-to-use methods for:
    - Finding symbols by name pattern
    - Finding all references to a symbol
    - Getting symbols in a document
    """

    def __init__(
        self,
        workspace_root: str | Path | None = None,
    ) -> None:
        """Initialize the symbol retriever.

        Args:
            workspace_root: Workspace root directory
        """
        self._wrapper = get_lsp_wrapper(workspace_root)
        if workspace_root:
            self._wrapper.workspace_root = Path(workspace_root)

    @property
    def workspace_root(self) -> Path | None:
        """Get the workspace root."""
        return self._wrapper.workspace_root

    def get_document_symbols(self, file_path: str | Path) -> list[Symbol]:
        """Get all symbols in a document.

        Args:
            file_path: Path to the file

        Returns:
            List of Symbol objects (hierarchical)
        """
        return self._wrapper.get_document_symbols(file_path)

    def find_symbol(
        self,
        pattern: str,
        file_path: str | Path | None = None,
    ) -> list[Symbol]:
        """Find symbols matching a name pattern.

        Args:
            pattern: Name path pattern (e.g., "MyClass.method", "my_func", "My*")
            file_path: Optional file to search in. If None, searches workspace.

        Returns:
            List of matching Symbol objects
        """
        if file_path is not None:
            # Search in specific file
            symbols = self.get_document_symbols(file_path)
            return find_symbols_by_pattern(symbols, pattern)

        # Search workspace
        symbols = self._wrapper.get_workspace_symbols(pattern)

        # Apply name path matcher for more precise filtering
        matcher = NamePathMatcher(pattern)
        return [s for s in symbols if matcher.matches(s)]

    def find_symbol_at_position(
        self,
        file_path: str | Path,
        line: int,
        character: int,
    ) -> Symbol | None:
        """Find the symbol at a specific position.

        Args:
            file_path: Path to the file
            line: 0-indexed line number
            character: 0-indexed character offset

        Returns:
            Symbol at position or None
        """
        symbols = self.get_document_symbols(file_path)

        for symbol in symbols:
            result = symbol.find_child_at_position(line, character)
            if result is not None:
                return result

        return None

    def find_references(
        self,
        file_path: str | Path,
        line: int,
        character: int,
        include_declaration: bool = True,
    ) -> list[dict[str, Any]]:
        """Find all references to a symbol at a position.

        Args:
            file_path: Path to the file containing the symbol
            line: 0-indexed line number
            character: 0-indexed character offset
            include_declaration: Whether to include the declaration itself

        Returns:
            List of reference locations with file, line, character info
        """
        return self._wrapper.find_references(file_path, line, character)

    def find_references_by_name(
        self,
        symbol_name: str,
        file_path: str | Path,
        include_declaration: bool = True,
    ) -> list[dict[str, Any]]:
        """Find all references to a symbol by name.

        Args:
            symbol_name: Name path pattern for the symbol
            file_path: Path to file where symbol is defined
            include_declaration: Whether to include the declaration

        Returns:
            List of reference locations
        """
        # First find the symbol
        symbols = self.find_symbol(symbol_name, file_path)

        if not symbols:
            logger.warning(f"Symbol not found: {symbol_name}")
            return []

        # Use the first match
        symbol = symbols[0]

        # Find references using the symbol's position
        return self.find_references(
            symbol.file_path,
            symbol.start_line,
            symbol.start_character,
            include_declaration,
        )

    def get_definition(
        self,
        file_path: str | Path,
        line: int,
        character: int,
    ) -> list[dict[str, Any]]:
        """Get definition location(s) for a symbol at position.

        Args:
            file_path: Path to the file
            line: 0-indexed line number
            character: 0-indexed character offset

        Returns:
            List of definition locations
        """
        return self._wrapper.get_definition(file_path, line, character)

    def rename_symbol(
        self,
        file_path: str | Path,
        line: int,
        character: int,
        new_name: str,
    ) -> dict[str, list[dict[str, Any]]] | None:
        """Rename a symbol across the workspace.

        Args:
            file_path: Path to file containing the symbol
            line: 0-indexed line number
            character: 0-indexed character offset
            new_name: New name for the symbol

        Returns:
            WorkspaceEdit with changes per file, or None if failed
        """
        return self._wrapper.rename_symbol(file_path, line, character, new_name)

    def rename_symbol_by_name(
        self,
        symbol_name: str,
        file_path: str | Path,
        new_name: str,
    ) -> dict[str, list[dict[str, Any]]] | None:
        """Rename a symbol by name.

        Args:
            symbol_name: Name path pattern for the symbol
            file_path: Path to file where symbol is defined
            new_name: New name for the symbol

        Returns:
            WorkspaceEdit with changes per file, or None if failed
        """
        symbols = self.find_symbol(symbol_name, file_path)

        if not symbols:
            logger.warning(f"Symbol not found: {symbol_name}")
            return None

        symbol = symbols[0]

        return self.rename_symbol(
            symbol.file_path,
            symbol.start_line,
            symbol.start_character,
            new_name,
        )


# Keep backward compatibility with async API by providing sync versions
# The solidlsp library is synchronous, so we don't need async here
async def get_retriever(workspace_root: str | Path | None = None) -> SymbolRetriever:
    """Get a SymbolRetriever instance (async for backward compatibility).

    Args:
        workspace_root: Optional workspace root

    Returns:
        SymbolRetriever instance
    """
    return SymbolRetriever(workspace_root=workspace_root)
