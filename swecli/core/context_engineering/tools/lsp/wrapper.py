"""Wrapper adapting solidlsp to our SymbolRetriever API.

This module provides a thin adapter layer between solidlsp's SolidLanguageServer
and our existing symbol tools API.
"""

from __future__ import annotations

import logging
import os
from pathlib import Path
from typing import Any

from swecli.core.context_engineering.tools.lsp import ls_types
from swecli.core.context_engineering.tools.lsp.ls import SolidLanguageServer
from swecli.core.context_engineering.tools.lsp.ls_config import Language
from swecli.core.context_engineering.tools.lsp.settings import SolidLSPSettings

from .symbol import Symbol, SymbolKind

logger = logging.getLogger(__name__)

# Mapping from file extensions to Language enum
_EXTENSION_TO_LANGUAGE: dict[str, Language] = {
    ".py": Language.PYTHON,
    ".pyi": Language.PYTHON,
    ".ts": Language.TYPESCRIPT,
    ".tsx": Language.TYPESCRIPT,
    ".js": Language.TYPESCRIPT,  # TypeScript server handles JS too
    ".jsx": Language.TYPESCRIPT,
    ".mjs": Language.TYPESCRIPT,
    ".rs": Language.RUST,
    ".go": Language.GO,
    ".java": Language.JAVA,
    ".kt": Language.KOTLIN,
    ".kts": Language.KOTLIN,
    ".rb": Language.RUBY,
    ".dart": Language.DART,
    ".c": Language.CPP,
    ".cpp": Language.CPP,
    ".cc": Language.CPP,
    ".cxx": Language.CPP,
    ".h": Language.CPP,
    ".hpp": Language.CPP,
    ".php": Language.PHP,
    ".cs": Language.CSHARP,
    ".swift": Language.SWIFT,
    ".sh": Language.BASH,
    ".bash": Language.BASH,
    ".ex": Language.ELIXIR,
    ".exs": Language.ELIXIR,
    ".clj": Language.CLOJURE,
    ".cljc": Language.CLOJURE,
    ".cljs": Language.CLOJURE,
    ".elm": Language.ELM,
    ".tf": Language.TERRAFORM,
    ".lua": Language.LUA,
    ".zig": Language.ZIG,
    ".nix": Language.NIX,
    ".yaml": Language.YAML,
    ".yml": Language.YAML,
    ".md": Language.MARKDOWN,
    ".erl": Language.ERLANG,
    ".hrl": Language.ERLANG,
    ".r": Language.R,
    ".R": Language.R,
    ".scala": Language.SCALA,
    ".sc": Language.SCALA,
    ".jl": Language.JULIA,
    ".f90": Language.FORTRAN,
    ".f95": Language.FORTRAN,
    ".f03": Language.FORTRAN,
    ".hs": Language.HASKELL,
    ".lhs": Language.HASKELL,
}


def get_language_from_path(file_path: str | Path) -> Language | None:
    """Get Language enum from file path.

    Args:
        file_path: Path to the file

    Returns:
        Language enum or None if unsupported
    """
    ext = Path(file_path).suffix.lower()
    return _EXTENSION_TO_LANGUAGE.get(ext)


