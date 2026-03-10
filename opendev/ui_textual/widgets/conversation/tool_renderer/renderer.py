"""DefaultToolRenderer – composes rendering mixins for tool calls, results, and animations.

Public API consumed by other modules lives here; internal logic is split across mixins.
"""

from __future__ import annotations

import threading
import time
from typing import Any, Dict, List, Optional, Tuple

from rich.text import Text
from textual.strip import Strip
from textual.timer import Timer

from opendev.ui_textual.constants import TOOL_ERROR_SENTINEL
from opendev.ui_textual.style_tokens import (
    CYAN,
    ERROR,
    GREEN_BRIGHT,
    GREY,
    PRIMARY,
    SUBTLE,
    SUCCESS,
)
from opendev.ui_textual.widgets.terminal_box_renderer import (
    TerminalBoxConfig,
    TerminalBoxRenderer,
)
from opendev.ui_textual.widgets.conversation.protocols import RichLogInterface
from opendev.ui_textual.widgets.conversation.spacing_manager import SpacingManager
from opendev.ui_textual.models.collapsible_output import CollapsibleOutput

# Re-export types so consumers can keep importing from this module
from opendev.ui_textual.widgets.conversation.tool_renderer.types import (  # noqa: F401
    TREE_BRANCH,
    TREE_LAST,
    TREE_VERTICAL,
    TREE_CONTINUATION,
    NestedToolState,
    AgentInfo,
    SingleAgentToolLine,
    SingleAgentToolRecord,
    SingleAgentInfo,
    ParallelAgentGroup,
    AgentStats,
)

# Import mixins
from opendev.ui_textual.widgets.conversation.tool_renderer.parallel_agent import ParallelAgentMixin
from opendev.ui_textual.widgets.conversation.tool_renderer.nested_tool import NestedToolMixin
from opendev.ui_textual.widgets.conversation.tool_renderer.bash_output import BashOutputMixin
from opendev.ui_textual.widgets.conversation.tool_renderer.collapsible import CollapsibleMixin
from opendev.ui_textual.widgets.conversation.tool_renderer.result_rendering import (
    ResultRenderingMixin,
)


