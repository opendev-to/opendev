"""UI callback for real-time tool call display in Textual UI."""

from __future__ import annotations

import logging
import sys
from pathlib import Path
from typing import Any, Dict, Optional

logger = logging.getLogger(__name__)

from swecli.ui_textual.formatters.style_formatter import StyleFormatter
from swecli.ui_textual.style_tokens import GREY, PRIMARY
from swecli.ui_textual.services import ToolDisplayService
from swecli.ui_textual.constants import TOOL_ERROR_SENTINEL
from swecli.ui_textual.utils.text_utils import summarize_error
from swecli.models.message import ToolCall


class TextualUICallback:
    """Callback for real-time display of agent actions in Textual UI."""

    def __init__(self, conversation_log, chat_app=None, working_dir: Optional[Path] = None):
        """Initialize the UI callback.

        Args:
            conversation_log: The ConversationLog widget to display messages
            chat_app: The main chat app (SWECLIChatApp instance) for controlling processing state
            working_dir: Working directory for resolving relative paths in tool displays
        """
        self.conversation = conversation_log
        self.chat_app = chat_app
        # chat_app IS the Textual App instance itself, not a wrapper
        self._app = chat_app
        self.formatter = StyleFormatter()
        self._current_thinking = False
        # Spinner IDs for tracking active spinners via SpinnerService
        self._progress_spinner_id: str = ""
        # Dict to track multiple tool spinners for parallel execution
        # Maps tool_call_id -> spinner_id
        self._tool_spinner_ids: Dict[str, str] = {}
        # Working directory for resolving relative paths
        self._working_dir = working_dir
        # Unified display service for formatting (single source of truth)
        self._display_service = ToolDisplayService(working_dir)
        # Collector for nested tool calls (for session storage)
        self._pending_nested_calls: list[ToolCall] = []
        # Thinking mode visibility toggle (default OFF)
        self._thinking_visible = False
        # Track parallel agent group state SYNCHRONOUSLY to avoid race conditions
        # This is set immediately when parallel agents start, before async UI update
        self._in_parallel_agent_group: bool = False
        # Track current single agent ID for completion callback
        self._current_single_agent_id: str | None = None
        # Guard against duplicate interrupt messages (Fix 3)
        self._interrupt_shown: bool = False

    def mark_interrupt_shown(self) -> None:
        """Mark that interrupt feedback has been shown (called from phase 1)."""
        self._interrupt_shown = True

    def on_thinking_start(self) -> None:
        """Called when the agent starts thinking."""
        self._current_thinking = True
        self._interrupt_shown = False  # Reset guard for new run (Fix 3)
        # Reset tool_renderer interrupt flag for new run
        if hasattr(self.conversation, "reset_interrupt"):
            self._run_on_ui(self.conversation.reset_interrupt)

        # The app's built-in spinner should already be running with our custom message
        # We don't need to start another spinner, just note that thinking has started

    def on_thinking_complete(self) -> None:
        """Called when the agent completes thinking."""
        if self._current_thinking:
            # Don't stop the spinner here - let it continue during tool execution
            # The app will stop it when the entire process is complete
            self._current_thinking = False

    def on_thinking(self, content: str) -> None:
        """Called when the model produces thinking content via the think tool.

        Displays thinking content in the conversation log with dark gray styling.
        Can be toggled on/off with Ctrl+Shift+T hotkey.

        Args:
            content: The reasoning/thinking text from the model
        """
        # Check visibility from chat_app (single source of truth) or fallback to local state
        if self.chat_app and hasattr(self.chat_app, "_thinking_visible"):
            if not self.chat_app._thinking_visible:
                return  # Skip display if thinking is hidden
        elif not self._thinking_visible:
            return  # Fallback to local state

        if not content or not content.strip():
            return

        # Stop spinner BEFORE displaying thinking trace (so it appears above, not below)
        if self.chat_app and hasattr(self.chat_app, "_stop_local_spinner"):
            self._run_on_ui(self.chat_app._stop_local_spinner)

        # Display thinking block with special styling
        if hasattr(self.conversation, "add_thinking_block"):
            self._run_on_ui(self.conversation.add_thinking_block, content)

        # Restart spinner for the action phase — but NOT if interrupted
        should_restart = True
        if self.chat_app and hasattr(self.chat_app, "_interrupt_manager"):
            token = self.chat_app._interrupt_manager._active_interrupt_token
            if token and token.is_requested():
                should_restart = False
        if should_restart and self.chat_app and hasattr(self.chat_app, "_start_local_spinner"):
            self._run_on_ui(self.chat_app._start_local_spinner)

    def toggle_thinking_visibility(self) -> bool:
        """Toggle thinking content visibility.

        Syncs with chat_app state if available.

        Returns:
            New visibility state (True = visible)
        """
        # Toggle app state (single source of truth) if available
        if self.chat_app and hasattr(self.chat_app, "_thinking_visible"):
            self.chat_app._thinking_visible = not self.chat_app._thinking_visible
            self._thinking_visible = self.chat_app._thinking_visible
            return self.chat_app._thinking_visible
        else:
            # Fallback to local state
            self._thinking_visible = not self._thinking_visible
            return self._thinking_visible

    def on_critique(self, content: str) -> None:
        """Called when the model produces critique content for a thinking trace.

        Displays critique content in the conversation log with special styling.
        Only shown when thinking level is High.

        Args:
            content: The critique/feedback text from the critique phase
        """
        # Check if thinking is visible (critique only shows when thinking is visible)
        if self.chat_app and hasattr(self.chat_app, "_thinking_visible"):
            if not self.chat_app._thinking_visible:
                return  # Skip display if thinking is hidden
        elif not self._thinking_visible:
            return  # Fallback to local state

        if not content or not content.strip():
            return

        # Display critique block with special styling (reuse thinking block with prefix)
        if hasattr(self.conversation, "add_thinking_block"):
            self._run_on_ui(self.conversation.add_thinking_block, f"[Critique]\n{content}")

    def get_and_clear_nested_calls(self) -> list[ToolCall]:
        """Return collected nested calls and clear the buffer.

        Called after spawn_subagent completes to attach nested calls to the ToolCall.
        """
        calls = self._pending_nested_calls
        self._pending_nested_calls = []
        return calls

    def on_assistant_message(self, content: str) -> None:
        """Called when assistant provides a message before tool execution.

        Args:
            content: The assistant's message/thinking
        """
        if content and content.strip():
            # Stop spinner before showing assistant message
            # Note: Only call _stop_local_spinner which goes through SpinnerController
            # with grace period. Don't call conversation.stop_spinner directly as it
            # bypasses the grace period and removes the spinner immediately.
            if self.chat_app and hasattr(self.chat_app, "_stop_local_spinner"):
                self._run_on_ui(self.chat_app._stop_local_spinner)

            # Display the assistant's thinking/message (via ledger if available)
            ledger = getattr(self._app, "_display_ledger", None) if self._app else None
            if ledger:
                ledger.display_assistant_message(
                    content, "ui_callback", call_on_ui=self._run_on_ui
                )
            elif hasattr(self.conversation, "add_assistant_message"):
                logger.warning(
                    "DisplayLedger not available, falling back to direct display "
                    "(source=%s)", "ui_callback"
                )
                self._run_on_ui(self.conversation.add_assistant_message, content)
            # Force refresh to ensure immediate visual update
            if hasattr(self.conversation, "refresh"):
                self._run_on_ui(self.conversation.refresh)

    def on_message(self, message: str) -> None:
        """Called to display a simple progress message (no spinner).

        Args:
            message: The message to display
        """
        if hasattr(self.conversation, "add_system_message"):
            self._run_on_ui(self.conversation.add_system_message, message)

    def on_progress_start(self, message: str) -> None:
        """Called when a progress operation starts (shows spinner).

        Args:
            message: The progress message to display with spinner
        """
        # Use SpinnerService for unified spinner management
        if self._app is not None and hasattr(self._app, "spinner_service"):
            self._progress_spinner_id = self._app.spinner_service.start(message)
        else:
            # Fallback to direct calls if SpinnerService not available
            from rich.text import Text

            display_text = Text(message, style=PRIMARY)
            if hasattr(self.conversation, "add_tool_call") and self._app is not None:
                self._app.call_from_thread(self.conversation.add_tool_call, display_text)
            if hasattr(self.conversation, "start_tool_execution") and self._app is not None:
                self._app.call_from_thread(self.conversation.start_tool_execution)

    def on_progress_update(self, message: str) -> None:
        """Update progress text in-place (same line, keeps spinner running).

        Use this for multi-step progress where you want to update the text
        without creating a new line. The spinner and timer continue running.

        Args:
            message: New progress message to display
        """
        # Use SpinnerService for unified spinner management
        if (
            self._progress_spinner_id
            and self._app is not None
            and hasattr(self._app, "spinner_service")
        ):
            self._app.spinner_service.update(self._progress_spinner_id, message)
        else:
            # Fallback to direct calls if SpinnerService not available
            from rich.text import Text

            display_text = Text(message, style=PRIMARY)
            if hasattr(self.conversation, "update_progress_text"):
                self._run_on_ui(self.conversation.update_progress_text, display_text)

    def on_progress_complete(self, message: str = "", success: bool = True) -> None:
        """Called when a progress operation completes.

        Args:
            message: Optional result message to display
            success: Whether the operation succeeded (affects bullet color)
        """
        # Use SpinnerService for unified spinner management
        if (
            self._progress_spinner_id
            and self._app is not None
            and hasattr(self._app, "spinner_service")
        ):
            self._app.spinner_service.stop(self._progress_spinner_id, success, message)
            self._progress_spinner_id = ""
        else:
            # Fallback to direct calls if SpinnerService not available
            from rich.text import Text

            # Stop spinner (shows green/red bullet based on success)
            if hasattr(self.conversation, "stop_tool_execution"):
                self._run_on_ui(lambda: self.conversation.stop_tool_execution(success))

            # Show result line (if message provided)
            if message:
                result_line = Text("  ⎿  ", style=GREY)
                result_line.append(message, style=GREY)
                self._run_on_ui(self.conversation.write, result_line)

    def on_interrupt(self, context: str = "thinking") -> None:
        """Called when execution is interrupted by user.

        Displays the interrupt message based on context:
        - "thinking": Show below/replacing blank line after user prompt
        - "tool": Show below the tool name being executed

        Args:
            context: "thinking" for prompt phase, "tool" for tool execution
        """
        # Guard against duplicate interrupt messages (Fix 3)
        if self._interrupt_shown:
            return
        self._interrupt_shown = True

        try:
            self._cleanup_spinners()
            self._show_interrupt_message(context)
        except Exception as e:
            # Fallback: at minimum ensure processing state is cleared
            logger.error(f"Interrupt handler error: {e}")
            if self.chat_app:
                self.chat_app._is_processing = False

    def _cleanup_spinners(self) -> None:
        """Stop all active spinners during interrupt."""
        # Stop any active spinners via SpinnerService
        if self._app is not None and hasattr(self._app, "spinner_service"):
            # Stop all tracked tool spinners (explicitly pass empty result message)
            for spinner_id in list(self._tool_spinner_ids.values()):
                self._app.spinner_service.stop(spinner_id, success=False, result_message="")
            self._tool_spinner_ids.clear()

            if self._progress_spinner_id:
                self._app.spinner_service.stop(self._progress_spinner_id, success=False, result_message="")
                self._progress_spinner_id = ""

        # Stop spinner first - this removes spinner lines but leaves the blank line after user prompt
        if hasattr(self.conversation, "stop_spinner"):
            self._run_on_ui(self.conversation.stop_spinner)
        if self.chat_app and hasattr(self.chat_app, "_stop_local_spinner"):
            self._run_on_ui(self.chat_app._stop_local_spinner)

    def _show_interrupt_message(self, context: str) -> None:
        """Display the interrupt message based on context.

        Args:
            context: "thinking" or "tool"
        """

        def write_interrupt_replacing_blank_line():
            # Remove trailing blank line if present (SpacingManager adds one after user message)
            # Use simpler detection: try to render last line and check if empty
            if hasattr(self.conversation, "lines") and len(self.conversation.lines) > 0:
                last_line = self.conversation.lines[-1]

                # Check if blank: Strip objects have _segments, Text objects have plain
                is_blank = False
                try:
                    if hasattr(last_line, "_segments"):
                        # Strip object - check if all segments are empty/whitespace
                        text = "".join(seg.text for seg in last_line._segments)
                        is_blank = not text.strip()
                    elif hasattr(last_line, "plain"):
                        is_blank = not last_line.plain.strip()
                    else:
                        # Try string conversion as fallback
                        is_blank = not str(last_line).strip()
                except Exception:
                    pass  # If detection fails, don't remove anything

                if is_blank and hasattr(self.conversation, "_truncate_from"):
                    self.conversation._truncate_from(len(self.conversation.lines) - 1)

            # Write interrupt message using shared utility
            from swecli.ui_textual.utils.interrupt_utils import (
                create_interrupt_text,
                STANDARD_INTERRUPT_MESSAGE,
            )

            interrupt_line = create_interrupt_text(STANDARD_INTERRUPT_MESSAGE)
            self.conversation.write(interrupt_line)

        self._run_on_ui(write_interrupt_replacing_blank_line)

    def on_bash_output_line(self, line: str, is_stderr: bool = False) -> None:
        """Called for each line of bash output during execution.

        For main agent: Output is collected and shown via add_bash_output_box in on_tool_result.
        For subagents: ForwardingUICallback forwards this to parent for nested display.

        Args:
            line: A single line of output from the bash command
            is_stderr: True if this line came from stderr
        """
        # Main agent doesn't stream - output shown in on_tool_result
        pass

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
            result.get("interrupted") if isinstance(result, dict)
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
                from swecli.ui_textual.style_tokens import GREY

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
                    self.on_single_agent_complete(agent_key, success)
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

        # Format the result using the Claude-style formatter
        normalized_args = self._normalize_arguments(tool_args)
        formatted = self.formatter.format_tool_result(tool_name, normalized_args, result)

        # Extract the result line(s) from the formatted output
        # First ⎿ line goes to spinner result placeholder, additional lines displayed separately
        summary_lines: list[str] = []
        collected_lines: list[str] = []
        if isinstance(formatted, str):
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
        if tool_name in {"write_todos", "update_todo", "complete_todo"}:
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
            result.get("interrupted") if isinstance(result, dict)
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

        # Skip ALL display when in collapsed parallel mode
        # The header shows aggregated stats, individual tool results are hidden
        if self._in_parallel_agent_group:
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
        if tool_name in {"write_todos", "update_todo", "complete_todo"}:
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
            result.get("interrupted") if isinstance(result, dict)
            else getattr(result, "interrupted", False)
        )
        if interrupted:
            return

        # Special handling for edit_file - use dedicated diff display with colors
        # This avoids ANSI code stripping that happens in add_nested_tool_sub_results
        if tool_name == "edit_file" and result.get("success"):
            diff_text = result.get("diff", "")
            if diff_text and hasattr(self.conversation, "add_edit_diff_result"):
                # Show summary line first
                file_path = tool_args.get("file_path", "unknown")
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

    def _normalize_arguments(self, tool_args: Any) -> Dict[str, Any]:
        """Ensure tool arguments are represented as a dictionary and normalize URLs for display.

        Delegates to ToolDisplayService for unified logic.
        """
        return self._display_service.normalize_arguments(tool_args)

    def _resolve_paths_in_args(self, tool_args: Dict[str, Any]) -> Dict[str, Any]:
        """Resolve relative paths to absolute paths for display.

        Delegates to ToolDisplayService for unified logic.

        Args:
            tool_args: Tool arguments dict

        Returns:
            Copy of tool_args with paths resolved to absolute paths
        """
        return self._display_service.resolve_paths(tool_args)

    def _run_on_ui(self, func, *args, **kwargs) -> None:
        """Execute a function on the Textual UI thread and WAIT for completion.

        Uses call_from_thread to ensure ordered execution of UI updates.
        This prevents race conditions where messages are displayed out of order.
        """
        if self._app is not None:
            self._app.call_from_thread(func, *args, **kwargs)
        else:
            func(*args, **kwargs)

    def _run_on_ui_non_blocking(self, func, *args, **kwargs) -> None:
        """Execute a function on the Textual UI thread WITHOUT waiting."""
        if self._app is not None:
            self._app.call_from_thread_nonblocking(func, *args, **kwargs)
        else:
            func(*args, **kwargs)

    def _should_skip_due_to_interrupt(self) -> bool:
        """Check if we should skip UI operations due to interrupt.

        Returns:
            True if an interrupt is pending and we should skip UI updates
        """
        if self.chat_app and hasattr(self.chat_app, "runner"):
            runner = self.chat_app.runner
            if hasattr(runner, "query_processor"):
                query_processor = runner.query_processor
                if hasattr(query_processor, "task_monitor"):
                    task_monitor = query_processor.task_monitor
                    if task_monitor and hasattr(task_monitor, "should_interrupt"):
                        return task_monitor.should_interrupt()
        return False

    def on_debug(self, message: str, prefix: str = "DEBUG") -> None:
        """Called to display debug information about execution flow.

        Args:
            message: The debug message to display
            prefix: Optional prefix for categorizing debug messages
        """
        # Skip debug if interrupted
        if self._should_skip_due_to_interrupt():
            return

        # Display debug message in conversation (non-blocking)
        if hasattr(self.conversation, "add_debug_message"):
            self._run_on_ui_non_blocking(self.conversation.add_debug_message, message, prefix)

    def _refresh_todo_panel(self) -> None:
        """Refresh the todo panel with latest state."""
        if not self.chat_app:
            logger.debug("[CALLBACK] _refresh_todo_panel: no chat_app")
            return

        try:
            from swecli.ui_textual.widgets.todo_panel import TodoPanel

            panel = self.chat_app.query_one("#todo-panel", TodoPanel)
            logger.debug("[CALLBACK] _refresh_todo_panel: calling panel.refresh_display()")
            self._run_on_ui(panel.refresh_display)
        except Exception as e:
            # Panel not found or not initialized yet
            logger.debug(f"[CALLBACK] _refresh_todo_panel: panel not found - {e}")
            pass

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
        from swecli.ui_textual.formatters.result_formatter import (
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

    # --- Parallel Agent Group Methods ---

    def on_parallel_agents_start(self, agent_infos: list[dict]) -> None:
        """Called when parallel agents start executing.

        Args:
            agent_infos: List of agent info dicts with keys:
                - agent_type: Type of agent (e.g., "Explore")
                - description: Short description of agent's task
                - tool_call_id: Unique ID for tracking this agent
        """
        print(f"[DEBUG] on_parallel_agents_start: {agent_infos}", file=sys.stderr)

        # Stop thinking spinner if still active (shows "Plotting...", etc.)
        if self._current_thinking:
            self._run_on_ui(self.conversation.stop_spinner)
            self._current_thinking = False

        # Stop any local spinner
        if self.chat_app and hasattr(self.chat_app, "_stop_local_spinner"):
            self._run_on_ui(self.chat_app._stop_local_spinner)

        # Set flag SYNCHRONOUSLY before async UI update to prevent race conditions
        # This ensures on_tool_call sees the flag immediately
        self._in_parallel_agent_group = True

        if hasattr(self.conversation, "on_parallel_agents_start") and self._app is not None:
            print("[DEBUG] Calling conversation.on_parallel_agents_start", file=sys.stderr)
            self._app.call_from_thread(
                self.conversation.on_parallel_agents_start,
                agent_infos,
            )
        else:
            print(
                f"[DEBUG] Missing on_parallel_agents_start or app: has_method={hasattr(self.conversation, 'on_parallel_agents_start')}, _app={self._app}",
                file=sys.stderr,
            )

    def on_parallel_agent_complete(self, tool_call_id: str, success: bool) -> None:
        """Called when a parallel agent completes.

        Args:
            tool_call_id: Unique tool call ID of the agent that completed
            success: Whether the agent succeeded
        """
        if self._interrupt_shown:
            return  # interrupt_cleanup already handled display

        if hasattr(self.conversation, "on_parallel_agent_complete") and self._app is not None:
            self._app.call_from_thread(
                self.conversation.on_parallel_agent_complete,
                tool_call_id,
                success,
            )

    def on_context_usage(self, usage_pct: float) -> None:
        """Update context usage display in the status bar."""
        import logging as _log

        _log.getLogger("swecli.context_debug").info(
            "on_context_usage called: pct=%.2f, has_app=%s",
            usage_pct,
            self._app is not None,
        )
        if not self._app:
            return
        try:
            if hasattr(self._app, "status_bar") and self._app.status_bar is not None:
                self._run_on_ui(self._app.status_bar.set_context_usage, usage_pct)
            else:
                from swecli.ui_textual.widgets.status_bar import StatusBar

                sb = self._app.query_one("#status-bar", StatusBar)
                self._run_on_ui(sb.set_context_usage, usage_pct)
        except Exception:
            pass

    def on_cost_update(self, total_cost_usd: float) -> None:
        """Update session cost display in the status bar."""
        if not self._app:
            return
        try:
            if hasattr(self._app, "status_bar") and self._app.status_bar is not None:
                self._run_on_ui(self._app.status_bar.set_session_cost, total_cost_usd)
            else:
                from swecli.ui_textual.widgets.status_bar import StatusBar

                sb = self._app.query_one("#status-bar", StatusBar)
                self._run_on_ui(sb.set_session_cost, total_cost_usd)
        except Exception:
            pass

    def on_parallel_agents_done(self) -> None:
        """Called when all parallel agents have completed."""
        # Clear flag SYNCHRONOUSLY to allow normal tool call display to resume
        self._in_parallel_agent_group = False

        if self._interrupt_shown:
            return  # interrupt_cleanup already handled display

        if hasattr(self.conversation, "on_parallel_agents_done") and self._app is not None:
            self._app.call_from_thread(self.conversation.on_parallel_agents_done)

    def on_single_agent_start(self, agent_type: str, description: str, tool_call_id: str) -> None:
        """Called when a single subagent starts.

        Args:
            agent_type: Type of agent (e.g., "Explore", "Code-Explorer")
            description: Task description
            tool_call_id: Unique ID for tracking
        """
        if hasattr(self.conversation, "on_single_agent_start") and self._app is not None:
            self._app.call_from_thread(
                self.conversation.on_single_agent_start,
                agent_type,
                description,
                tool_call_id,
            )

    def on_single_agent_complete(self, tool_call_id: str, success: bool = True) -> None:
        """Called when a single subagent completes.

        Args:
            tool_call_id: Unique ID of the agent that completed
            success: Whether the agent succeeded
        """
        if self._interrupt_shown:
            return  # interrupt_cleanup already handled display

        if hasattr(self.conversation, "on_single_agent_complete") and self._app is not None:
            self._app.call_from_thread(
                self.conversation.on_single_agent_complete,
                tool_call_id,
                success,
            )

    def toggle_parallel_expansion(self) -> bool:
        """Toggle the expand/collapse state of parallel agent display.

        Returns:
            New expansion state (True = expanded)
        """
        if hasattr(self.conversation, "toggle_parallel_expansion"):
            return self.conversation.toggle_parallel_expansion()
        return True

    def has_active_parallel_group(self) -> bool:
        """Check if there's an active parallel agent group.

        Returns:
            True if a parallel group is currently active
        """
        if hasattr(self.conversation, "has_active_parallel_group"):
            return self.conversation.has_active_parallel_group()
        return False

    def request_plan_mode_approval(self, message: str) -> bool:
        """Request user approval to enter plan mode.

        Args:
            message: Message explaining why entering plan mode

        Returns:
            True if user approved, False if denied

        Note:
            This is a placeholder implementation that auto-approves.
            Full UI dialog implementation should be added later.
        """
        # TODO: Implement full approval dialog with prompt_toolkit
        # For now, auto-approve to allow the feature to work
        logger.info(f"Plan mode approval requested: {message}")
        return True

    def display_plan_content(self, plan_content: str) -> None:
        """Display plan content in a bordered Markdown box in the conversation log."""
        if hasattr(self.conversation, "add_plan_content_box"):
            self._run_on_ui(self.conversation.add_plan_content_box, plan_content)

    def set_plan_approval_callback(self, callback):
        """Set the callback for plan approval UI interaction.

        Args:
            callback: Function that takes plan_content and returns dict with action/feedback
        """
        self._plan_approval_callback = callback

    def request_plan_approval(
        self,
        plan_content: str,
        allowed_prompts: Optional[list[Dict[str, str]]] = None,
    ) -> Dict[str, str]:
        """Request user approval of a completed plan.

        Args:
            plan_content: The full plan text
            allowed_prompts: Optional list of prompt-based permissions

        Returns:
            Dict with:
                - action: "approve_auto", "approve", or "modify"
                - feedback: Optional feedback for modification
        """
        callback = getattr(self, "_plan_approval_callback", None)
        if callback:
            return callback(plan_content)
        # Fallback: auto-approve (non-interactive contexts)
        return {"action": "approve", "feedback": ""}
