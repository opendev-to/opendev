<!--
name: 'Tool Description: insert_after_symbol'
description: Insert code after a symbol
version: 2.0.0
-->

Insert code immediately after a symbol (function, class, method, etc.) using LSP-based positioning.

## Usage notes

- The content is inserted at the same indentation level as the target symbol — indentation is preserved automatically
- Useful for adding new methods after existing ones, appending helper functions, or adding related code after a class definition
- When to use vs edit_file: use insert_after_symbol for structured insertion that should appear directly after a specific code element; use edit_file for general text replacement anywhere in a file
- The symbol is located by name using LSP, so you don't need to know the exact line number
