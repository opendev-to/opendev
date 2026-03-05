<!--
name: 'Thinking: Available Tools'
description: Tool reference list for reasoning about possible actions
version: 1.0.0
priority: 45
-->

# Available Tools

Use this list to reason about what actions are possible. Suggest which tools to use in your reasoning.

- **File Operations**: `read_file`, `write_file`, `edit_file`
- **Search & Navigation**: `list_files`, `search` (regex/ast)
- **Symbol Operations**: `find_symbol`, `find_referencing_symbols`, `rename_symbol`
- **Command Execution**: `run_command`, `list_processes`, `kill_process`
- **Web**: `fetch_url`, `capture_web_screenshot`, `open_browser`, `analyze_image`
- **MCP**: `search_tools` (find tools by keyword then use them)
- **Task Tracking**: `write_todos`, `update_todo`, `complete_todo`
- **Subagents**: `spawn_subagent(subagent_type, task)` - Delegate complex tasks to specialized subagents (e.g., large features, deep research, multi-file refactoring). Don't use for single file edits.
