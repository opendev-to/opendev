<!--
name: 'Tool Description: grep'
description: Search file contents using regex patterns via ripgrep
version: 1.0.0
-->

Search file contents using regex patterns (ripgrep).

- Full regex syntax (e.g., "log.*Error", "function\\s+\\w+")
- Literal braces need escaping (use `interface\\{\\}` for `interface{}` in Go)
- Filter by glob ("*.rs"), file_type ("py", "rs"), or path
- Case insensitive: set `-i=true`
- Multiline: set `multiline=true` for cross-line patterns
- Fixed string: set `fixed_string=true` for literal (non-regex) matching
- Output modes: "content" (default), "files_with_matches", "count"
- Results in files_with_matches mode sorted by modification time (newest first)

## Usage notes

- ALWAYS use grep for content searching. NEVER use run_command with grep or rg — the grep tool has been optimized for correct permissions and access
- For simple, directed searches (specific class/function name), use grep directly. For broader codebase exploration requiring multiple rounds, consider a subagent
- When to use grep vs find_symbol: use grep for text/regex matching across files; use find_symbol for structured code navigation via LSP (finds definitions, understands symbol hierarchy)
- When to use grep vs ast_grep: use grep for text/regex matching; use ast_grep when you care about code structure (matching AST patterns regardless of whitespace/formatting)