class LSPServerWrapper:
    """Wrapper adapting solidlsp to our symbol tools API.

    This class manages SolidLanguageServer instances and provides
    methods that match our existing SymbolRetriever interface.
    """

    def __init__(
        self,
        workspace_root: str | Path | None = None,
        settings: SolidLSPSettings | None = None,
    ) -> None:
        """Initialize the wrapper.

        Args:
            workspace_root: Root directory of the workspace
            settings: Optional solidlsp settings
        """
        self._workspace_root = Path(workspace_root) if workspace_root else Path.cwd()
        self._settings = settings or SolidLSPSettings()
        self._servers: dict[Language, SolidLanguageServer] = {}

    @property
    def workspace_root(self) -> Path:
        """Get the workspace root directory."""
        return self._workspace_root

    @workspace_root.setter
    def workspace_root(self, value: Path | str) -> None:
        """Set the workspace root directory."""
        self._workspace_root = Path(value)

    def get_server(self, language: Language) -> SolidLanguageServer | None:
        """Get or create a language server for the specified language.

        Args:
            language: Language enum

        Returns:
            SolidLanguageServer instance or None if creation fails
        """
        if language in self._servers:
            server = self._servers[language]
            if server.is_running():
                return server
            # Server died, remove it
            del self._servers[language]

        try:
            server = SolidLanguageServer.create(
                language=language,
                repository_root_path=str(self._workspace_root),
                settings=self._settings,
            )
            server.start()
            self._servers[language] = server
            return server
        except Exception as e:
            logger.warning(f"Failed to create {language.name} server: {e}")
            return None

    def get_server_for_file(self, file_path: str | Path) -> SolidLanguageServer | None:
        """Get a language server for the specified file.

        Args:
            file_path: Path to the file

        Returns:
            SolidLanguageServer instance or None
        """
        language = get_language_from_path(file_path)
        if language is None:
            logger.debug(f"No language server for {file_path}")
            return None
        return self.get_server(language)

    def get_document_symbols(self, file_path: str | Path) -> list[Symbol]:
        """Get all symbols in a document.

        Args:
            file_path: Path to the file

        Returns:
            List of Symbol objects
        """
        path = Path(file_path).resolve()
        server = self.get_server_for_file(path)
        if server is None:
            return []

        # Get relative path from workspace root
        try:
            relative_path = path.relative_to(self._workspace_root)
        except ValueError:
            relative_path = path

        try:
            doc_symbols = server.request_document_symbols(str(relative_path))
            return self._convert_unified_symbols(doc_symbols.root_symbols, str(path))
        except Exception as e:
            logger.warning(f"Failed to get document symbols for {path}: {e}")
            return []

    def find_references(
        self,
        file_path: str | Path,
        line: int,
        character: int,
    ) -> list[dict[str, Any]]:
        """Find all references to a symbol at position.

        Args:
            file_path: Path to the file
            line: 0-indexed line number
            character: 0-indexed character offset

        Returns:
            List of reference locations
        """
        path = Path(file_path).resolve()
        server = self.get_server_for_file(path)
        if server is None:
            return []

        try:
            relative_path = path.relative_to(self._workspace_root)
        except ValueError:
            relative_path = path

        try:
            locations = server.request_references(str(relative_path), line, character)
            return [
                {
                    "file": loc.get("absolutePath", loc.get("uri", "")),
                    "line": loc["range"]["start"]["line"] + 1,
                    "character": loc["range"]["start"]["character"],
                    "end_line": loc["range"]["end"]["line"] + 1,
                    "end_character": loc["range"]["end"]["character"],
                }
                for loc in locations
            ]
        except Exception as e:
            logger.warning(f"Failed to get references: {e}")
            return []

    def get_definition(
        self,
        file_path: str | Path,
        line: int,
        character: int,
    ) -> list[dict[str, Any]]:
        """Get definition location(s) for symbol at position.

        Args:
            file_path: Path to the file
            line: 0-indexed line number
            character: 0-indexed character offset

        Returns:
            List of definition locations
        """
        path = Path(file_path).resolve()
        server = self.get_server_for_file(path)
        if server is None:
            return []

        try:
            relative_path = path.relative_to(self._workspace_root)
        except ValueError:
            relative_path = path

        try:
            locations = server.request_definition(str(relative_path), line, character)
            return [
                {
                    "file": loc.get("absolutePath", loc.get("uri", "")),
                    "line": loc["range"]["start"]["line"] + 1,
                    "character": loc["range"]["start"]["character"],
                }
                for loc in locations
            ]
        except Exception as e:
            logger.warning(f"Failed to get definition: {e}")
            return []

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
            Dict mapping file paths to text edits, or None if failed
        """
        path = Path(file_path).resolve()
        server = self.get_server_for_file(path)
        if server is None:
            return None

        try:
            relative_path = path.relative_to(self._workspace_root)
        except ValueError:
            relative_path = path

        try:
            workspace_edit = server.request_rename_symbol_edit(
                str(relative_path), line, character, new_name
            )
            if workspace_edit is None:
                return None

            return self._parse_workspace_edit(workspace_edit)
        except Exception as e:
            logger.warning(f"Failed to rename symbol: {e}")
            return None

    def get_workspace_symbols(self, query: str) -> list[Symbol]:
        """Search for symbols in the workspace.

        Args:
            query: Search query

        Returns:
            List of matching symbols
        """
        # Try to use Python server first, then others
        for language in [Language.PYTHON, Language.TYPESCRIPT, Language.GO, Language.RUST]:
            server = self.get_server(language)
            if server is None:
                continue

            try:
                symbols = server.request_workspace_symbol(query)
                if symbols:
                    return self._convert_unified_symbols(symbols, "")
            except Exception as e:
                logger.debug(f"Workspace symbol search failed for {language}: {e}")

        return []

    def get_diagnostics(
        self,
        file_path: str | Path,
        *,
        severity_filter: int = 1,
        max_diagnostics: int = 20,
        timeout: float = 3.0,
    ) -> list[dict[str, Any]]:
        """Get LSP diagnostics for a file after a change.

        Notifies the language server of file changes and retrieves diagnostics.

        Args:
            file_path: Path to the file to check.
            severity_filter: Maximum severity to include (1=Error, 2=Warning, etc.)
            max_diagnostics: Cap on number of diagnostics returned.
            timeout: Maximum seconds to wait for diagnostics.

        Returns:
            List of diagnostic dicts with keys: severity, message, line, character.
            Empty list if no LSP server is available or no diagnostics found.
        """
        server = self.get_server_for_file(file_path)
        if not server:
            return []

        try:
            abs_path = Path(file_path).resolve()
            rel_path = abs_path.relative_to(self._workspace_root)
        except (ValueError, OSError):
            return []

        try:
            diagnostics = server.request_text_document_diagnostics(str(rel_path))
        except Exception as e:
            logger.debug("LSP diagnostics request failed for %s: %s", file_path, e)
            return []

        results = []
        for diag in diagnostics:
            sev = diag.get("severity", 4)
            if sev > severity_filter:
                continue
            range_data = diag.get("range", {})
            start = range_data.get("start", {})
            results.append({
                "severity": "Error" if sev == 1 else "Warning" if sev == 2 else "Info",
                "message": diag.get("message", ""),
                "line": start.get("line", 0) + 1,  # 1-based for display
                "character": start.get("character", 0),
            })
            if len(results) >= max_diagnostics:
                break

        return results

    def shutdown(self) -> None:
        """Shutdown all running language servers."""
        for language, server in list(self._servers.items()):
            try:
                server.stop()
            except Exception as e:
                logger.warning(f"Failed to stop {language} server: {e}")
            del self._servers[language]

    def _convert_unified_symbols(
        self,
        unified_symbols: list[ls_types.UnifiedSymbolInformation],
        file_path: str,
        parent: Symbol | None = None,
    ) -> list[Symbol]:
        """Convert solidlsp UnifiedSymbolInformation to our Symbol class.

        Args:
            unified_symbols: List of UnifiedSymbolInformation from solidlsp
            file_path: Default file path for symbols without location
            parent: Parent symbol if nested

        Returns:
            List of Symbol objects
        """
        symbols = []

        for usym in unified_symbols:
            # Extract range info
            range_data = usym.get("range", {})
            start = range_data.get("start", {"line": 0, "character": 0})
            end = range_data.get("end", {"line": 0, "character": 0})

            # Get file path from location if available
            location = usym.get("location")
            sym_file = file_path
            if location:
                abs_path = location.get("absolutePath")
                if abs_path:
                    sym_file = abs_path
                elif location.get("uri", "").startswith("file://"):
                    sym_file = location["uri"][7:]

            symbol = Symbol(
                name=usym["name"],
                kind=SymbolKind.from_value(usym.get("kind", 13)),
                file_path=sym_file,
                start_line=start.get("line", 0),
                start_character=start.get("character", 0),
                end_line=end.get("line", 0),
                end_character=end.get("character", 0),
                container_name=usym.get("containerName"),
                parent=parent,
            )

            # Convert children recursively
            children_data = usym.get("children", [])
            if children_data:
                symbol.children = self._convert_unified_symbols(
                    children_data, sym_file, parent=symbol
                )

            symbols.append(symbol)

        return symbols

    def _parse_workspace_edit(
        self,
        workspace_edit: ls_types.WorkspaceEdit,
    ) -> dict[str, list[dict[str, Any]]]:
        """Parse solidlsp WorkspaceEdit into simplified format.

        Returns:
            Dict mapping file paths to list of text edits
        """
        result: dict[str, list[dict[str, Any]]] = {}

        # Use the helper function from ls_types
        from swecli.core.context_engineering.tools.lsp.ls_types import extract_text_edits

        try:
            changes = extract_text_edits(workspace_edit)
            for uri, edits in changes.items():
                file_path = uri
                if file_path.startswith("file://"):
                    file_path = file_path[7:]

                parsed_edits = []
                for edit in edits:
                    range_data = edit.get("range", {})
                    parsed_edits.append({
                        "start_line": range_data.get("start", {}).get("line", 0),
                        "start_character": range_data.get("start", {}).get("character", 0),
                        "end_line": range_data.get("end", {}).get("line", 0),
                        "end_character": range_data.get("end", {}).get("character", 0),
                        "new_text": edit.get("newText", ""),
                    })

                result[file_path] = parsed_edits
        except Exception as e:
            logger.warning(f"Failed to parse workspace edit: {e}")

        return result


# Global wrapper instance
_wrapper: LSPServerWrapper | None = None


def get_lsp_wrapper(workspace_root: str | Path | None = None) -> LSPServerWrapper:
    """Get or create the global LSP wrapper.

    Args:
        workspace_root: Optional workspace root to set

    Returns:
        LSPServerWrapper instance
    """
    global _wrapper
    if _wrapper is None:
        _wrapper = LSPServerWrapper(workspace_root=workspace_root)
    elif workspace_root:
        _wrapper.workspace_root = Path(workspace_root)
    return _wrapper


def shutdown_lsp_wrapper() -> None:
    """Shutdown the global LSP wrapper."""
    global _wrapper
    if _wrapper is not None:
        _wrapper.shutdown()
        _wrapper = None
