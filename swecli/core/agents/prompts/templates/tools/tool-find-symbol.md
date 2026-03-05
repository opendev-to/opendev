<!--
name: 'Tool Description: find_symbol'
description: Find symbols by name using LSP
version: 2.0.0
-->

Find symbols (functions, classes, variables, methods, etc.) by name using LSP. This provides structured code navigation that understands the language's syntax, unlike text-based search.

## Name path patterns

- Qualified name: "MyClass.method" finds a method within a specific class
- Partial match: "method" finds all symbols named "method" across the codebase
- Wildcards: "My*" matches all symbols starting with "My"
- Module path: "module.Class" finds a class within a specific module

## Return format

Each result includes: file path, line number, column, symbol kind (function, class, variable, etc.), and the symbol's container (if any).

## When to use

- Use find_symbol for structured code navigation — finding definitions, understanding class hierarchies, locating specific functions
- Use search for text/regex matching across files — finding string literals, comments, configuration values, or patterns that aren't code symbols
- find_symbol understands code structure; search does text matching. Prefer find_symbol when looking for code definitions
