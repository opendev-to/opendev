<!--
name: 'Tool Description: find_referencing_symbols'
description: Find all code locations that reference a symbol
version: 2.0.0
-->

Find all code locations that reference a specific symbol. Uses LSP to semantically find every place a function, class, or variable is used throughout the codebase.

## Usage notes

- Returns all locations where the symbol is referenced (imports, calls, assignments, type annotations, etc.)
- Results include file path, line number, and the referencing code context
- Use cases:
  - **Impact analysis**: Before changing a function, find everywhere it's called
  - **Refactoring preparation**: Understand the scope of a rename or API change
  - **Dead code detection**: Check if a function or class has any references at all
- Combine with rename_symbol for safe cross-codebase refactoring: first find references to understand impact, then rename
- Unlike search (text matching), this understands semantic references — it won't match string literals or comments that happen to contain the same text
