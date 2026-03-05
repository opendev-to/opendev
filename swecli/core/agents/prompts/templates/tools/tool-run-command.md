<!--
name: 'Tool Description: run_command'
description: Execute a bash/shell command
version: 2.0.0
-->

Execute a bash/shell command with optional timeout.

## Before Executing

1. **Directory verification**: If the command creates new directories or files, first verify the parent directory exists using list_files.
2. **Path quoting**: Always quote file paths that contain spaces with double quotes.
   - `python "/path/with spaces/script.py"` (correct)
   - `python /path/with spaces/script.py` (incorrect — will fail)

## Usage notes

- Commands are subject to safety checks and may require user approval
- Output is capped at 30,000 characters. If the output exceeds this limit, content is middle-truncated before being returned
- Working directory persists between commands. Prefer absolute paths over cd
- You can specify an optional timeout in seconds (default 120s, max 600s). Long commands may need a longer timeout
- Use background=true for long-running servers (Flask, Django, npm start, dev servers). You will be notified when background commands complete. You do not need to add '&' when using background mode
- IMPORTANT: Do NOT use run_command for file operations when a dedicated tool exists. Specifically:
  - Use read_file instead of cat, head, tail
  - Use edit_file instead of sed or awk
  - Use write_file instead of echo/cat with redirection
  - Use search instead of grep or rg
  - Use list_files instead of find or ls
- If a command fails, analyze the error before retrying. Do not blindly retry the same command
- When running multiple independent commands, use batch_tool with parallel mode for better performance
- When chaining dependent commands, use '&&' to ensure each succeeds before the next runs. Do NOT use newlines to separate commands

## Command Safety

Before executing commands, check for injection risks:
- Never interpolate untrusted user input directly into shell commands
- Use parameterized approaches or proper escaping when constructing commands with variable data
- Be cautious with pipe chains that process user-provided data
- Avoid `eval`, backtick substitution, or `$()` with untrusted input
- When in doubt about a command's safety, ask the user before executing
