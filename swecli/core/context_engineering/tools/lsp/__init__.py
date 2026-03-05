"""LSP (Language Server Protocol) integration for semantic code analysis.

This module provides LSP server management and symbol tools for
Python, TypeScript, Rust, Go, Java, and many other languages.
"""

# ruff: noqa: F401
from .symbol import Symbol, SymbolKind, NamePathMatcher, find_symbols_by_pattern
from .retriever import SymbolRetriever, get_retriever
from .wrapper import (
    LSPServerWrapper,
    get_lsp_wrapper,
    shutdown_lsp_wrapper,
    get_language_from_path,
)
from .ls_config import Language
from .ls import SolidLanguageServer

__all__ = [
    # Symbol
    "Symbol",
    "SymbolKind",
    "NamePathMatcher",
    "find_symbols_by_pattern",
    # Retriever
    "SymbolRetriever",
    "get_retriever",
    # Wrapper
    "LSPServerWrapper",
    "get_lsp_wrapper",
    "shutdown_lsp_wrapper",
    "get_language_from_path",
    # Language enum
    "Language",
    # SolidLanguageServer
    "SolidLanguageServer",
]
