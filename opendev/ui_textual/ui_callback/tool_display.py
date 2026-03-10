"""Mixin for tool call/result display in TextualUICallback."""

from __future__ import annotations

import logging
from typing import Any, Dict, Optional, TYPE_CHECKING

from opendev.models.message import ToolCall

if TYPE_CHECKING:
    pass

logger = logging.getLogger(__name__)


class CallbackToolDisplayMixin:
    """Mixin handling on_tool_call, on_tool_result, nested tool display, and todo display."""

    def on_tool_call(
        self,
        tool_name: str,
        tool_args: Dict[str, Any],
        tool_call_id: Optional[str] = None,
    ) -> None:
        """Called when a tool call is about to be executed.

        Args:
            tool_name: Name of the tool being called
            tool_args: Arguments for the tool call
            tool_call_id: Unique ID for this tool call (for parallel tracking)
        """
        # For think tool: stop spinner but don't display a tool call line
        # Thinking content will be shown via on_thinking callback
        if tool_name == "think":
            # Always stop the thinking spinner so thinking content appears cleanly
            # Use _stop_local_spinner to properly stop the SpinnerController
            if self.chat_app and hasattr(self.chat_app, "_stop_local_spinner"):
                self._run_on_ui(self.chat_app._stop_local_spinner)
            self._current_thinking = False
            return

        # Skip displaying individual spawn_subagent calls when in parallel mode
        # The parallel group header handles display for these
        if tool_name == "spawn_subagent" and self._in_parallel_agent_group:
            return  # Already displayed in parallel header, skip regular display

        # For single spawn_subagent calls, use single agent display
        # The tool still needs to execute - we just want custom display
        if tool_name == "spawn_subagent" and not self._in_parallel_agent_group:
            # Normalize args first - tool_args may be a JSON string from react_executor
            normalized = self._normalize_arguments(tool_args)
            subagent_type = normalized.get("subagent_type", "general-purpose")
            description = normalized.get("description", "")

            # Set the flag to prevent nested tool calls from showing individually
            self._in_parallel_agent_group = True

            # Use tool_call_id if available, otherwise use the agent type as the key
            agent_key = tool_call_id or subagent_type
            self._current_single_agent_id = agent_key  # Store for completion

            # Stop thinking spinner if still active (shows "Plotting...", etc.)
            if self._current_thinking:
                self._run_on_ui(self.conversation.stop_spinner)
                self._current_thinking = False

            # Stop any local spinner
            if self.chat_app and hasattr(self.chat_app, "_stop_local_spinner"):
                self._run_on_ui(self.chat_app._stop_local_spinner)

            # Call on_single_agent_start for proper single agent display
            self.on_single_agent_start(subagent_type, description, agent_key)
            return  # Prevent SpinnerService from creating competing display

        # Stop thinking spinner if still active
        if self._current_thinking:
            self._run_on_ui(self.conversation.stop_spinner)
            self._current_thinking = False

        if self.chat_app and hasattr(self.chat_app, "_stop_local_spinner"):
            self._run_on_ui(self.chat_app._stop_local_spinner)

        # Skip regular display for spawn_subagent - parallel display handles it
        if tool_name != "spawn_subagent":
            normalized_args = self._normalize_arguments(tool_args)
            # Use unified service for formatting with path resolution
            display_text = self._display_service.format_tool_header(tool_name, normalized_args)

            # Use SpinnerService for unified spinner management
            if self._app is not None and hasattr(self._app, "spinner_service"):
                # Bash commands don't need placeholders - their output is rendered separately
                is_bash = tool_name in ("bash_execute", "run_command")
                spinner_id = self._app.spinner_service.start(display_text, skip_placeholder=is_bash)
                # Track spinner by tool_call_id for parallel execution
                key = tool_call_id or f"_default_{id(tool_args)}"
                self._tool_spinner_ids[key] = spinner_id
            else:
                # Fallback to direct calls if SpinnerService not available
                if hasattr(self.conversation, "add_tool_call") and self._app is not None:
                    self._app.call_from_thread(self.conversation.add_tool_call, display_text)
                if hasattr(self.conversation, "start_tool_execution") and self._app is not None:
                    self._app.call_from_thread(self.conversation.start_tool_execution)

    def on_tool_result(
        self,
        tool_name: str,
        tool_args: Dict[str, Any],
        result: Any,
        tool_call_id: Optional[str] = None,
    ) -> None:
        """Called when a tool execution completes.

        Args:
            tool_name: Name of the tool that was executed
            tool_args: Arguments that were used
            result: Result of the tool execution (can be dict or string)
            tool_call_id: Unique ID for this tool call (for parallel tracking)
        """
        # Handle string results by converting to dict format
        if isinstance(result, str):
            result = {"success": True, "output": result}

        # EARLY interrupt check - BEFORE any spinner operations
        # This prevents redundant "Interrupted by user" messages from appearing
        # when on_interrupt() has already shown the proper interrupt message
        # Check for interrupted flag in both dict and dataclass objects (e.g., HttpResult)
        interrupted = (
            result.get("interrupted")
            if isinstance(result, dict)
            else getattr(result, "interrupted", False)
        )
        if interrupted:
            # Clean up spinner if it exists (may have been removed by _cleanup_spinners)
            key = tool_call_id or f"_default_{id(tool_args)}"
            spinner_id = self._tool_spinner_ids.pop(key, None)
            if spinner_id and self._app is not None and hasattr(self._app, "spinner_service"):
                # Pass empty message to prevent any result display
                self._app.spinner_service.stop(spinner_id, False, "")
            # For dataclass results (HttpResult), clear the error field to prevent
            # accidental formatting elsewhere in the code path
            if hasattr(result, "error"):
                result.error = None
            return  # Don't show any result message - interrupt already shown by on_interrupt()

        # Special handling for think tool - display via on_thinking callback
        # Check BEFORE spinner handling since we didn't start a spinner for think
        if tool_name == "think" and isinstance(result, dict):
            thinking_content = result.get("_thinking_content", "")
            if thinking_content:
                self.on_thinking(thinking_content)

            # Restart spinner - model continues processing after think
            if self.chat_app and hasattr(self.chat_app, "_start_local_spinner"):
                self._run_on_ui(self.chat_app._start_local_spinner)
            return  # Don't show as standard tool result

        # Stop spinner animation
        # Pass success status to color the bullet (green for success, red for failure)
        success = result.get("success", True) if isinstance(result, dict) else True

        # Look up spinner_id by tool_call_id for parallel execution
        key = tool_call_id or f"_default_{id(tool_args)}"
        spinner_id = self._tool_spinner_ids.pop(key, None)

        # Special handling for ask_user tool - the result placeholder gets removed when
        # the ask_user panel is displayed (render_ask_user_prompt removes trailing blank lines).
        # So we need to add the result line directly instead of relying on spinner_service.stop()
        if tool_name == "ask_user" and isinstance(result, dict):
            # Stop spinner without result message (placeholder was removed)
            if spinner_id and self._app is not None and hasattr(self._app, "spinner_service"):
                self._app.spinner_service.stop(spinner_id, success, "")

            # Add result line directly with standard ⎿ prefix (2 spaces, matching spinner_service)
            output = result.get("output") or result.get("error") or ""
            if output and self._app is not None:
                from rich.text import Text
                from opendev.ui_textual.style_tokens import GREY

                result_line = Text("  ⎿  ", style=GREY)
                result_line.append(output, style=GREY)
                self._run_on_ui(self.conversation.write, result_line)
            return

        # Skip displaying spawn_subagent results - the command handler shows its own result
        # EXCEPT for ask-user which needs to show the answer summary
        if tool_name == "spawn_subagent":
            normalized_args = self._normalize_arguments(tool_args)
            subagent_type = normalized_args.get("subagent_type", "")

            if spinner_id and self._app is not None and hasattr(self._app, "spinner_service"):
                self._app.spinner_service.stop(spinner_id, success)

            # For single agent spawns, mark as complete
            if self._in_parallel_agent_group:
                agent_key = getattr(self, "_current_single_agent_id", None)
                if agent_key:
                    # Extract failure reason from result
                    failure_reason = ""
                    if not success and isinstance(result, dict):
                        failure_reason = result.get("error") or result.get("content") or ""
                    self.on_single_agent_complete(agent_key, success, failure_reason=failure_reason)
                    self._in_parallel_agent_group = False
                    self._current_single_agent_id = None

            # For ask-user, show the result summary with ⎿ prefix
            # This is done AFTER completion to add the result line below the header
            if subagent_type == "ask-user" and isinstance(result, dict):
                content = result.get("content", "")
                if content and self._app is not None:
                    # Add result line with ⎿ prefix
                    self._run_on_ui(
                        self.conversation.add_tool_result,
                        content,
                    )

            return

        # Bash commands: handle background vs immediate differently
        if tool_name in ("bash_execute", "run_command") and isinstance(result, dict):
            background_task_id = result.get("background_task_id")

            if background_task_id:
                # Background task - show special message (Claude Code style)
                if spinner_id and self._app is not None and hasattr(self._app, "spinner_service"):
                    self._app.spinner_service.stop(
                        spinner_id, success, f"Running in background ({background_task_id})"
                    )
                return

            # Quick command - stop spinner first, then show bash output box
            if spinner_id and self._app is not None and hasattr(self._app, "spinner_service"):
                self._app.spinner_service.stop(spinner_id, success, "")

            is_error = not result.get("success", True)

            if hasattr(self.conversation, "add_bash_output_box"):
                import os

                command = self._normalize_arguments(tool_args).get("command", "")
                working_dir = os.getcwd()
                # Use "output" key (combined stdout+stderr from process_handlers),
                # falling back to "stdout" for compatibility
                output = result.get("output") or result.get("stdout") or ""
                stderr = result.get("stderr") or ""
                # Combine stdout and stderr for display
                if stderr and stderr not in output:
                    output = (output + "\n" + stderr).strip() if output else stderr
                # Filter out placeholder messages
                if output in ("Command executed", "Command execution failed"):
                    output = ""

                # Add OK prefix for successful commands (Claude Code style)
                if not is_error:
                    # Extract command name for the OK message
                    cmd_name = command.split()[0] if command else "command"
                    ok_line = f"OK: {cmd_name} ran successfully"
                    if output:
                        output = ok_line + "\n" + output
                    else:
                        output = ok_line

                # Add fallback for failed commands with empty output
                if is_error and not output:
                    output = f"Command failed (exit code {result.get('exit_code', 1)})"

                self._run_on_ui(
                    self.conversation.add_bash_output_box,
                    output,
                    is_error,
                    command,
                    working_dir,
                    0,  # depth
                )

            return

        # Reset status bar when plan mode completes via present_plan
        if tool_name == "present_plan" and isinstance(result, dict):
            requires_modification = result.get("requires_modification", False)
            if not requires_modification:
                if self.chat_app and hasattr(self.chat_app, "status_bar"):
                    self._run_on_ui(lambda: self.chat_app.status_bar.set_mode("normal"))

        normalized_args = self._normalize_arguments(tool_args)

        # Multi-hunk edit: display each hunk as a separate Edit block
        if tool_name == "edit_file" and isinstance(result, dict) and result.get("success"):
            hunks = self.formatter.get_edit_hunks(normalized_args, result)
            if hunks:
                file_path = normalized_args.get("file_path", "unknown")
                first = hunks[0]

                # Build per-hunk summary for the first hunk
                def _plural(count: int, singular: str) -> str:
                    return f"{count} {singular}" if count == 1 else f"{count} {singular}s"

                parts = []
                if first["additions"]:
                    parts.append(_plural(first["additions"], "addition"))
                if first["removals"]:
                    parts.append(_plural(first["removals"], "removal"))
                first_summary = f"Updated {file_path} ({', '.join(parts)})"

                # Update spinner display text to include "at line N" before stopping
                if spinner_id and self._app is not None and hasattr(self._app, "spinner_service"):
                    svc = self._app.spinner_service
                    if spinner_id in svc._spinner_displays:
                        from rich.text import Text as RichText
                        from opendev.ui_textual.style_tokens import (
                            GREY as _GREY,
                            PRIMARY as _PRIMARY,
                        )

                        new_display = RichText()
                        new_display.append("Edit", style=_PRIMARY)
                        new_display.append(
                            f"({file_path}) at line {first['start_line']}", style=_GREY
                        )
                        svc._spinner_displays[spinner_id] = new_display

                    svc.stop(spinner_id, success, first_summary)
                else:
                    if hasattr(self.conversation, "stop_tool_execution"):
                        self._run_on_ui(lambda: self.conversation.stop_tool_execution(success))

                # Render first hunk's diff lines as continuation
                first_diff_lines = []
                for entry_type, line_no, content in first["entries"]:
                    display_no = f"{line_no:>4}" if line_no is not None else "    "
                    sanitized = content.replace("\t", "    ")
                    if entry_type == "add":
                        first_diff_lines.append(f"{display_no} + {sanitized}")
                    elif entry_type == "del":
                        first_diff_lines.append(f"{display_no} - {sanitized}")
                    else:
                        first_diff_lines.append(f"{display_no}   {sanitized}")
                if first_diff_lines:
                    self._run_on_ui(
                        self.conversation.add_tool_result_continuation, first_diff_lines
                    )

                # Render additional hunks as separate Edit blocks
                for hunk in hunks[1:]:
                    self._run_on_ui(
                        self.conversation.add_extra_edit_block,
                        file_path,
                        hunk["start_line"],
                        hunk["additions"],
                        hunk["removals"],
                        hunk["entries"],
                    )

                if (
                    [first_summary]
                    and self.chat_app
                    and hasattr(self.chat_app, "record_tool_summary")
                ):
                    self._run_on_ui(
                        self.chat_app.record_tool_summary,
                        tool_name,
                        normalized_args,
                        [first_summary],
                    )

                # Auto-refresh todo panel (edit_file won't trigger, but keep pattern)
                return

        # Format the result using the Claude-style formatter
        formatted = self.formatter.format_tool_result(tool_name, normalized_args, result)

        # Extract the result line(s) from the formatted output
        # First ⎿ line goes to spinner result placeholder, additional lines displayed separately
        summary_lines: list[str] = []
        collected_lines: list[str] = []
        if isinstance(formatted, str):
            from opendev.ui_textual.constants import TOOL_ERROR_SENTINEL
            from opendev.ui_textual.utils.text_utils import summarize_error

            lines = formatted.splitlines()
            first_result_line_seen = False
            for line in lines:
                stripped = line.strip()
                if stripped.startswith("⎿"):
                    result_text = stripped.lstrip("⎿").strip()
                    # Strip error sentinel and summarize if present
                    if result_text.startswith(TOOL_ERROR_SENTINEL):
                        result_text = result_text[len(TOOL_ERROR_SENTINEL) :].strip()
                        result_text = summarize_error(result_text)
                    if result_text:
                        if not first_result_line_seen:
                            # First ⎿ line goes to placeholder only
                            first_result_line_seen = True
                            summary_lines.append(result_text)
                        else:
                            # Subsequent ⎿ lines go to collected_lines (e.g., diff content)
                            # Skip @@ header lines
                            if not result_text.startswith("@@"):
                                collected_lines.append(result_text)
        else:
            self._run_on_ui(self.conversation.write, formatted)
            if hasattr(formatted, "renderable") and hasattr(formatted, "title"):
                # Panels typically summarize tool output in title/body; try to capture text
                renderable = getattr(formatted, "renderable", None)
                if isinstance(renderable, str):
                    summary_lines.append(renderable.strip())

        # Stop spinner WITH the first summary line (for parallel tool display)
        first_summary = summary_lines[0] if summary_lines else ""
        if spinner_id and self._app is not None and hasattr(self._app, "spinner_service"):
            self._app.spinner_service.stop(spinner_id, success, first_summary)
        else:
            # Fallback to direct calls if SpinnerService not available
            if hasattr(self.conversation, "stop_tool_execution"):
                self._run_on_ui(lambda: self.conversation.stop_tool_execution(success))

        # Write tool result continuation (e.g., diff lines for edit_file)
        # These follow the summary line, so no ⎿ prefix needed - just space indentation
        if collected_lines:
            self._run_on_ui(self.conversation.add_tool_result_continuation, collected_lines)

        if summary_lines and self.chat_app and hasattr(self.chat_app, "record_tool_summary"):
            self._run_on_ui(
                self.chat_app.record_tool_summary, tool_name, normalized_args, summary_lines.copy()
            )

        # Auto-refresh todo panel after todo tool execution
        if tool_name in {"write_todos", "update_todo", "complete_todo", "clear_todos"}:
            logger.debug(f"[CALLBACK] Todo tool completed: {tool_name}, refreshing panel")
            self._refresh_todo_panel()

    def on_nested_tool_call(
        self,
        tool_name: str,
        tool_args: Dict[str, Any],
        depth: int,
        parent: str,
        tool_id: str = "",
    ) -> None:
        """Called when a nested tool call (from subagent) is about to be executed.

        Args:
            tool_name: Name of the tool being called
            tool_args: Arguments for the tool call
            depth: Nesting depth level (1 = direct child of main agent)
            parent: Name/identifier of the parent subagent
            tool_id: Unique tool call ID for tracking parallel tools
        """
        normalized_args = self._normalize_arguments(tool_args)

        # Display nested tool call with indentation (BLOCKING to ensure timer starts before tool executes)
        if hasattr(self.conversation, "add_nested_tool_call") and self._app is not None:
            # Use unified service for formatting with path resolution
            display_text = self._display_service.format_tool_header(tool_name, normalized_args)
            self._app.call_from_thread(
                self.conversation.add_nested_tool_call,
                display_text,
                depth,
                parent,
                tool_id,
            )

    def on_nested_tool_result(
        self,
        tool_name: str,
        tool_args: Dict[str, Any],
        result: Any,
        depth: int,
        parent: str,
        tool_id: str = "",
    ) -> None:
        """Called when a nested tool execution (from subagent) completes.

        Args:
            tool_name: Name of the tool that was executed
            tool_args: Arguments that were used
            result: Result of the tool execution (can be dict or string)
            depth: Nesting depth level
            parent: Name/identifier of the parent subagent
            tool_id: Unique tool call ID for tracking parallel tools
        """
        # Handle string results by converting to dict format
        if isinstance(result, str):
            result = {"success": True, "output": result}

        # EARLY interrupt check - BEFORE any collection/display logic
        # This prevents redundant "Interrupted by user" messages and prevents
        # collecting interrupted tools for session storage
        # Check for interrupted flag in both dict and dataclass objects (e.g., HttpResult)
        interrupted = (
            result.get("interrupted")
            if isinstance(result, dict)
            else getattr(result, "interrupted", False)
        )
        if interrupted:
            # Still update the tool call status to show it was interrupted
            # Use BLOCKING call_from_thread to ensure display updates before next tool
            if hasattr(self.conversation, "complete_nested_tool_call") and self._app is not None:
                self._app.call_from_thread(
                    self.conversation.complete_nested_tool_call,
                    tool_name,
                    depth,
                    parent,
                    False,  # success=False for interrupted
                    tool_id,
                )
            # For dataclass results (HttpResult), clear the error field to prevent
            # accidental formatting elsewhere in the code path
            if hasattr(result, "error"):
                result.error = None
            return  # Don't collect or display - interrupt already shown by on_interrupt()

        # Collect for session storage (always, even in collapsed/suppressed mode)
        self._pending_nested_calls.append(
            ToolCall(
                id=f"nested_{len(self._pending_nested_calls)}",
                name=tool_name,
                parameters=tool_args,
                result=result,
            )
        )

        # Skip display when in actual parallel agent group mode (not single agent)
        # The header shows aggregated stats, individual tool results are hidden
        # But for single agents, we need completions to update tool line spinners → checkmarks
        if self._in_parallel_agent_group and not self._current_single_agent_id:
            return

        # Update the nested tool call status to complete (for ALL tools including bash)
        # Use BLOCKING call_from_thread to ensure each tool's completion is displayed
        # before the next tool starts (fixes "all at once" display issue)
        if hasattr(self.conversation, "complete_nested_tool_call") and self._app is not None:
            success = result.get("success", False) if isinstance(result, dict) else True
            self._app.call_from_thread(
                self.conversation.complete_nested_tool_call,
                tool_name,
                depth,
                parent,
                success,
                tool_id,
            )

        # In single-agent mode, skip sub-result display — only checkmark updates matter
        if self._current_single_agent_id:
            return

        normalized_args = self._normalize_arguments(tool_args)

        # Special handling for todo tools (custom display format with icons)
        if tool_name == "write_todos" and result.get("success"):
            todos = tool_args.get("todos", [])
            self._display_todo_sub_results(todos, depth)
        elif tool_name == "update_todo" and result.get("success"):
            todo_data = result.get("todo", {})
            self._display_todo_update_result(tool_args, todo_data, depth)
        elif tool_name == "complete_todo" and result.get("success"):
            todo_data = result.get("todo", {})
            self._display_todo_complete_result(todo_data, depth)
        elif tool_name in ("bash_execute", "run_command") and isinstance(result, dict):
            # Special handling for bash commands - render in VS Code Terminal style
            # Docker returns "output", local bash returns "stdout"/"stderr"
            stdout = result.get("stdout") or result.get("output") or ""
            # Filter out placeholder messages
            if stdout in ("Command executed", "Command execution failed"):
                stdout = ""
            stderr = result.get("stderr") or ""
            is_error = not result.get("success", True)
            exit_code = result.get("exit_code", 1 if is_error else 0)
            command = normalized_args.get("command", "")

            # Get working_dir from tool args (Docker subagents inject this with prefix)
            working_dir = normalized_args.get("working_dir", ".")

            # Combine stdout and stderr for display, avoiding duplicates
            output = stdout.strip()
            if stderr.strip():
                output = (output + "\n" + stderr.strip()) if output else stderr.strip()

            if hasattr(self.conversation, "add_nested_bash_output_box"):
                # Signature: (output, is_error, command, working_dir, depth)
                self._run_on_ui(
                    self.conversation.add_nested_bash_output_box,
                    output,
                    is_error,
                    command,
                    working_dir,
                    depth,
                )
        else:
            # ALL other tools use unified StyleFormatter (same as main agent)
            self._display_tool_sub_result(tool_name, normalized_args, result, depth)

        # Auto-refresh todo panel after nested todo tool execution
        if tool_name in {"write_todos", "update_todo", "complete_todo", "clear_todos"}:
            logger.debug(f"[CALLBACK] Nested todo tool completed: {tool_name}, refreshing panel")
            self._refresh_todo_panel()

    def _display_tool_sub_result(
        self, tool_name: str, tool_args: Dict[str, Any], result: Dict[str, Any], depth: int
    ) -> None:
        """Display tool result using StyleFormatter (same as main agent).

        This ensures subagent results look identical to main agent results.
        No code duplication - reuses the same formatting logic.

        Args:
            tool_name: Name of the tool that was executed
            tool_args: Arguments that were used
            result: Result of the tool execution
            depth: Nesting depth for indentation
        """
        # Skip displaying interrupted operations (safety net - should be caught earlier)
        # Check for interrupted flag in both dict and dataclass objects (e.g., HttpResult)
        interrupted = (
            result.get("interrupted")
            if isinstance(result, dict)
            else getattr(result, "interrupted", False)
        )
        if interrupted:
            return

        # Special handling for edit_file - use dedicated diff display with colors
        # This avoids ANSI code stripping that happens in add_nested_tool_sub_results
        if tool_name == "edit_file" and result.get("success"):
            diff_text = result.get("diff", "")
            if diff_text and hasattr(self.conversation, "add_edit_diff_result"):
                file_path = tool_args.get("file_path", "unknown")

                # Check for multi-hunk edits
                hunks = self.formatter.get_edit_hunks(tool_args, result)
                if hunks:
                    # Multi-hunk: show first hunk with standard summary + diff,
                    # additional hunks as separate blocks
                    first = hunks[0]

                    def _plural_h(count: int, singular: str) -> str:
                        return f"{count} {singular}" if count == 1 else f"{count} {singular}s"

                    parts = []
                    if first["additions"]:
                        parts.append(_plural_h(first["additions"], "addition"))
                    if first["removals"]:
                        parts.append(_plural_h(first["removals"], "removal"))
                    summary = f"Updated {file_path} ({', '.join(parts)})"
                    self._run_on_ui(self.conversation.add_nested_tool_sub_results, [summary], depth)

                    # Build partial diff for first hunk only
                    from opendev.ui_textual.formatters_internal.utils import DiffParser

                    all_entries = DiffParser.parse_unified_diff(diff_text)
                    all_hunks = DiffParser.group_by_hunk(all_entries)
                    if all_hunks:
                        # Reconstruct diff text for first hunk only
                        first_start, first_entries = all_hunks[0]
                        first_diff_lines = [f"@@ -1 +{first_start} @@"]
                        for etype, lno, content in first_entries:
                            if etype == "add":
                                first_diff_lines.append(f"+{content}")
                            elif etype == "del":
                                first_diff_lines.append(f"-{content}")
                            else:
                                first_diff_lines.append(f" {content}")
                        self._run_on_ui(
                            self.conversation.add_edit_diff_result,
                            "\n".join(first_diff_lines),
                            depth,
                        )

                    # Render additional hunks as separate blocks
                    for hunk in hunks[1:]:
                        self._run_on_ui(
                            self.conversation.add_nested_extra_edit_block,
                            file_path,
                            hunk["start_line"],
                            hunk["additions"],
                            hunk["removals"],
                            hunk["entries"],
                            depth,
                        )
                    return

                # Single hunk: show summary + full diff (existing behavior)
                lines_added = result.get("lines_added", 0) or 0
                lines_removed = result.get("lines_removed", 0) or 0

                def _plural(count: int, singular: str) -> str:
                    return f"{count} {singular}" if count == 1 else f"{count} {singular}s"

                summary = f"Updated {file_path} with {_plural(lines_added, 'addition')} and {_plural(lines_removed, 'removal')}"
                self._run_on_ui(self.conversation.add_nested_tool_sub_results, [summary], depth)
                # Then show colored diff
                self._run_on_ui(self.conversation.add_edit_diff_result, diff_text, depth)
                return
            # Fall through to generic display if no diff

        # Get result lines from StyleFormatter (same code path as main agent)
        if tool_name == "read_file":
            lines = self.formatter._format_read_file_result(tool_args, result)
        elif tool_name == "write_file":
            lines = self.formatter._format_write_file_result(tool_args, result)
        elif tool_name == "edit_file":
            lines = self.formatter._format_edit_file_result(tool_args, result)
        elif tool_name == "search":
            lines = self.formatter._format_search_result(tool_args, result)
        elif tool_name in {"run_command", "bash_execute"}:
            lines = self.formatter._format_shell_result(tool_args, result)
        elif tool_name == "list_files":
            lines = self.formatter._format_list_files_result(tool_args, result)
        elif tool_name == "fetch_url":
            lines = self.formatter._format_fetch_url_result(tool_args, result)
        elif tool_name == "analyze_image":
            lines = self.formatter._format_analyze_image_result(tool_args, result)
        elif tool_name == "get_process_output":
            lines = self.formatter._format_process_output_result(tool_args, result)
        else:
            lines = self.formatter._format_generic_result(tool_name, tool_args, result)

        # Debug logging for missing content
        if not lines:
            import logging

            logging.getLogger(__name__).debug(
                f"No display lines for nested {tool_name}: result keys={list(result.keys()) if isinstance(result, dict) else 'not dict'}"
            )

        # Display each line with proper nesting
        if lines and hasattr(self.conversation, "add_nested_tool_sub_results"):
            self._run_on_ui(self.conversation.add_nested_tool_sub_results, lines, depth)

    def _display_todo_sub_results(self, todos: list, depth: int) -> None:
        """Display nested list of created todos.

        Args:
            todos: List of todo items (dicts with content/status or strings)
            depth: Nesting depth for indentation
        """
        if not todos:
            return

        items = []
        for item in todos:
            if isinstance(item, dict):
                title = item.get("content", "")
                status = item.get("status", "pending")
            else:
                title = str(item)
                status = "pending"

            symbol = {"pending": "○", "in_progress": "▶", "completed": "✓"}.get(status, "○")
            items.append((symbol, title))

        if items and hasattr(self.conversation, "add_todo_sub_results"):
            self._run_on_ui(self.conversation.add_todo_sub_results, items, depth)

    def _display_todo_update_result(
        self, args: Dict[str, Any], todo_data: Dict[str, Any], depth: int
    ) -> None:
        """Display what was updated in the todo.

        Args:
            args: Tool arguments (contains status)
            todo_data: The todo data from result
            depth: Nesting depth for indentation
        """
        status = args.get("status", "")
        title = todo_data.get("title", "") or todo_data.get("content", "")

        if not title:
            return

        # Use icons only, no text like "doing:"
        if status in ("in_progress", "doing"):
            line = f"▶ {title}"
        elif status in ("completed", "done"):
            line = f"✓ {title}"
        else:
            line = f"○ {title}"

        if hasattr(self.conversation, "add_todo_sub_result"):
            self._run_on_ui(self.conversation.add_todo_sub_result, line, depth)

    def _display_todo_complete_result(self, todo_data: Dict[str, Any], depth: int) -> None:
        """Display completed todo.

        Args:
            todo_data: The todo data from result
            depth: Nesting depth for indentation
        """
        title = todo_data.get("title", "") or todo_data.get("content", "")

        if not title:
            return

        if hasattr(self.conversation, "add_todo_sub_result"):
            self._run_on_ui(self.conversation.add_todo_sub_result, f"✓ {title}", depth)

    def on_tool_complete(
        self,
        tool_name: str,
        success: bool,
        message: str,
        details: Optional[str] = None,
    ) -> None:
        """Called when ANY tool completes to display result.

        This is the standardized method for showing tool completion results.
        Every tool should call this to display its pass/fail status.

        Args:
            tool_name: Name of the tool that completed
            success: Whether the tool succeeded
            message: Result message to display
            details: Optional additional details (shown dimmed)
        """
        from opendev.ui_textual.formatters.result_formatter import (
            ToolResultFormatter,
            ResultType,
        )

        formatter = ToolResultFormatter()

        # Determine result type based on success
        result_type = ResultType.SUCCESS if success else ResultType.ERROR

        # Format the result using centralized formatter
        result_text = formatter.format_result(
            message,
            result_type,
            secondary=details,
        )

        # Display in conversation
        self._run_on_ui(self.conversation.write, result_text)
