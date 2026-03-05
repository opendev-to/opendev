"""Default conversation tool formatter used by the Textual UI."""

from __future__ import annotations

from pathlib import Path
from typing import Any, Dict, List

from swecli.ui_textual.formatters_internal.formatter_base import STATUS_ICONS
from swecli.ui_textual.utils.tool_display import get_tool_display_parts
from swecli.ui_textual.utils.text_utils import summarize_error
from swecli.ui_textual.constants import TOOL_ERROR_SENTINEL
from swecli.ui_textual.utils.interrupt_utils import (
    create_interrupt_message,
    STANDARD_INTERRUPT_MESSAGE,
)


class StyleFormatter:
    """Minimalist formatter for conversational tool output."""

    def format_tool_result(
        self, tool_name: str, tool_args: Dict[str, Any], result: Dict[str, Any]
    ) -> str:
        # Early return for interrupted results - don't format any error message
        # Check for interrupted flag in both dict and dataclass objects (e.g., HttpResult)
        interrupted = (
            result.get("interrupted") if isinstance(result, dict)
            else getattr(result, "interrupted", False)
        )
        if interrupted:
            # Return empty string - interrupt message is already shown by on_interrupt()
            return ""

        tool_display = self._format_tool_call(tool_name, tool_args)

        if tool_name == "read_file":
            result_lines = self._format_read_file_result(tool_args, result)
        elif tool_name == "write_file":
            result_lines = self._format_write_file_result(tool_args, result)
        elif tool_name == "edit_file":
            result_lines = self._format_edit_file_result(tool_args, result)
        elif tool_name == "search":
            result_lines = self._format_search_result(tool_args, result)
        elif tool_name in {"run_command", "bash_execute"}:
            result_lines = self._format_shell_result(tool_args, result)
        elif tool_name == "list_files":
            result_lines = self._format_list_files_result(tool_args, result)
        elif tool_name == "fetch_url":
            result_lines = self._format_fetch_url_result(tool_args, result)
        elif tool_name == "analyze_image":
            result_lines = self._format_analyze_image_result(tool_args, result)
        elif tool_name == "get_process_output":
            result_lines = self._format_process_output_result(tool_args, result)
        elif tool_name == "write_todos":
            result_lines = self._format_write_todos_result(tool_args, result)
        elif tool_name == "list_todos":
            result_lines = self._format_list_todos_result(tool_args, result)
        elif tool_name in {"update_todo", "complete_todo", "complete_and_activate_next"}:
            result_lines = self._format_todo_status_result(tool_args, result)
        elif tool_name == "search_tools":
            result_lines = self._format_search_tools_result(tool_args, result)
        else:
            result_lines = self._format_generic_result(tool_name, tool_args, result)

        if not result_lines:
            result_lines = ["Completed"]
        return f"⏺ {tool_display}\n" + "\n".join(f"  ⎿  {line}" for line in result_lines)

    def _format_tool_call(self, tool_name: str, tool_args: Dict[str, Any]) -> str:
        verb, label = get_tool_display_parts(tool_name)
        display_name = f"{verb}({label})" if label else verb

        if not tool_args:
            return display_name

        arg_strs = []
        for key, value in tool_args.items():
            arg_str = self._format_argument(key, value)
            if arg_str:
                arg_strs.append(f"{key}={arg_str}")

        if arg_strs:
            return f"{self._highlight_function_name(display_name)}({', '.join(arg_strs)})"
        return self._highlight_function_name(display_name)

    def _highlight_function_name(self, function_name: str) -> str:
        COLOR = "\033[96m"
        BOLD = "\033[1m"
        RESET = "\033[0m"
        return f"{COLOR}{BOLD}{function_name}{RESET}"

    @staticmethod
    def _error_line(message: str) -> str:
        return f"{TOOL_ERROR_SENTINEL} {summarize_error(message)}"

    @staticmethod
    def _interrupted_line(message: str) -> str:
        """Format an interrupted command message in red without error sentinel.

        Uses special ::interrupted:: marker instead of ::tool_error:: to avoid
        showing the ❌ icon in the conversation log.
        """
        return f"::interrupted:: {message.strip()}"

    def _format_argument(self, key: str, value: Any) -> str:
        if value is None:
            return ""
        if isinstance(value, str):
            if key in {"content", "new_string", "old_string", "text", "command"}:
                if len(value) > 80:
                    lines = value.count("\n") + 1
                    return f"<{len(value)} chars, {lines} lines>"
                first_line = value.split("\n", 1)[0]
                if len(first_line) > 50:
                    return repr(first_line[:47] + "...")
                return repr(first_line)
            if key in {"file_path", "path", "image_path"}:
                return repr(value)
            if key == "pattern":
                return repr(value)
            if key in {"old_content", "new_content"}:
                if len(value) > 50:
                    return repr(value[:47] + "...")
                return repr(value)

        value_repr = repr(value)
        if len(value_repr) > 100:
            return value_repr[:97] + "..."
        return value_repr

    def _format_read_file_result(
        self, tool_args: Dict[str, Any], result: Dict[str, Any]
    ) -> List[str]:
        if not result.get("success"):
            error_msg = result.get("error") or "Unknown error"
            if "interrupted by user" in error_msg.lower():
                return [create_interrupt_message(STANDARD_INTERRUPT_MESSAGE)]
            return [self._error_line(error_msg)]

        output = result.get("output", "")
        size_bytes = len(output)
        size_kb = size_bytes / 1024
        lines = output.count("\n") + 1 if output else 0

        size_display = f"{size_kb:.1f} KB" if size_kb >= 1 else f"{size_bytes} B"
        return [f"Read {lines} lines • {size_display}"]

    def _format_write_file_result(
        self, tool_args: Dict[str, Any], result: Dict[str, Any]
    ) -> List[str]:
        if not result.get("success"):
            error_msg = result.get("error") or "Unknown error"
            if "interrupted by user" in error_msg.lower():
                return [create_interrupt_message(STANDARD_INTERRUPT_MESSAGE)]
            return [self._error_line(error_msg)]

        file_path = tool_args.get("file_path", "unknown")
        content = tool_args.get("content", "")
        size_bytes = len(content)
        size_kb = size_bytes / 1024
        lines = content.count("\n") + 1 if content else 0
        size_display = f"{size_kb:.1f} KB" if size_kb >= 1 else f"{size_bytes} B"
        return [f"Created {Path(file_path).name} • {size_display} • {lines} lines"]

    def _format_edit_file_result(
        self, tool_args: Dict[str, Any], result: Dict[str, Any]
    ) -> List[str]:
        from swecli.ui_textual.formatters_internal.utils import DiffParser

        if not result.get("success"):
            error_msg = result.get("error") or "Unknown error"
            if "interrupted by user" in error_msg.lower():
                return [create_interrupt_message(STANDARD_INTERRUPT_MESSAGE)]
            return [self._error_line(error_msg)]

        file_path = tool_args.get("file_path", "unknown")
        lines_added = result.get("lines_added", 0) or 0
        lines_removed = result.get("lines_removed", 0) or 0
        diff_text = result.get("diff") or ""

        # ANSI color codes
        GREEN = "\033[32m"
        RED = "\033[31m"
        CYAN = "\033[36m"
        DIM = "\033[2m"
        RESET = "\033[0m"

        def _plural(count: int, singular: str) -> str:
            return f"{count} {singular}" if count == 1 else f"{count} {singular}s"

        lines = []
        lines.append(
            f"Updated {file_path} with {_plural(lines_added, 'addition')} and {_plural(lines_removed, 'removal')}"
        )

        # Parse and display diff if available
        if diff_text:
            diff_entries = DiffParser.parse_unified_diff(diff_text)
            if diff_entries:
                hunks = DiffParser.group_by_hunk(diff_entries)
                total_hunks = len(hunks)

                for hunk_idx, (start_line, hunk_entries) in enumerate(hunks):
                    # Add hunk header for multiple hunks
                    if total_hunks > 1:
                        lines.append("")  # Blank line before hunk
                        lines.append(
                            f"{CYAN}[Edit {hunk_idx + 1}/{total_hunks} at line {start_line}]{RESET}"
                        )

                    for entry_type, line_no, content in hunk_entries:
                        display_no = f"{line_no:>3}" if line_no is not None else "   "
                        sanitized = content.replace("\t", "    ")

                        if entry_type == "add":
                            lines.append(
                                f"{DIM}{display_no}{RESET} {GREEN}+{RESET} {GREEN}{sanitized}{RESET}"
                            )
                        elif entry_type == "del":
                            lines.append(
                                f"{DIM}{display_no}{RESET} {RED}-{RESET} {RED}{sanitized}{RESET}"
                            )
                        else:
                            lines.append(f"{DIM}{display_no}   {sanitized}{RESET}")

        return lines

    # Binary file extensions to skip in search results
    _BINARY_EXTENSIONS = {
        ".exe",
        ".dll",
        ".so",
        ".dylib",
        ".o",
        ".a",
        ".lib",
        ".pyc",
        ".pyo",
        ".class",
        ".jar",
        ".war",
        ".test",
        ".bin",
        ".dat",
        ".db",
        ".sqlite",
        ".sqlite3",
        ".zip",
        ".tar",
        ".gz",
        ".bz2",
        ".xz",
        ".7z",
        ".rar",
        ".png",
        ".jpg",
        ".jpeg",
        ".gif",
        ".bmp",
        ".ico",
        ".webp",
        ".pdf",
        ".doc",
        ".docx",
        ".xls",
        ".xlsx",
        ".ppt",
        ".pptx",
        ".mp3",
        ".mp4",
        ".avi",
        ".mov",
        ".mkv",
        ".wav",
        ".flac",
        ".woff",
        ".woff2",
        ".ttf",
        ".otf",
        ".eot",
    }

    def _is_binary_file(self, filepath: str) -> bool:
        """Check if a file is likely binary based on extension."""
        ext = Path(filepath).suffix.lower()
        return ext in self._BINARY_EXTENSIONS

    def _format_search_result(self, tool_args: Dict[str, Any], result: Dict[str, Any]) -> List[str]:
        if not result.get("success"):
            error_msg = result.get("error") or "Unknown error"
            if "interrupted by user" in error_msg.lower():
                return [create_interrupt_message(STANDARD_INTERRUPT_MESSAGE)]
            return [self._error_line(error_msg)]

        matches = result.get("matches", [])
        if not matches:
            return ["No matches found"]

        # Group matches by file and count
        file_counts: Dict[str, int] = {}
        binary_files: set = set()

        for match in matches:
            # Support both formats: {"file", "line", "content"} and {"location", "preview"}
            if "file" in match:
                filepath = match["file"]
            else:
                # Extract file from location like "path/file.py:123"
                location = match.get("location", "unknown")
                filepath = location.split(":")[0] if ":" in location else location

            # Skip binary files
            if self._is_binary_file(filepath):
                binary_files.add(filepath)
                continue

            file_counts[filepath] = file_counts.get(filepath, 0) + 1

        if not file_counts and not binary_files:
            return ["No matches found"]

        # Format as "file.py (N matches)" - show up to 10 files
        summary = []
        total_matches = sum(file_counts.values())

        for filepath, count in list(file_counts.items())[:10]:
            match_word = "match" if count == 1 else "matches"
            summary.append(f"{filepath} ({count} {match_word})")

        # Show truncation if more files
        remaining_files = len(file_counts) - 10
        if remaining_files > 0:
            summary.append(f"... and {remaining_files} more files")

        # Note skipped binary files
        if binary_files:
            summary.append(f"(skipped {len(binary_files)} binary file(s))")

        return summary

    def _format_shell_result(self, tool_args: Dict[str, Any], result: Dict[str, Any]) -> List[str]:
        if not result.get("success"):
            error_msg = result.get("error") or "Unknown error"
            # Special handling for interrupted commands
            if "interrupted" in error_msg.lower():
                return [create_interrupt_message(STANDARD_INTERRUPT_MESSAGE)]
            return [self._error_line(error_msg)]

        command = (tool_args.get("command") or "").strip()
        stdout = (result.get("stdout") or result.get("output") or "").strip()
        stderr = (result.get("stderr") or "").strip()
        exit_code = result.get("exit_code", 0)

        normalized_cmd = command.lower()
        normalized_stdout = stdout.lower()

        if exit_code not in (None, 0):
            if stderr:
                first_err = stderr.splitlines()[0].strip()
                return [self._error_line(first_err)]
            return [self._error_line(f"Exit code {exit_code}")]

        if normalized_cmd.startswith("git ") or " git " in normalized_cmd:
            if "push" in normalized_cmd:
                return ["Changes pushed to remote"]
            if "commit" in normalized_cmd:
                return ["Changes committed"]
            if "pull" in normalized_cmd:
                return ["Changes pulled from remote"]
            return ["Git command completed"]

        if "npm install" in normalized_cmd:
            if "added" in normalized_stdout and "package" in normalized_stdout:
                return ["Packages installed successfully"]
            return ["npm install completed"]

        # lsof - count processes (skip header row)
        if "lsof" in normalized_cmd:
            lines = stdout.splitlines()
            count = max(0, len(lines) - 1)  # Exclude header
            if count == 0:
                return ["No processes found"]
            return [f"{count} process(es) listening"]

        # ps - count processes (skip header row)
        if normalized_cmd.startswith("ps "):
            lines = stdout.splitlines()
            count = max(0, len(lines) - 1)
            return [f"{count} process(es)"]

        # netstat/ss - count connections (skip header row)
        if "netstat" in normalized_cmd or normalized_cmd.startswith("ss "):
            lines = [l for l in stdout.splitlines() if l.strip()]
            count = max(0, len(lines) - 1)
            return [f"{count} connection(s)"]

        # wc - show the actual count
        if normalized_cmd.startswith("wc "):
            # wc output is typically: "  123 filename" or just "123"
            first_line = stdout.splitlines()[0].strip() if stdout else ""
            parts = first_line.split()
            if parts:
                return [parts[0]]  # Just the count

        if stdout:
            lines = stdout.splitlines()
            first_line = lines[0].strip()
            if len(lines) == 1 and len(first_line) < 80:
                return [first_line]
            first_preview = first_line[:70] + ("..." if len(first_line) > 70 else "")
            return [f"{first_preview} ({len(lines)} lines)"]

        if stderr:
            first_err = stderr.splitlines()[0].strip()
            return [first_err]

        return ["Command completed with no output"]

    def _format_list_files_result(
        self, tool_args: Dict[str, Any], result: Dict[str, Any]
    ) -> List[str]:
        if not result.get("success"):
            error_msg = result.get("error") or "Unknown error"
            if "interrupted by user" in error_msg.lower():
                return [create_interrupt_message(STANDARD_INTERRUPT_MESSAGE)]
            return [self._error_line(error_msg)]

        entries = result.get("entries")
        if entries:
            return [f"{len(entries)} entries"]

        output = result.get("output")
        if not output:
            return ["No files found"]

        lines = [line for line in output.splitlines() if line.strip()]
        if not lines:
            return ["No files found"]

        first_line = lines[0]
        preview = first_line if len(first_line) <= 70 else first_line[:67] + "..."
        if len(lines) == 1:
            return [preview]
        return [f"{preview} ({len(lines)} lines)"]

    def _format_fetch_url_result(
        self, tool_args: Dict[str, Any], result: Dict[str, Any]
    ) -> List[str]:
        if not result.get("success"):
            error_msg = result.get("error") or "Unknown error"
            if "interrupted by user" in error_msg.lower():
                return [create_interrupt_message(STANDARD_INTERRUPT_MESSAGE)]
            return [self._error_line(error_msg)]

        elapsed = result.get("elapsed", 0.0)
        status = result.get("status_code", 200)
        return [f"HTTP {status} in {elapsed:.2f}s"]

    def _format_analyze_image_result(
        self, tool_args: Dict[str, Any], result: Dict[str, Any]
    ) -> List[str]:
        if not result.get("success"):
            error_msg = result.get("error") or "Unknown error"
            if "interrupted by user" in error_msg.lower():
                return [create_interrupt_message(STANDARD_INTERRUPT_MESSAGE)]
            return [self._error_line(error_msg)]
        return [result.get("summary", "Analysis complete")]

    def _format_process_output_result(
        self, tool_args: Dict[str, Any], result: Dict[str, Any]
    ) -> List[str]:
        if not result.get("success"):
            error_msg = result.get("error") or "Unknown error"
            if "interrupted by user" in error_msg.lower():
                return [create_interrupt_message(STANDARD_INTERRUPT_MESSAGE)]
            return [self._error_line(error_msg)]

        lines = (result.get("output") or "").splitlines()
        lines = [line.strip() for line in lines if line.strip()]
        if not lines:
            return ["Process completed with no output"]

        first_line = lines[0]
        if len(lines) == 1 and len(first_line) < 80:
            return [first_line]

        preview = first_line[:70] + ("..." if len(first_line) > 70 else "")
        return [f"{preview} ({len(lines)} lines)"]

    def _format_write_todos_result(
        self, tool_args: Dict[str, Any], result: Dict[str, Any]
    ) -> List[str]:
        """Format write_todos result as a brief 1-line summary."""
        if not result.get("success"):
            error_msg = result.get("error") or "Unknown error"
            return [self._error_line(error_msg)]

        count = result.get("created_count", 0)
        if count:
            return [f"Created {count} todo{'s' if count != 1 else ''}"]
        return ["Todos created"]

    def _format_list_todos_result(
        self, tool_args: Dict[str, Any], result: Dict[str, Any]
    ) -> List[str]:
        """Format list_todos result as a 1-line summary."""
        if not result.get("success"):
            return [self._error_line(result.get("error", "Unknown error"))]

        count = result.get("count", 0)
        todos = result.get("todos", [])
        if not count:
            return ["No todos"]

        doing = sum(1 for t in todos if t.get("status") == "doing")
        done = sum(1 for t in todos if t.get("status") == "done")
        pending = count - doing - done
        parts = []
        if doing:
            parts.append(f"{doing} active")
        if done:
            parts.append(f"{done} done")
        if pending:
            parts.append(f"{pending} pending")
        return [f"{count} todo{'s' if count != 1 else ''} ({', '.join(parts)})"]

    def _format_todo_status_result(
        self, tool_args: Dict[str, Any], result: Dict[str, Any]
    ) -> List[str]:
        """Format update_todo / complete_todo / complete_and_activate_next results."""
        if not result.get("success"):
            return [self._error_line(result.get("error", "Unknown error"))]

        output = result.get("output", "")
        if not output:
            return ["Updated"]
        return output.strip().splitlines()

    def _format_search_tools_result(
        self, tool_args: Dict[str, Any], result: Dict[str, Any]
    ) -> List[str]:
        """Format search_tools result with concise summary."""
        if not result.get("success"):
            error_msg = result.get("error") or "Unknown error"
            return [self._error_line(error_msg)]

        count = result.get("count", 0)
        detail_level = result.get("detail_level", "brief")

        if count == 0:
            query = tool_args.get("query", "")
            return [f"No tools found matching '{query}'."]

        # For full detail, schemas are loaded
        if detail_level == "full":
            tools = result.get("tools", [])
            tool_names = [t.get("name", "") for t in tools[:6]]
            names_str = ", ".join(tool_names)
            if len(tools) > 6:
                names_str += f", +{len(tools) - 6} more"
            return [f"Found {count} tool(s) (schemas now loaded):", f"     {names_str}"]

        # For brief/names, just show count
        return [f"Found {count} tool(s)."]

    def _format_generic_result(
        self, tool_name: str, tool_args: Dict[str, Any], result: Dict[str, Any]
    ) -> List[str]:
        if not result.get("success"):
            error_msg = result.get("error") or "Unknown error"
            if "interrupted by user" in error_msg.lower():
                return [create_interrupt_message(STANDARD_INTERRUPT_MESSAGE)]
            return [self._error_line(error_msg)]

        output = result.get("output")
        if isinstance(output, str):
            lines = output.strip().splitlines()
            if not lines:
                return ["Completed"]
            return lines[:3] + (["…"] if len(lines) > 3 else [])

        if isinstance(output, list):
            truncated = [str(item) for item in output[:3]]
            if len(output) > 3:
                truncated.append("…")
            return truncated

        if isinstance(output, dict):
            return [f"{key}: {value}" for key, value in list(output.items())[:3]]

        if output is None:
            status = result.get("status", "completed")
            if isinstance(status, str):
                status_display = status.replace("_", " ").capitalize()
            else:
                status_display = "Completed"
            return [status_display]

        return [str(output)]