class DefaultToolRenderer(
    ParallelAgentMixin,
    NestedToolMixin,
    BashOutputMixin,
    CollapsibleMixin,
    ResultRenderingMixin,
):
    """Handles rendering of tool calls, results, and nested execution animations."""

    def __init__(self, log: RichLogInterface, app_callback_interface: Any = None):
        self.log = log
        self.app = app_callback_interface
        self._spacing = SpacingManager(log)

        # Tool execution state
        self._tool_display: Text | None = None
        self._tool_spinner_timer: Timer | None = None
        self._spinner_active = False
        self._spinner_chars = [
            "\u280b",
            "\u2819",
            "\u2839",
            "\u2838",
            "\u283c",
            "\u2834",
            "\u2826",
            "\u2827",
            "\u2807",
            "\u280f",
        ]
        self._spinner_index = 0
        self._tool_call_start: int | None = None
        self._tool_timer_start: float | None = None
        self._tool_last_elapsed: int | None = None

        # Thread timers for blocking operations
        self._tool_thread_timer: threading.Timer | None = None
        self._nested_tool_thread_timer: threading.Timer | None = None

        # Nested tool state - multi-tool tracking for parallel agents
        self._nested_spinner_char = "\u23fa"
        # Multi-tool tracking: (parent, tool_id) -> NestedToolState
        self._nested_tools: Dict[Tuple[str, str], NestedToolState] = {}
        self._nested_tool_timer: Timer | None = None

        # Parallel agent group tracking
        self._parallel_group: Optional[ParallelAgentGroup] = None
        self._parallel_expanded: bool = False  # Default to collapsed view
        self._agent_spinner_states: Dict[str, int] = {}  # tool_call_id -> spinner_index

        # Single agent tracking (treat single agents like parallel group of 1)
        self._single_agent: Optional[SingleAgentInfo] = None
        # Completed single agent info (for Ctrl+O expansion)
        self._completed_single_agent: Optional[SingleAgentInfo] = None
        self._single_agent_expanded: bool = False

        # Animation indices for single agent
        self._header_spinner_index = 0  # For spinner rotation
        self._bullet_gradient_index = 0  # For gradient pulse

        # Legacy single-tool state (for backwards compatibility)
        self._nested_color_index = 0
        self._nested_tool_line: int | None = None
        self._nested_tool_text: Text | None = None
        self._nested_tool_depth: int = 1
        self._nested_tool_timer_start: float | None = None

        # Streaming terminal box state
        self._streaming_box_header_line: int | None = None
        self._streaming_box_width: int = 60
        self._streaming_box_top_line: int | None = None
        self._streaming_box_command: str = ""
        self._streaming_box_working_dir: str = "."
        self._streaming_box_content_lines: list[tuple[str, bool]] = []
        self._streaming_box_config: TerminalBoxConfig | None = None

        # Helper renderer
        self._box_renderer = TerminalBoxRenderer(self._get_box_width)

        # Collapsible output tracking: line_index -> CollapsibleOutput
        self._collapsible_outputs: Dict[int, CollapsibleOutput] = {}
        # Track most recent collapsible output for quick access
        self._most_recent_collapsible: Optional[int] = None

        # Resize coordination
        self._paused_for_resize = False

        # Interrupt state
        self._interrupted: bool = False

    def cleanup(self) -> None:
        """Stop all timers and clear state."""
        self._stop_timers()
        if self._nested_tool_timer:
            self._nested_tool_timer.stop()
            self._nested_tool_timer = None

    # --- Interrupt Cleanup Methods ---

    def _stop_nested_tool_timer(self) -> None:
        """Stop the nested tool animation timer (both Textual and thread timers)."""
        if self._nested_tool_timer:
            self._nested_tool_timer.stop()
            self._nested_tool_timer = None
        if self._nested_tool_thread_timer:
            self._nested_tool_thread_timer.cancel()
            self._nested_tool_thread_timer = None

    def interrupt_cleanup(self) -> None:
        """Collapse subagent display for clean interrupt feedback.

        Deletes per-agent detail rows (for parallel groups) or the tool status line
        (for single agents), updates the header to a red bullet, and stops animation.
        Called from phase 1 (_show_interrupt_feedback) on the UI thread.
        """
        self._interrupted = True
        self._stop_nested_tool_timer()

        if self._parallel_group is not None:
            self._interrupt_parallel_group()
        elif self._single_agent is not None:
            self._interrupt_single_agent()

    def _interrupt_parallel_group(self) -> None:
        """Collapse parallel agent group: delete per-agent rows, update header."""
        group = self._parallel_group

        for agent in group.agents.values():
            if agent.status == "running":
                agent.status = "failed"
        group.completed = True

        self._update_parallel_header()

        lines_to_delete = []
        for agent in group.agents.values():
            lines_to_delete.append(agent.status_line)
            lines_to_delete.append(agent.line_number)
        lines_to_delete.sort(reverse=True)

        for line_num in lines_to_delete:
            if line_num < len(self.log.lines):
                del self.log.lines[line_num]

        if lines_to_delete:
            first_line = min(lines_to_delete)
            if hasattr(self.log, "_block_registry"):
                self.log._block_registry.remove_blocks_from(first_line)

        if hasattr(self.log, "_line_cache"):
            self.log._line_cache.clear()
        if hasattr(self.log, "_recalculate_virtual_size"):
            self.log._recalculate_virtual_size()

        self._parallel_group = None
        self._agent_spinner_states.clear()
        self._nested_tools.clear()
        self.log.refresh()

    def _interrupt_single_agent(self) -> None:
        """Collapse single agent: delete all tool lines, update header to red bullet."""
        agent = self._single_agent
        agent.status = "failed"

        header_row = Text()
        header_row.append("\u23fa ", style=ERROR)
        header_row.append(f"{agent.agent_type}(", style=CYAN)
        header_row.append(agent.description, style=PRIMARY)
        header_row.append(")", style=CYAN)
        strip = self._text_to_strip(header_row)
        if agent.header_line < len(self.log.lines):
            self.log.lines[agent.header_line] = strip

        # Collect all tool lines to delete (tool_line + extras + overflow)
        lines_to_delete = [agent.tool_line]
        for tl in agent.active_tool_lines.values():
            if tl.line_number != agent.tool_line:
                lines_to_delete.append(tl.line_number)
        if agent.overflow_line is not None:
            lines_to_delete.append(agent.overflow_line)

        # Delete in reverse order to preserve indices
        for line_num in sorted(lines_to_delete, reverse=True):
            if line_num < len(self.log.lines):
                del self.log.lines[line_num]

        first_deleted = min(lines_to_delete) if lines_to_delete else agent.tool_line
        if hasattr(self.log, "_block_registry"):
            self.log._block_registry.remove_blocks_from(first_deleted)

        if hasattr(self.log, "_line_cache"):
            self.log._line_cache.clear()
        if hasattr(self.log, "_recalculate_virtual_size"):
            self.log._recalculate_virtual_size()

        self._single_agent = None
        self._nested_tools.clear()
        self.log.refresh()

    def reset_interrupt(self) -> None:
        """Reset interrupt flag for a new agent run."""
        self._interrupted = False

    # --- Resize Coordination Methods ---

    def pause_for_resize(self) -> None:
        """Stop animation timers for resize."""
        self._paused_for_resize = True
        self._stop_timers()
        if self._nested_tool_timer:
            self._nested_tool_timer.stop()
            self._nested_tool_timer = None

    def adjust_indices(self, delta: int, first_affected: int) -> None:
        """Adjust all tracked line indices by delta.

        Args:
            delta: Number of lines added (positive) or removed (negative)
            first_affected: First line index affected by the change
        """

        def adj(idx: Optional[int]) -> Optional[int]:
            return idx + delta if idx is not None and idx >= first_affected else idx

        self._tool_call_start = adj(self._tool_call_start)
        self._nested_tool_line = adj(self._nested_tool_line)

        for state in self._nested_tools.values():
            if state.line_number >= first_affected:
                state.line_number += delta

        if self._parallel_group is not None:
            if self._parallel_group.header_line >= first_affected:
                self._parallel_group.header_line += delta
            for agent in self._parallel_group.agents.values():
                if agent.line_number >= first_affected:
                    agent.line_number += delta
                if agent.status_line >= first_affected:
                    agent.status_line += delta

        if self._single_agent is not None:
            if self._single_agent.header_line >= first_affected:
                self._single_agent.header_line += delta
            if self._single_agent.tool_line >= first_affected:
                self._single_agent.tool_line += delta
            for tl in self._single_agent.active_tool_lines.values():
                if tl.line_number >= first_affected:
                    tl.line_number += delta
            if (
                self._single_agent.overflow_line is not None
                and self._single_agent.overflow_line >= first_affected
            ):
                self._single_agent.overflow_line += delta

        self._streaming_box_header_line = adj(self._streaming_box_header_line)
        self._streaming_box_top_line = adj(self._streaming_box_top_line)

        new_collapsibles: Dict[int, CollapsibleOutput] = {}
        for start, coll in self._collapsible_outputs.items():
            new_start = start + delta if start >= first_affected else start
            coll.start_line = new_start
            if coll.end_line >= first_affected:
                coll.end_line += delta
            new_collapsibles[new_start] = coll
        self._collapsible_outputs = new_collapsibles

        self._most_recent_collapsible = adj(self._most_recent_collapsible)

    def resume_after_resize(self) -> None:
        """Restart animations after resize."""
        self._paused_for_resize = False

        has_active = (
            self._nested_tools
            or self._nested_tool_line is not None
            or (
                self._parallel_group is not None
                and any(a.status == "running" for a in self._parallel_group.agents.values())
            )
            or (self._single_agent is not None and self._single_agent.status == "running")
        )

        if has_active and self._nested_tool_timer is None:
            self._animate_nested_tool_spinner()

    def _stop_timers(self) -> None:
        if self._tool_spinner_timer:
            self._tool_spinner_timer.stop()
            self._tool_spinner_timer = None
        if self._tool_thread_timer:
            self._tool_thread_timer.cancel()
            self._tool_thread_timer = None
        if self._nested_tool_thread_timer:
            self._nested_tool_thread_timer.cancel()
            self._nested_tool_thread_timer = None

    def _get_box_width(self) -> int:
        return self.log.virtual_size.width

    # --- Standard Tool Calls ---

    def add_tool_call(self, display: Text | str, *_: Any) -> None:
        self._spacing.before_tool_call()

        if isinstance(display, Text):
            self._tool_display = display.copy()
        else:
            self._tool_display = Text(str(display), style=PRIMARY)

        if hasattr(self.log, "smart_scroll_end"):
            self.log.smart_scroll_end(animate=False)
        else:
            self.log.scroll_end(animate=False)
        self._tool_call_start = len(self.log.lines)
        self._tool_timer_start = None
        self._tool_last_elapsed = None
        self._write_tool_call_line("\u23fa")

    def start_tool_execution(self) -> None:
        if self._tool_display is None:
            return

        self._spinner_active = True
        self._spinner_index = 0
        self._tool_timer_start = time.monotonic()
        self._tool_last_elapsed = None
        self._render_tool_spinner_frame()
        self._schedule_tool_spinner()

    def stop_tool_execution(self, success: bool = True) -> None:
        self._spinner_active = False
        if self._tool_timer_start is not None:
            elapsed_raw = time.monotonic() - self._tool_timer_start
            self._tool_last_elapsed = max(round(elapsed_raw), 0)
        else:
            self._tool_last_elapsed = None
        self._tool_timer_start = None

        if self._tool_call_start is not None and self._tool_display is not None:
            self._replace_tool_call_line("\u23fa", success=success)

        self._tool_display = None
        self._tool_call_start = None
        self._spinner_index = 0
        self._stop_timers()

    def update_progress_text(self, message: str | Text) -> None:
        if self._tool_call_start is None:
            self.add_tool_call(message)
            self.start_tool_execution()
            return

        if isinstance(message, Text):
            self._tool_display = message.copy()
        else:
            self._tool_display = Text(str(message), style=PRIMARY)

        if self._spinner_active:
            self._render_tool_spinner_frame()

    def add_tool_result(self, result: str) -> None:
        """Add a tool result to the log."""
        try:
            result_plain = Text.from_markup(result).plain
        except Exception:
            result_plain = result

        header, diff_lines = self._extract_edit_payload(result_plain)
        if header:
            self._write_edit_result(header, diff_lines)
        else:
            self._write_generic_tool_result(result_plain)

        self._spacing.after_tool_result()

    def add_tool_result_continuation(self, lines: list[str]) -> None:
        """Add continuation lines for tool result."""
        if not lines:
            return

        def text_to_strip(text: Text) -> Strip:
            from rich.console import Console

            console = Console(width=1000, force_terminal=True, no_color=False)
            segments = list(text.render(console))
            return Strip(segments)

        spacing_line = getattr(self.log, "_pending_spacing_line", None)

        for i, line in enumerate(lines):
            formatted = Text("     ", style=GREY)
            formatted.append(line, style=SUBTLE)

            if i == 0 and spacing_line is not None and spacing_line < len(self.log.lines):
                self.log.lines[spacing_line] = text_to_strip(formatted)
            else:
                self.log.write(formatted, wrappable=False)

        self.log._pending_spacing_line = None
        self._spacing.after_tool_result_continuation()

    # --- Single Agent Display ---

    def has_expandable_single_agent(self) -> bool:
        """Check if there's a completed single agent that can be expanded."""
        return (
            self._completed_single_agent is not None
            and len(self._completed_single_agent.tool_records) > 0
        )

    def toggle_single_agent_expansion(self) -> tuple[bool, int, int]:
        """Toggle expand/collapse of the last completed single agent's tool calls.

        Returns:
            Tuple of (new expansion state, line delta, first affected line index)
        """
        agent = self._completed_single_agent
        if agent is None or not agent.tool_records:
            return False, 0, 0

        self._single_agent_expanded = not self._single_agent_expanded

        if self._single_agent_expanded:
            insert_at = agent.tool_line + 1
            lines_to_insert = []
            for record in agent.tool_records:
                row = Text()
                row.append("    ", style="")
                status_char = "\u2713" if record.success else "\u2717"
                status_color = SUCCESS if record.success else ERROR
                row.append(f"{status_char} ", style=status_color)
                row.append(record.display_text, style=SUBTLE)
                lines_to_insert.append(self._text_to_strip(row))

            # Add failure reason as last line when expanded
            if agent.failure_reason:
                row = Text()
                row.append("    ", style="")
                row.append("\u26a0 ", style=ERROR)
                row.append(agent.failure_reason[:200], style=ERROR)
                lines_to_insert.append(self._text_to_strip(row))

            for i, strip in enumerate(lines_to_insert):
                self.log.lines.insert(insert_at + i, strip)

            delta = len(lines_to_insert)
            self._update_single_agent_summary(agent, "(ctrl+o to collapse)")

            if hasattr(self.log, "_recalculate_virtual_size"):
                self.log._recalculate_virtual_size()
            self.log.refresh()
        else:
            insert_at = agent.tool_line + 1
            count = len(agent.tool_records)
            if agent.failure_reason:
                count += 1  # Account for the failure reason line
            del self.log.lines[insert_at : insert_at + count]

            delta = -count
            hint = " (ctrl+o to expand)"
            self._update_single_agent_summary(agent, hint)

            if hasattr(self.log, "_recalculate_virtual_size"):
                self.log._recalculate_virtual_size()
            self.log.refresh()

        return self._single_agent_expanded, delta, insert_at

    def _update_single_agent_summary(self, agent: SingleAgentInfo, hint: str) -> None:
        """Update the tool line summary with the given hint text."""
        tool_word = "tool use" if agent.tool_count == 1 else "tool uses"
        elapsed = int(time.monotonic() - agent.start_time)
        tool_row = Text()
        tool_row.append("  \u23bf  ", style=GREY)
        if agent.status == "failed":
            status_text = f"Failed ({agent.tool_count} {tool_word} \u00b7 {elapsed}s)"
            if agent.failure_reason and not self._single_agent_expanded:
                reason = agent.failure_reason.split("\n")[0][:80]
                status_text += f": {reason}"
        else:
            status_text = f"Done ({agent.tool_count} {tool_word} \u00b7 {elapsed}s)"
        style = ERROR if agent.status == "failed" else SUBTLE
        tool_row.append(status_text, style=style)
        if hint:
            tool_row.append(f" {hint}", style=f"{SUBTLE} italic")
        if agent.tool_line < len(self.log.lines):
            self.log.lines[agent.tool_line] = self._text_to_strip(tool_row)

    def on_single_agent_start(self, agent_type: str, description: str, tool_call_id: str) -> None:
        """Called when a single agent starts (non-parallel execution).

        Args:
            agent_type: Type of agent (e.g., "Explore", "Code-Explorer")
            description: Task description
            tool_call_id: Unique ID for tracking
        """
        self._spacing.before_single_agent()

        header = Text()
        header.append("\u280b ", style=CYAN)
        header.append(f"{agent_type}(", style=CYAN)
        header.append(description, style=PRIMARY)
        header.append(")", style=CYAN)
        self.log.write(header, scroll_end=True, animate=False, wrappable=False)
        header_line = len(self.log.lines) - 1

        tool_row = Text()
        tool_row.append("  \u23bf  ", style=GREY)
        tool_row.append("Initializing...", style=SUBTLE)
        self.log.write(tool_row, scroll_end=True, animate=False, wrappable=False)
        tool_line_num = len(self.log.lines) - 1

        self._single_agent = SingleAgentInfo(
            agent_type=agent_type,
            description=description,
            tool_call_id=tool_call_id,
            header_line=header_line,
            tool_line=tool_line_num,
        )

        self._header_spinner_index = 0
        self._bullet_gradient_index = 0

        self._start_nested_tool_timer()

    def _update_header_spinner(self) -> None:
        """Update header line with rotating spinner."""
        if self._single_agent is None:
            return

        agent = self._single_agent
        if agent.header_line >= len(self.log.lines):
            return

        idx = self._header_spinner_index % len(self._spinner_chars)
        self._header_spinner_index += 1
        spinner_char = self._spinner_chars[idx]

        row = Text()
        row.append(f"{spinner_char} ", style=CYAN)
        row.append(f"{agent.agent_type}(", style=CYAN)
        row.append(agent.description, style=PRIMARY)
        row.append(")", style=CYAN)

        strip = self._text_to_strip(row)
        self.log.lines[agent.header_line] = strip
        self.log.refresh_line(agent.header_line)

    def _update_single_agent_tool_line(self) -> None:
        """Update single agent's current tool line."""
        if self._single_agent is None or self._single_agent.tool_line >= len(self.log.lines):
            return

        agent = self._single_agent
        row = Text()
        row.append("  \u23bf  ", style=GREY)
        row.append(agent.current_tool, style=SUBTLE)

        strip = self._text_to_strip(row)
        self.log.lines[agent.tool_line] = strip
        self.log.refresh_line(agent.tool_line)

    def on_single_agent_complete(
        self, tool_call_id: str, success: bool = True, failure_reason: str = ""
    ) -> None:
        """Called when a single agent completes.

        Args:
            tool_call_id: Unique ID of the agent that completed
            success: Whether the agent succeeded
            failure_reason: Why the agent failed (API error, etc.)
        """
        if self._interrupted:
            self._single_agent = None
            return

        if self._single_agent is None:
            return

        if tool_call_id and self._single_agent.tool_call_id != tool_call_id:
            return

        agent = self._single_agent
        agent.status = "completed" if success else "failed"
        agent.failure_reason = failure_reason

        header_row = Text()
        header_row.append("\u23fa ", style=GREEN_BRIGHT if success else ERROR)
        header_row.append(f"{agent.agent_type}(", style=CYAN)
        header_row.append(agent.description, style=PRIMARY)
        header_row.append(")", style=CYAN)

        strip = self._text_to_strip(header_row)
        if agent.header_line < len(self.log.lines):
            self.log.lines[agent.header_line] = strip
            self.log.refresh_line(agent.header_line)

        # Delete extra tool lines (all lines after tool_line that we appended)
        extra_lines = sorted(
            [
                tl.line_number
                for tl in agent.active_tool_lines.values()
                if tl.line_number != agent.tool_line
            ]
        )
        if agent.overflow_line is not None:
            extra_lines.append(agent.overflow_line)
        # Delete in reverse order to preserve indices
        for line_num in sorted(extra_lines, reverse=True):
            if line_num < len(self.log.lines):
                del self.log.lines[line_num]
        if extra_lines:
            if hasattr(self.log, "_line_cache"):
                self.log._line_cache.clear()
            if hasattr(self.log, "_recalculate_virtual_size"):
                self.log._recalculate_virtual_size()

        elapsed = int(time.monotonic() - agent.start_time)
        tool_word = "tool use" if agent.tool_count == 1 else "tool uses"
        tool_row = Text()
        tool_row.append("  \u23bf  ", style=GREY)
        if success:
            tool_row.append(
                f"Done ({agent.tool_count} {tool_word} \u00b7 {elapsed}s)", style=SUBTLE
            )
        else:
            fail_text = f"Failed ({agent.tool_count} {tool_word} \u00b7 {elapsed}s)"
            if agent.failure_reason:
                # Show truncated reason on the summary line
                reason = agent.failure_reason.split("\n")[0][:80]
                fail_text += f": {reason}"
            tool_row.append(fail_text, style=ERROR)
        if agent.tool_records:
            tool_row.append(" (ctrl+o to expand)", style=f"{SUBTLE} italic")

        strip = self._text_to_strip(tool_row)
        if agent.tool_line < len(self.log.lines):
            self.log.lines[agent.tool_line] = strip
            self.log.refresh_line(agent.tool_line)

        self._spacing.after_single_agent()

        self._completed_single_agent = agent
        self._single_agent_expanded = False
        self._single_agent = None
        self.log.refresh()

    # --- Utility / Helpers ---

    def _text_to_strip(self, text: Text) -> Strip:
        """Convert Text to Strip for line replacement.

        Args:
            text: Rich Text object to convert

        Returns:
            Strip object for use in log.lines
        """
        from rich.console import Console

        console = Console(width=1000, force_terminal=True, no_color=False)
        segments = list(text.render(console))
        return Strip(segments)

    def _truncate_from(self, index: int) -> None:
        if index >= len(self.log.lines):
            return

        protected_lines = getattr(self.log, "_protected_lines", set())
        protected_in_range = [i for i in protected_lines if i >= index]

        if protected_in_range:
            non_protected = [
                i for i in range(index, len(self.log.lines)) if i not in protected_lines
            ]
            if not non_protected:
                return
            for i in sorted(non_protected, reverse=True):
                if i < len(self.log.lines):
                    del self.log.lines[i]
        else:
            del self.log.lines[index:]

        if hasattr(self.log, "_block_registry"):
            self.log._block_registry.remove_blocks_from(index)

        if hasattr(self.log, "_line_cache"):
            self.log._line_cache.clear()

        if protected_lines:
            new_protected = set()
            for p in protected_lines:
                if p < index:
                    new_protected.add(p)
                elif p in protected_in_range:
                    deleted_before = len([i for i in range(index, p) if i not in protected_lines])
                    new_protected.add(p - deleted_before)

            if hasattr(self.log, "_protected_lines"):
                self.log._protected_lines.clear()
                self.log._protected_lines.update(new_protected)

        if hasattr(self.log, "virtual_size"):
            pass
        self.log.refresh()

    # --- Spinner Animation ---

    def _schedule_tool_spinner(self) -> None:
        if self._tool_spinner_timer:
            self._tool_spinner_timer.stop()
        if self._tool_thread_timer:
            self._tool_thread_timer.cancel()

        self._tool_spinner_timer = self.log.set_timer(0.12, self._animate_tool_spinner)

        self._tool_thread_timer = threading.Timer(0.12, self._thread_animate_tool)
        self._tool_thread_timer.daemon = True
        self._tool_thread_timer.start()

    def _thread_animate_tool(self) -> None:
        if not self._spinner_active:
            return
        try:
            if self.app:
                self.app.call_from_thread(self._animate_tool_spinner)
        except Exception:
            pass

    def _animate_tool_spinner(self) -> None:
        if not self._spinner_active:
            return
        self._advance_tool_frame()
        self._schedule_tool_spinner()

    def _advance_tool_frame(self) -> None:
        if not self._spinner_active:
            return
        self._spinner_index = (self._spinner_index + 1) % len(self._spinner_chars)
        self._render_tool_spinner_frame()

    def _render_tool_spinner_frame(self) -> None:
        if self._tool_call_start is None:
            return
        char = self._spinner_chars[self._spinner_index]
        self._replace_tool_call_line(char)

    def _replace_tool_call_line(self, prefix: str, success: bool = True) -> None:
        if self._tool_call_start is None or self._tool_display is None:
            return

        if self._tool_call_start >= len(self.log.lines):
            return

        elapsed_str = ""
        if self._tool_timer_start is not None:
            elapsed = int(time.monotonic() - self._tool_timer_start)
            elapsed_str = f" ({elapsed}s)"
        elif self._tool_last_elapsed is not None:
            elapsed_str = f" ({self._tool_last_elapsed}s)"

        formatted = Text()

        if len(prefix) == 1 and prefix in self._spinner_chars:
            style = GREEN_BRIGHT
        elif not success:
            style = ERROR
        elif prefix == "\u23fa":
            style = GREEN_BRIGHT
        else:
            style = GREEN_BRIGHT

        formatted.append(f"{prefix} ", style=style)
        formatted.append_text(self._tool_display)
        formatted.append(elapsed_str, style=GREY)

        from rich.console import Console

        width = self._get_box_width()
        console = Console(width=width, force_terminal=True, no_color=False)
        segments = list(formatted.render(console))
        strip = Strip(segments)

        self.log.lines[self._tool_call_start] = strip
        self.log.refresh_line(self._tool_call_start)
        if self.app and hasattr(self.app, "refresh"):
            self.app.refresh()

    def _write_tool_call_line(self, prefix: str) -> None:
        formatted = Text()
        formatted.append(f"{prefix} ", style=GREEN_BRIGHT)
        if self._tool_display:
            formatted.append_text(self._tool_display)
        formatted.append(" (0s)", style=GREY)

        self.log.write(formatted, wrappable=False)

    def add_extra_edit_block(
        self,
        file_path: str,
        start_line: int,
        additions: int,
        removals: int,
        hunk_entries: list,
    ) -> None:
        """Render a completed Edit block for an additional hunk (not spinner-managed)."""
        self._spacing.before_tool_call()

        # Header: ⏺ Edit(file.py) at line N
        header = Text()
        header.append("⏺ ", style=GREEN_BRIGHT)
        header.append("Edit", style=PRIMARY)
        header.append(f"({file_path}) at line {start_line}", style=GREY)
        self.log.write(header, wrappable=False)

        # Summary: ⎿  Updated file.py (N additions, M removals)
        def _plural(count: int, singular: str) -> str:
            return f"{count} {singular}" if count == 1 else f"{count} {singular}s"

        parts = []
        if additions:
            parts.append(_plural(additions, "addition"))
        if removals:
            parts.append(_plural(removals, "removal"))
        summary_text = ", ".join(parts) if parts else "no changes"

        summary = Text("  ⎿  ", style=GREY)
        summary.append(f"Updated {file_path} ({summary_text})", style=SUBTLE)
        self.log.write(summary, wrappable=False)

        # Diff lines
        for entry_type, line_no, content in hunk_entries:
            formatted = Text("     ")
            display_no = f"{line_no:>4} " if line_no is not None else "     "
            sanitized = content.replace("\t", "    ")

            if entry_type == "add":
                formatted.append(display_no, style=SUBTLE)
                formatted.append("+ ", style=SUCCESS)
                formatted.append(sanitized, style=SUCCESS)
            elif entry_type == "del":
                formatted.append(display_no, style=SUBTLE)
                formatted.append("- ", style=ERROR)
                formatted.append(sanitized, style=ERROR)
            else:
                formatted.append(display_no, style=SUBTLE)
                formatted.append("  ", style=SUBTLE)
                formatted.append(sanitized, style=SUBTLE)
            self.log.write(formatted, wrappable=False)

        self._spacing.after_tool_result()

    # --- Tool Result Parsing Helpers ---

    def _extract_edit_payload(self, text: str) -> Tuple[str, List[str]]:
        lines = text.splitlines()
        if not lines:
            return "", []

        if lines[0].startswith("<<<<") or lines[0].startswith("Replaced lines"):
            pass

        header = ""
        diff_lines = []

        if "Editing file" in lines[0] or "Applied edit" in lines[0] or "Updated " in lines[0]:
            header = lines[0]
            diff_lines = lines[1:]
            return header, diff_lines

        return "", []

    def _write_edit_result(self, header: str, diff_lines: list[str]) -> None:
        self.log.write(Text(f"  \u23bf  {header}", style=SUBTLE), wrappable=True)

        for line in diff_lines:
            formatted = Text("     ")
            is_addition = len(line) > 4 and line[4] == "+"
            is_deletion = len(line) > 4 and line[4] == "-"
            if is_addition:
                formatted.append(line, style=SUCCESS)
            elif is_deletion:
                formatted.append(line, style=ERROR)
            else:
                formatted.append(line, style=SUBTLE)
            self.log.write(formatted, wrappable=False)

    def _write_generic_tool_result(self, text: str) -> None:
        lines = text.rstrip("\n").splitlines() or [text]
        for i, raw_line in enumerate(lines):
            prefix = "  \u23bf  " if i == 0 else "     "
            line = Text(prefix, style=GREY)
            message = raw_line.rstrip("\n")
            is_error = False
            is_interrupted = False

            if message.startswith(TOOL_ERROR_SENTINEL):
                is_error = True
                message = message[len(TOOL_ERROR_SENTINEL) :].lstrip()
            elif message.startswith("::interrupted::"):
                is_interrupted = True
                message = message[len("::interrupted::") :].lstrip()

            if is_interrupted:
                line.append(message, style=f"bold {ERROR}")
            else:
                line.append(message, style=ERROR if is_error else SUBTLE)
            self.log.write(line, wrappable=False)
