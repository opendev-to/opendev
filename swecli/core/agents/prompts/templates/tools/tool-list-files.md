<!--
name: 'Tool Description: list_files'
description: List files in a directory or search by glob pattern
version: 2.0.0
-->

Fast file pattern matching tool that works with any codebase size.

## Usage notes

- Supports glob patterns like "**/*.py", "src/**/*.ts", or "*.json"
- Returns matching file paths sorted by modification time
- Results are capped at 500 entries to prevent context bloat
- Use this tool when you need to find files by name or extension patterns
- Prefer list_files over run_command with ls or find
- When doing an open-ended search that may require multiple rounds of globbing and grepping, consider using a subagent instead
- You can speculatively perform multiple searches in parallel using batch_tool if they are potentially useful
- For searching file contents rather than file names, use the search tool instead
