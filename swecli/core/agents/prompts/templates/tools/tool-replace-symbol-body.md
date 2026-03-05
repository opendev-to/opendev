<!--
name: 'Tool Description: replace_symbol_body'
description: Replace the body of a symbol with new content
version: 2.0.0
-->

Replace the body of a symbol (function, method, class) with new content using LSP-based positioning.

## Usage notes

- By default preserves the function/method signature — only the body is replaced. This prevents accidentally changing the API contract
- The symbol is located by name using LSP, so you don't need to know exact line numbers
- Useful for rewriting an entire function implementation, replacing a class body, or completely overhauling a method's logic
- When to use vs edit_file:
  - Use replace_symbol_body when you want to rewrite an entire function/method body — it's safer because it preserves the signature and handles boundaries automatically
  - Use edit_file for surgical text replacements within a function body, or for edits that span multiple symbols
