<!--
name: 'Tool Description: ast_grep'
description: Structural code search using AST patterns via ast-grep
version: 1.0.0
-->

Search code structurally using AST patterns (ast-grep).

- Use $VAR wildcards for structural matching (e.g., "$A && $A()")
- $$$VAR matches multiple nodes (e.g., "fn $NAME() { $$$BODY }")
- Matches code structure regardless of whitespace or formatting
- Specify `lang` for ambiguous files (auto-detected from extension)
- Supported languages: rust, javascript, typescript, python, go, java, c, cpp, etc.
- Use for: finding code patterns, anti-patterns, refactoring targets
- When to use vs grep: use ast_grep when you care about code structure, use grep for text/regex
