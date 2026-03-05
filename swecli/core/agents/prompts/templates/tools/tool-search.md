<!--
name: 'Tool Description: search'
description: Search for patterns in code using text or AST mode
version: 2.0.0
-->

Search for patterns in code. Supports 'text' mode (default, regex via ripgrep) and 'ast' mode (structural matching via ast-grep).

## Text mode

- Uses ripgrep under the hood — supports full regex syntax (e.g., "log.*Error", "function\\s+\\w+")
- Pattern syntax note: literal braces need escaping (use `interface\\{\\}` to find `interface{}` in Go)
- Filter files with the glob parameter (e.g., "*.js", "**/*.tsx") or type parameter (e.g., "js", "py", "rust")
- Output modes: "content" shows matching lines with context, "files_with_matches" shows only file paths (default), "count" shows match counts
- Multiline matching: By default patterns match within single lines. For cross-line patterns like `struct \\{[\\s\\S]*?field`, use multiline=true
- Be specific with the path to avoid slow searches across the entire codebase

## AST mode

- Use $VAR wildcards for structural patterns (e.g., "$A && $A()")
- Better for matching code structures regardless of whitespace or formatting

## Usage notes

- Results are capped at 50 matches and 30,000 chars total
- ALWAYS use search for content searching. NEVER use run_command with grep or rg — the search tool has been optimized for correct permissions and access
- For simple, directed searches (specific class/function name), use search directly. For broader codebase exploration requiring multiple rounds, consider a subagent
- When to use search vs find_symbol: use search for text/regex matching across files; use find_symbol for structured code navigation via LSP (finds definitions, understands symbol hierarchy)
