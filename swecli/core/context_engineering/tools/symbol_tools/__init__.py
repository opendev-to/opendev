"""Symbol manipulation tools using LSP.

This module provides tools for semantic code operations:
- find_symbol: Find symbols by name pattern
- find_referencing_symbols: Find code referencing a symbol
- insert_before_symbol / insert_after_symbol: Insert code relative to symbols
- replace_symbol_body: Replace a symbol's definition
- rename_symbol: Refactor-safe rename across codebase
"""

from .find_symbol import handle_find_symbol
from .find_referencing_symbols import handle_find_referencing_symbols
from .insert_symbol import handle_insert_before_symbol, handle_insert_after_symbol
from .replace_symbol_body import handle_replace_symbol_body
from .rename_symbol import handle_rename_symbol

__all__ = [
    "handle_find_symbol",
    "handle_find_referencing_symbols",
    "handle_insert_before_symbol",
    "handle_insert_after_symbol",
    "handle_replace_symbol_body",
    "handle_rename_symbol",
]
