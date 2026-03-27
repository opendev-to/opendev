<!--
name: 'Tool Description: list_files'
description: List files in a directory or search by glob pattern
version: 3.0.0
-->

Fast file pattern matching tool that works with any codebase size.

## Usage notes

- Supports glob patterns like "**/*.py", "src/**/*.ts", or "*.json"
- Returns matching file paths sorted by modification time
- Results are capped at 500 entries to prevent context bloat (configurable via `max_results`)
- Control directory traversal depth with `max_depth` (default 2) when listing without a glob pattern
- Use this tool when you need to find files by name or extension patterns
- Prefer list_files over run_command with ls or find
- When doing an open-ended search that may require multiple rounds of globbing and grepping, consider using a subagent instead
- You can speculatively perform multiple searches in the same response if they are potentially useful
- For searching file contents rather than file names, use the search tool instead

## Common patterns

- List all files in a subdirectory: `pattern="**/*"` with `path="src/game"`
- List files by extension: `pattern="**/*.ts"` (from working dir) or `pattern="**/*.ts"` with `path="src"`
- List top-level files only: `pattern="*"` with desired `path`

## Common mistakes

- `pattern="src/game/**"` → `**` alone matches directories, not files. Use `pattern="**/*"` with `path="src/game"`
- `pattern="flappy/**"` with `path="src/game"` → same issue. Use `pattern="**/*"` with `path="src/game/flappy"`
- Putting directory paths in `pattern` instead of `path` wastes a retry when the dir doesn't exist
