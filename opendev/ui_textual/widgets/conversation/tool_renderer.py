from __future__ import annotations

import re
import threading
import time
from dataclasses import dataclass, field
from typing import Any, Dict, List, Optional, Tuple

from rich.console import Group
from rich.panel import Panel
from rich.syntax import Syntax
from rich.text import Text
from textual.strip import Strip
from textual.timer import Timer

from opendev.ui_textual.constants import TOOL_ERROR_SENTINEL
from opendev.ui_textual.style_tokens import (
    BLUE_PATH,
    CYAN,
    ERROR,
    GREEN_BRIGHT,
    GREEN_GRADIENT,
    GREEN_PROMPT,
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
from opendev.ui_textual.utils.output_summarizer import summarize_output, get_expansion_hint

# Tree connector characters
TREE_BRANCH = "├─"
TREE_LAST = "└─"
TREE_VERTICAL = "│"
TREE_CONTINUATION = "⎿"


@dataclass
class NestedToolState:
    """State tracking for a single nested tool call."""

    line_number: int
    tool_text: Text
    depth: int
    timer_start: float
    color_index: int = 0
    parent: str = ""
    tool_id: str = ""


@dataclass
class AgentInfo:
    """Info for a single parallel agent tracked by tool_call_id."""

    agent_type: str
    description: str
    tool_call_id: str
    line_number: int = 0  # Line for agent row
    status_line: int = 0  # Line for status/current tool
    tool_count: int = 0  # Total tool call count
    current_tool: str = "Initializing...."
    status: str = "running"  # running, completed, failed
    is_last: bool = False  # For tree connector rendering


@dataclass
@dataclass
class SingleAgentToolRecord:
    """Record of a single tool call within a subagent execution."""

    tool_name: str
    display_text: str
    success: bool = True
    elapsed_s: int = 0


@dataclass
class SingleAgentInfo:
    """Info for a single (non-parallel) agent execution."""

    agent_type: str
    description: str
    tool_call_id: str
    header_line: int = 0  # Line for header "⠋ Explore(description)"
    tool_line: int = 0  # Line for "  ⎿  current_tool"
    tool_count: int = 0  # Total tool call count
    current_tool: str = "Initializing..."
    status: str = "running"
    start_time: float = field(default_factory=time.monotonic)
    tool_records: List["SingleAgentToolRecord"] = field(default_factory=list)


@dataclass
class ParallelAgentGroup:
    """Tracks a group of parallel agents for collapsed display."""

    agents: Dict[str, AgentInfo] = field(default_factory=dict)  # key = tool_call_id
    header_line: int = 0
    expanded: bool = False
    start_time: float = field(default_factory=time.monotonic)
    completed: bool = False


@dataclass
class AgentStats:
    """Stats tracking for a single agent type in a parallel group (legacy)."""

    tool_count: int = 0
    token_count: int = 0
    current_tool: str = ""
    status: str = "running"  # running, completed, failed
    agent_count: int = 1  # Number of agents of this type (for "Running 2 Explore agents")
    completed_count: int = 0  # Number of agents that have completed


class DefaultToolRenderer:
    """Handles rendering of tool calls, results, and nested execution animations."""

    def __init__(self, log: RichLogInterface, app_callback_interface: Any = None):
        self.log = log
        self.app = app_callback_interface
        self._spacing = SpacingManager(log)

        # Tool execution state
        self._tool_display: Text | None = None
        self._tool_spinner_timer: Timer | None = None
        self._spinner_active = False
        self._spinner_chars = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"]
        self._spinner_index = 0
        self._tool_call_start: int | None = None
        self._tool_timer_start: float | None = None
        self._tool_last_elapsed: int | None = None

        # Thread timers for blocking operations
        self._tool_thread_timer: threading.Timer | None = None
        self._nested_tool_thread_timer: threading.Timer | None = None

        # Nested tool state - multi-tool tracking for parallel agents
        self._nested_spinner_char = "⏺"
        # Multi-tool tracking: (parent, tool_id) -> NestedToolState
        self._nested_tools: Dict[Tuple[str, str], NestedToolState] = {}
        self._nested_tool_timer: Timer | None = None
        self._nested_tool_thread_timer: threading.Timer | None = None

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
        self._header_spinner_index = 0  # For ⠋⠙⠹... rotation
        self._bullet_gradient_index = 0  # For ⏺ gradient pulse

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

        # Interrupt state — set by interrupt_cleanup() to suppress post-interrupt rendering
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

        # Mark all agents as failed
        for agent in group.agents.values():
            if agent.status == "running":
                agent.status = "failed"
        group.completed = True

        # Update header to red bullet with completed summary
        self._update_parallel_header()

        # Collect all per-agent lines (agent row + status line), delete bottom-first
        lines_to_delete = []
        for agent in group.agents.values():
            lines_to_delete.append(agent.status_line)
            lines_to_delete.append(agent.line_number)
        lines_to_delete.sort(reverse=True)

        for line_num in lines_to_delete:
            if line_num < len(self.log.lines):
                del self.log.lines[line_num]

        # Clean up block registry and caches
        if lines_to_delete:
            first_line = min(lines_to_delete)
            if hasattr(self.log, "_block_registry"):
                self.log._block_registry.remove_blocks_from(first_line)

        if hasattr(self.log, "_line_cache"):
            self.log._line_cache.clear()
        if hasattr(self.log, "_recalculate_virtual_size"):
            self.log._recalculate_virtual_size()

        # Clear parallel state
        self._parallel_group = None
        self._agent_spinner_states.clear()
        self._nested_tools.clear()
        self.log.refresh()

    def _interrupt_single_agent(self) -> None:
        """Collapse single agent: delete tool line, update header to red bullet."""
        agent = self._single_agent
        agent.status = "failed"

        # Update header to red bullet
        header_row = Text()
        header_row.append("⏺ ", style=ERROR)
        header_row.append(f"{agent.agent_type}(", style=CYAN)
        header_row.append(agent.description, style=PRIMARY)
        header_row.append(")", style=CYAN)
        strip = self._text_to_strip(header_row)
        if agent.header_line < len(self.log.lines):
            self.log.lines[agent.header_line] = strip

        # Delete tool line (the "⎿  current_tool" line below the header)
        if agent.tool_line < len(self.log.lines):
            del self.log.lines[agent.tool_line]
            if hasattr(self.log, "_block_registry"):
                self.log._block_registry.remove_blocks_from(agent.tool_line)

        if hasattr(self.log, "_line_cache"):
            self.log._line_cache.clear()
        if hasattr(self.log, "_recalculate_virtual_size"):
            self.log._recalculate_virtual_size()

        # Clear single agent state
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
            """Adjust a single index if affected."""
            return idx + delta if idx is not None and idx >= first_affected else idx

        # Adjust tool call tracking
        self._tool_call_start = adj(self._tool_call_start)
        self._nested_tool_line = adj(self._nested_tool_line)

        # Adjust nested tools (multi-tool tracking)
        for state in self._nested_tools.values():
            if state.line_number >= first_affected:
                state.line_number += delta

        # Adjust parallel group
        if self._parallel_group is not None:
            if self._parallel_group.header_line >= first_affected:
                self._parallel_group.header_line += delta
            for agent in self._parallel_group.agents.values():
                if agent.line_number >= first_affected:
                    agent.line_number += delta
                if agent.status_line >= first_affected:
                    agent.status_line += delta

        # Adjust single agent
        if self._single_agent is not None:
            if self._single_agent.header_line >= first_affected:
                self._single_agent.header_line += delta
            if self._single_agent.tool_line >= first_affected:
                self._single_agent.tool_line += delta

        # Adjust streaming box lines
        self._streaming_box_header_line = adj(self._streaming_box_header_line)
        self._streaming_box_top_line = adj(self._streaming_box_top_line)

        # Adjust collapsible outputs (rebuild dict with new keys)
        new_collapsibles: Dict[int, CollapsibleOutput] = {}
        for start, coll in self._collapsible_outputs.items():
            new_start = start + delta if start >= first_affected else start
            coll.start_line = new_start
            if coll.end_line >= first_affected:
                coll.end_line += delta
            new_collapsibles[new_start] = coll
        self._collapsible_outputs = new_collapsibles

        # Adjust most recent collapsible pointer
        self._most_recent_collapsible = adj(self._most_recent_collapsible)

    def resume_after_resize(self) -> None:
        """Restart animations after resize."""
        self._paused_for_resize = False

        # Check if there are any active animations that need to be restarted
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

        self.log.scroll_end(animate=False)
        self._tool_call_start = len(self.log.lines)
        self._tool_timer_start = None
        self._tool_last_elapsed = None
        self._write_tool_call_line("⏺")

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
            self._replace_tool_call_line("⏺", success=success)

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
        """Add a tool result to the log.

        Note: We intentionally do NOT add a trailing blank line here.
        Spacing is handled by the NEXT element via before_* methods.
        This prevents double spacing.
        """
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
        """Add continuation lines for tool result (no ⎿ prefix, just space indentation).

        Used for diff lines that follow a summary line. The summary line already
        has the ⎿ prefix in the result placeholder, so these continuation lines
        just need space indentation to align.

        Structure:
        - ⎿  Summary line       (result placeholder, updated by spinner_service.stop)
        -    First diff line    (overwrites spacing placeholder - no gap)
        -    More diff lines...
        -    Last diff line
        - [blank line]          (added at end for spacing before next tool)
        """
        if not lines:
            return

        # Convert line to Strip helper
        def text_to_strip(text: Text) -> "Strip":
            from rich.console import Console
            from textual.strip import Strip

            console = Console(width=1000, force_terminal=True, no_color=False)
            segments = list(text.render(console))
            return Strip(segments)

        # Check if we have a pending spacing line to overwrite
        spacing_line = getattr(self.log, "_pending_spacing_line", None)

        for i, line in enumerate(lines):
            formatted = Text("     ", style=GREY)  # 5 spaces to align with ⎿ content
            formatted.append(line, style=SUBTLE)

            if i == 0 and spacing_line is not None and spacing_line < len(self.log.lines):
                # Overwrite spacing placeholder with first diff line (no gap)
                self.log.lines[spacing_line] = text_to_strip(formatted)
            else:
                # Tool result continuation lines preserve formatting, don't re-wrap
                self.log.write(formatted, wrappable=False)

        # Clear the pending spacing line
        self.log._pending_spacing_line = None

        # Add blank line at end for spacing before next tool
        self._spacing.after_tool_result_continuation()

    # --- Nested Tool Calls ---

    def add_nested_tool_call(
        self,
        display: Text | str,
        depth: int,
        parent: str,
        tool_id: str = "",
        is_last: bool = False,
    ) -> None:
        """Add a nested tool call with multi-tool tracking support.

        Args:
            display: Tool display text
            depth: Nesting depth (1 = direct child)
            parent: Parent agent name
            tool_id: Unique tool call ID for tracking parallel tools
            is_last: Whether this is the last tool in its group (for tree connectors)

        DEBUG: Parallel agent tracking
        """
        if self._interrupted:
            return

        import sys

        if self._parallel_group is not None:
            print(f"[DEBUG PARALLEL] add_nested_tool_call: parent={parent!r}", file=sys.stderr)
            print(
                f"[DEBUG PARALLEL] agents keys={list(self._parallel_group.agents.keys())}",
                file=sys.stderr,
            )
            agent = self._parallel_group.agents.get(parent)
            print(f"[DEBUG PARALLEL] agent found={agent is not None}", file=sys.stderr)
            if agent:
                print(
                    f"[DEBUG PARALLEL] agent.tool_call_id={agent.tool_call_id!r}", file=sys.stderr
                )
        if isinstance(display, Text):
            tool_text = display.copy()
        else:
            tool_text = Text(str(display), style=SUBTLE)

        # NEW: If single agent is active, track its tools and update display
        if self._single_agent is not None and self._single_agent.status == "running":
            # Extract tool name
            plain_text = tool_text.plain if hasattr(tool_text, "plain") else str(tool_text)
            if ":" in plain_text:
                tool_name = plain_text.split(":")[0].strip()
            elif "(" in plain_text:
                tool_name = plain_text.split("(")[0].strip()
            else:
                tool_name = plain_text.split()[0] if plain_text.split() else "unknown"

            # Count tool calls and store record for expansion
            self._single_agent.tool_count += 1
            self._single_agent.current_tool = plain_text
            self._single_agent.tool_records.append(
                SingleAgentToolRecord(
                    tool_name=tool_name,
                    display_text=plain_text,
                )
            )

            # Update header with rotating spinner
            self._update_header_spinner()

            # Update current tool line
            self._update_single_agent_tool_line()

            # Don't expand individual tools for single agent (collapsed mode like parallel)
            return

        # If active parallel group: update agent stats and status line in-place
        if self._parallel_group is not None:
            # parent is now tool_call_id for parallel agents
            agent = self._parallel_group.agents.get(parent)
            if agent is not None:
                # Track unique tools: extract tool name from display text
                plain_text = tool_text.plain if hasattr(tool_text, "plain") else str(tool_text)
                # Extract tool name (e.g., "Read (file.py)" -> "Read", or "list_files: src" -> "list_files")
                if ":" in plain_text:
                    tool_name = plain_text.split(":")[0].strip()
                elif "(" in plain_text:
                    tool_name = plain_text.split("(")[0].strip()
                else:
                    tool_name = plain_text.split()[0] if plain_text.split() else "unknown"

                # Count tool calls
                agent.tool_count += 1

                # Update display text
                agent.current_tool = plain_text

                # Update agent row to show unique tool count
                self._update_agent_row(agent)

                # Update status line with current tool
                self._update_status_line(agent)

                if not self._parallel_expanded:
                    return  # DON'T write individual tool line when collapsed

        # Expanded mode: write the tool call line
        self._spacing.before_nested_tool_call()

        # Build tree-style indentation
        formatted = Text()
        indent = self._build_tree_indent(depth, parent, is_last)
        formatted.append(indent)
        formatted.append(f"{self._nested_spinner_char} ", style=GREEN_GRADIENT[0])
        formatted.append_text(tool_text)
        formatted.append(" (0s)", style=GREY)

        # Nested tool lines have tree structure and are updated in-place, don't re-wrap
        self.log.write(formatted, scroll_end=True, animate=False, wrappable=False)

        # Generate tool_id if not provided
        if not tool_id:
            tool_id = f"{parent}_{len(self._nested_tools)}_{time.monotonic()}"

        # Store state in multi-tool tracking dict
        key = (parent, tool_id)
        self._nested_tools[key] = NestedToolState(
            line_number=len(self.log.lines) - 1,
            tool_text=tool_text.copy(),
            depth=depth,
            timer_start=time.monotonic(),
            color_index=0,
            parent=parent,
            tool_id=tool_id,
        )

        # Note: parallel group stats are already updated at the top of this method
        # when we found the agent by tool_call_id

        # Maintain legacy single-tool state for backwards compat
        self._nested_tool_line = len(self.log.lines) - 1
        self._nested_tool_text = tool_text.copy()
        self._nested_tool_depth = depth
        self._nested_color_index = 0
        self._nested_tool_timer_start = time.monotonic()

        self._start_nested_tool_timer()

    def _build_tree_indent(self, depth: int, parent: str, is_last: bool) -> str:
        """Build tree connector prefix for nested tool display.

        Args:
            depth: Nesting depth
            parent: Parent agent name
            is_last: Whether this is the last tool in its group

        Returns:
            String like "   ├─ " or "   └─ " or "   │  ├─ "
        """
        if depth == 1:
            # First level: simple tree connector
            connector = TREE_LAST if is_last else TREE_BRANCH
            return f"   {connector} "
        else:
            # Deeper nesting: add vertical lines for continuation
            return (
                "   "
                + f"{TREE_VERTICAL}  " * (depth - 1)
                + (f"{TREE_LAST} " if is_last else f"{TREE_BRANCH} ")
            )

    def complete_nested_tool_call(
        self,
        tool_name: str,
        depth: int,
        parent: str,
        success: bool,
        tool_id: str = "",
    ) -> None:
        """Complete a nested tool call, updating the display.

        Args:
            tool_name: Name of the tool
            depth: Nesting depth
            parent: Parent agent name
            success: Whether the tool succeeded
            tool_id: Unique tool call ID for tracking
        """
        if self._interrupted:
            return

        # Update single agent tool records with completion status
        if self._single_agent is not None and self._single_agent.tool_records:
            # Find the most recent record for this tool and update it
            for record in reversed(self._single_agent.tool_records):
                if record.tool_name == tool_name and record.success is True:
                    record.success = success
                    break

        # Try to find the tool in multi-tool tracking dict
        state: Optional[NestedToolState] = None

        if tool_id:
            key = (parent, tool_id)
            state = self._nested_tools.pop(key, None)

        # Fallback: find most recent tool for this parent
        if state is None:
            for key in list(self._nested_tools.keys()):
                if key[0] == parent:
                    state = self._nested_tools.pop(key)
                    break

        # Final fallback: use legacy single-tool state
        if state is None:
            if self._nested_tool_line is None or self._nested_tool_text is None:
                return
            state = NestedToolState(
                line_number=self._nested_tool_line,
                tool_text=self._nested_tool_text,
                depth=self._nested_tool_depth,
                timer_start=self._nested_tool_timer_start or time.monotonic(),
                parent=parent,
            )
            self._nested_tool_line = None
            self._nested_tool_text = None
            self._nested_tool_timer_start = None

        # Stop timers only if no more active tools
        if not self._nested_tools:
            if self._nested_tool_timer:
                self._nested_tool_timer.stop()
                self._nested_tool_timer = None
            if self._nested_tool_thread_timer:
                self._nested_tool_thread_timer.cancel()
                self._nested_tool_thread_timer = None

        # Build completed tool display
        formatted = Text()
        indent = self._build_tree_indent(state.depth, state.parent, is_last=False)
        formatted.append(indent)

        status_char = "✓" if success else "✗"
        status_color = SUCCESS if success else ERROR

        formatted.append(f"{status_char} ", style=status_color)
        formatted.append_text(state.tool_text)

        elapsed = round(time.monotonic() - state.timer_start)
        formatted.append(f" ({elapsed}s)", style=GREY)

        # In-place update
        from rich.console import Console

        console = Console(width=1000, force_terminal=True, no_color=False)
        segments = list(formatted.render(console))
        strip = Strip(segments)

        if state.line_number < len(self.log.lines):
            self.log.lines[state.line_number] = strip
            self.log.refresh_line(state.line_number)

    def _start_nested_tool_timer(self) -> None:
        """Start or continue the nested tool animation timer."""
        # Only start timer if not already running
        if self._nested_tool_timer is None:
            self._animate_nested_tool_spinner()

    def _animate_nested_tool_spinner(self) -> None:
        """Animate ALL active nested tool spinners AND agent row spinners."""
        if self._paused_for_resize:
            return  # Skip animation during resize

        if self._nested_tool_thread_timer:
            self._nested_tool_thread_timer.cancel()
            self._nested_tool_thread_timer = None

        # Check if there are any active tools, parallel agents, or single agent to animate
        has_active_tools = self._nested_tools or (
            self._nested_tool_line is not None or self._nested_tool_text is not None
        )
        has_active_agents = self._parallel_group is not None and any(
            a.status == "running" for a in self._parallel_group.agents.values()
        )
        has_single_agent = self._single_agent is not None and self._single_agent.status == "running"

        if not has_active_tools and not has_active_agents and not has_single_agent:
            self._nested_tool_timer = None
            return

        # Animate all tools in the multi-tool tracking dict
        for key, state in self._nested_tools.items():
            state.color_index = (state.color_index + 1) % len(GREEN_GRADIENT)
            self._render_nested_tool_line_for_state(state)

        # Also animate legacy single-tool state if present
        if self._nested_tool_line is not None and self._nested_tool_text is not None:
            self._nested_color_index = (self._nested_color_index + 1) % len(GREEN_GRADIENT)
            self._render_nested_tool_line()

        # Animate parallel agents: header spinner and agent row gradient bullets
        if self._parallel_group is not None:
            # Animate header with rotating spinner
            if any(a.status == "running" for a in self._parallel_group.agents.values()):
                self._header_spinner_index += 1  # Increment spinner frame for animation
                self._update_parallel_header()

            # Animate agent rows with gradient bullets
            for tool_call_id, agent in self._parallel_group.agents.items():
                if agent.status == "running":
                    # Update gradient color index
                    idx = self._agent_spinner_states.get(tool_call_id, 0)
                    idx = (idx + 1) % len(GREEN_GRADIENT)
                    self._agent_spinner_states[tool_call_id] = idx

                    # Update agent row with gradient animation
                    self._update_agent_row_gradient(agent, idx)

        # Animate single agent: header spinner
        if self._single_agent is not None and self._single_agent.status == "running":
            # Update header with rotating spinner
            self._update_header_spinner()

        # Schedule next animation frame
        interval = 0.15
        self._nested_tool_timer = self.log.set_timer(interval, self._animate_nested_tool_spinner)
        self._nested_tool_thread_timer = threading.Timer(interval, self._on_nested_tool_thread_tick)
        self._nested_tool_thread_timer.daemon = True
        self._nested_tool_thread_timer.start()

    def _on_nested_tool_thread_tick(self) -> None:
        """Thread timer callback for nested tool animation."""
        # Check if there are any active tools
        if not self._nested_tools and self._nested_tool_line is None:
            return
        try:
            if self.app:
                self.app.call_from_thread(self._animate_nested_tool_spinner)
        except Exception:
            pass

    def _render_nested_tool_line_for_state(self, state: NestedToolState) -> None:
        """Render a specific nested tool line from its state.

        Args:
            state: The NestedToolState to render
        """
        if state.line_number >= len(self.log.lines):
            return

        elapsed = round(time.monotonic() - state.timer_start)

        formatted = Text()
        indent = self._build_tree_indent(state.depth, state.parent, is_last=False)
        formatted.append(indent)
        color = GREEN_GRADIENT[state.color_index]
        formatted.append(f"{self._nested_spinner_char} ", style=color)
        formatted.append_text(state.tool_text.copy())
        formatted.append(f" ({elapsed}s)", style=GREY)

        from rich.console import Console

        console = Console(width=1000, force_terminal=True, no_color=False)
        segments = list(formatted.render(console))
        strip = Strip(segments)

        self.log.lines[state.line_number] = strip
        self.log.refresh_line(state.line_number)

    def _render_nested_tool_line(self) -> None:
        """Render the legacy single nested tool line."""
        if self._nested_tool_line is None or self._nested_tool_text is None:
            return

        if self._nested_tool_line >= len(self.log.lines):
            return

        elapsed = 0
        if self._nested_tool_timer_start:
            elapsed = round(time.monotonic() - self._nested_tool_timer_start)

        formatted = Text()
        indent = "  " * self._nested_tool_depth
        formatted.append(indent)
        color = GREEN_GRADIENT[self._nested_color_index]
        formatted.append(f"{self._nested_spinner_char} ", style=color)
        formatted.append_text(self._nested_tool_text.copy())
        formatted.append(f" ({elapsed}s)", style=GREY)

        from rich.console import Console

        console = Console(width=1000, force_terminal=True, no_color=False)
        segments = list(formatted.render(console))
        strip = Strip(segments)

        self.log.lines[self._nested_tool_line] = strip
        self.log.refresh_line(self._nested_tool_line)
        if self.app and hasattr(self.app, "refresh"):
            self.app.refresh()

    # --- Parallel Agent Group Management ---

    def on_parallel_agents_start(self, agent_infos: List[dict]) -> None:
        """Called when parallel agents start executing.

        Creates a parallel group and renders header + individual agent lines with status.

        Args:
            agent_infos: List of agent info dicts with keys:
                - agent_type: Type of agent (e.g., "Explore")
                - description: Short description of agent's task
                - tool_call_id: Unique ID for tracking this agent
        """
        self._spacing.before_parallel_agents()

        # Write header line - updated in-place with spinner, don't re-wrap
        header = Text()
        header.append("⠋ ", style=CYAN)  # Rotating spinner for header
        header.append(f"Running {len(agent_infos)} agents… ")
        self.log.write(header, scroll_end=True, animate=False, wrappable=False)
        header_line = len(self.log.lines) - 1

        # Create agents dict keyed by tool_call_id
        agents: Dict[str, AgentInfo] = {}
        for i, info in enumerate(agent_infos):
            is_last = i == len(agent_infos) - 1
            tool_call_id = info.get("tool_call_id", f"agent_{i}")
            description = info.get("description") or info.get("agent_type", "Agent")
            agent_type = info.get("agent_type", "Agent")

            # Agent row: "   ⏺ Description · 0 tools" (gradient flashing bullet)
            # Updated in-place with spinner animation, don't re-wrap
            agent_row = Text()
            agent_row.append("   ⏺ ", style=GREEN_BRIGHT)  # Gradient bullet for agent rows
            agent_row.append(description)
            agent_row.append(" · 0 tools", style=GREY)
            self.log.write(agent_row, scroll_end=True, animate=False, wrappable=False)
            agent_line = len(self.log.lines) - 1

            # Status row: "      ⎿  Initializing...." (no tree connector for agent row)
            # Updated in-place, don't re-wrap
            status_row = Text()
            status_row.append("      ⎿  ", style=GREY)
            status_row.append("Initializing....", style=SUBTLE)
            self.log.write(status_row, scroll_end=True, animate=False, wrappable=False)
            status_line_num = len(self.log.lines) - 1

            agents[tool_call_id] = AgentInfo(
                agent_type=agent_type,
                description=description,
                tool_call_id=tool_call_id,
                line_number=agent_line,
                status_line=status_line_num,
                is_last=is_last,
            )

        self._parallel_group = ParallelAgentGroup(
            agents=agents,
            header_line=header_line,
            expanded=self._parallel_expanded,
            start_time=time.monotonic(),
        )

        # Reset animation indices for parallel agents
        self._header_spinner_index = 0
        self._agent_spinner_states.clear()

        # Start animation timer for spinner and gradient effects
        self._start_nested_tool_timer()

    def _render_parallel_header(self) -> Text:
        """Render the parallel agents header line.

        Returns:
            Text object for the header line
        """
        if self._parallel_group is None:
            return Text("")

        group = self._parallel_group
        total_agents = len(group.agents)
        total_tools = sum(a.tool_count for a in group.agents.values())
        all_completed = all(a.status in ("completed", "failed") for a in group.agents.values())
        any_failed = any(a.status == "failed" for a in group.agents.values())

        # Count agents by type for description
        type_counts: Dict[str, int] = {}
        for agent in group.agents.values():
            type_counts[agent.agent_type] = type_counts.get(agent.agent_type, 0) + 1

        # Build agent type description (e.g., "2 Explore agents" or "1 Explore + 1 Bash agents")
        type_descriptions = []
        for agent_type, count in type_counts.items():
            type_descriptions.append(f"{count} {agent_type}")
        agent_desc = (
            " + ".join(type_descriptions)
            if len(type_descriptions) > 1
            else type_descriptions[0] if type_descriptions else "0"
        )
        agent_word = "agent" if total_agents == 1 else "agents"

        text = Text()

        if all_completed:
            if any_failed:
                text.append("⏺ ", style=ERROR)
            else:
                text.append("⏺ ", style=SUCCESS)
            elapsed = round(time.monotonic() - group.start_time)
            text.append(f"Completed {agent_desc} {agent_word} ")
            text.append(f"({total_tools} tools · {elapsed}s)", style=GREY)
        else:
            # Use rotating spinner character from _header_spinner_index
            spinner_char = self._spinner_chars[
                self._header_spinner_index % len(self._spinner_chars)
            ]
            text.append(f"{spinner_char} ", style=CYAN)
            text.append(f"Running {agent_desc} {agent_word}… ")

        return text

    def _update_parallel_header(self) -> None:
        """Update the parallel header line in-place."""
        if self._parallel_group is None:
            return

        header_text = self._render_parallel_header()

        from rich.console import Console

        console = Console(width=1000, force_terminal=True, no_color=False)
        segments = list(header_text.render(console))
        strip = Strip(segments)

        if self._parallel_group.header_line < len(self.log.lines):
            self.log.lines[self._parallel_group.header_line] = strip
            self.log.refresh_line(self._parallel_group.header_line)

    def _update_agent_row(self, agent: AgentInfo) -> None:
        """Update an agent's row line to show tool count (with gradient bullet if running).

        Args:
            agent: AgentInfo for the agent to update
        """
        if agent.line_number >= len(self.log.lines):
            return

        # Build agent row: "   ⏺ Description · N tools" (gradient bullet if running, checkmark if complete)
        unique_count = agent.tool_count
        use_spinner = agent.status == "running"

        if use_spinner:
            # Use gradient color for ⏺ bullet (animate through green gradient)
            idx = self._agent_spinner_states.get(agent.tool_call_id, 0)
            color_idx = idx % len(GREEN_GRADIENT)
            color = GREEN_GRADIENT[color_idx]
            row = Text()
            row.append("   ⏺ ", style=color)  # Gradient flashing bullet
            row.append(agent.description)
            row.append(f" · {unique_count} tool" + ("s" if unique_count != 1 else ""), style=GREY)
        else:
            # Use checkmark/X for completed agents
            status_char = "✓" if agent.status == "completed" else "✗"
            status_style = SUCCESS if agent.status == "completed" else ERROR
            row = Text()
            row.append(f"   {status_char} ", style=status_style)
            row.append(agent.description)
            row.append(f" · {unique_count} tool" + ("s" if unique_count != 1 else ""), style=GREY)

        strip = self._text_to_strip(row)
        self.log.lines[agent.line_number] = strip
        self.log.refresh_line(agent.line_number)

    def _update_status_line(self, agent: AgentInfo) -> None:
        """Update an agent's status line with current tool.

        Args:
            agent: AgentInfo for the agent to update
        """
        if agent.status_line >= len(self.log.lines):
            return

        # Build status row: "      ⎿  Current tool name" (no tree connector)
        status = Text()
        status.append("      ⎿  ", style=GREY)
        status.append(agent.current_tool, style=SUBTLE)

        strip = self._text_to_strip(status)
        self.log.lines[agent.status_line] = strip
        self.log.refresh_line(agent.status_line)

    def _update_agent_row_gradient(self, agent: AgentInfo, color_idx: int) -> None:
        """Update agent row with animated gradient bullet.

        Args:
            agent: AgentInfo for the agent to update
            color_idx: Current color index for gradient animation
        """
        if agent.line_number >= len(self.log.lines):
            return

        # Build agent row: "   ⏺ Description · N tools" with gradient color
        unique_count = agent.tool_count
        color = GREEN_GRADIENT[color_idx % len(GREEN_GRADIENT)]
        row = Text()
        row.append("   ⏺ ", style=color)  # Gradient flashing bullet
        row.append(agent.description)
        row.append(f" · {unique_count} tool" + ("s" if unique_count != 1 else ""), style=GREY)

        strip = self._text_to_strip(row)
        self.log.lines[agent.line_number] = strip
        self.log.refresh_line(agent.line_number)

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

    def on_parallel_agent_complete(self, tool_call_id: str, success: bool) -> None:
        """Called when a parallel agent completes.

        Args:
            tool_call_id: Unique tool call ID of the agent that completed
            success: Whether the agent succeeded
        """
        if self._interrupted:
            return

        if self._parallel_group is None:
            return

        agent = self._parallel_group.agents.get(tool_call_id)
        if agent is not None:
            # Update status
            agent.status = "completed" if success else "failed"

            # Update agent row with final status
            self._update_agent_row_completed(agent, success)

            # Update status line to show completion
            self._update_status_line_completed(agent, success)

            # Update header
            self._update_parallel_header()

    def _update_agent_row_completed(self, agent: AgentInfo, success: bool) -> None:
        """Update an agent's row line to show completion status.

        Args:
            agent: AgentInfo for the agent to update
            success: Whether the agent succeeded
        """
        if agent.line_number >= len(self.log.lines):
            return

        # Build completed agent row: "   ✓ Description · N tools" (green checkmark)
        status_char = "✓" if success else "✗"
        status_style = SUCCESS if success else ERROR
        unique_count = agent.tool_count

        row = Text()
        row.append(f"   {status_char} ", style=status_style)
        row.append(agent.description)
        row.append(f" · {unique_count} tool" + ("s" if unique_count != 1 else ""), style=GREY)

        strip = self._text_to_strip(row)
        self.log.lines[agent.line_number] = strip
        self.log.refresh_line(agent.line_number)

    def _update_status_line_completed(self, agent: AgentInfo, success: bool) -> None:
        """Update an agent's status line to show completion.

        Args:
            agent: AgentInfo for the agent to update
            success: Whether the agent succeeded
        """
        if agent.status_line >= len(self.log.lines):
            return

        # Build completed status row: "      ⎿  Done" or "      ⎿  Failed" (no tree connector)
        status_text = "Done" if success else "Failed"

        status = Text()
        status.append("      ⎿  ", style=GREY)
        status.append(status_text, style=SUBTLE if success else ERROR)

        strip = self._text_to_strip(status)
        self.log.lines[agent.status_line] = strip
        self.log.refresh_line(agent.status_line)

    def on_parallel_agents_done(self) -> None:
        """Called when all parallel agents have completed."""
        if self._parallel_group is None:
            return

        if self._interrupted:
            # interrupt_cleanup already handled display; just clear state
            self._parallel_group = None
            return

        # Mark all agents as completed (in case some weren't explicitly marked)
        for agent in self._parallel_group.agents.values():
            if agent.status == "running":
                agent.status = "completed"
                self._update_agent_row_completed(agent, success=True)
                self._update_status_line_completed(agent, success=True)

        self._parallel_group.completed = True
        self._update_parallel_header()

        # Add blank line for spacing before next content
        self._spacing.after_parallel_agents()

        # Clear parallel group
        self._parallel_group = None

    def _write_parallel_agent_summaries(self) -> None:
        """Write summary lines for each agent in the parallel group."""
        if self._parallel_group is None:
            return

        agents = list(self._parallel_group.agents.items())
        for i, (name, stats) in enumerate(agents):
            is_last = i == len(agents) - 1
            connector = TREE_LAST if is_last else TREE_BRANCH

            text = Text()
            text.append(f"   {connector} ", style=GREY)
            text.append(f"{name}", style=PRIMARY)
            text.append(f" · {stats.tool_count} tool uses", style=GREY)

            if stats.current_tool:
                text.append("\n")
                continuation = "      " if is_last else f"   {TREE_VERTICAL}  "
                text.append(f"{continuation}{TREE_CONTINUATION}  ", style=GREY)
                text.append(stats.current_tool, style=SUBTLE)

            # Parallel agent summaries have tree structure, don't re-wrap
            self.log.write(text, scroll_end=True, animate=False, wrappable=False)

    def toggle_parallel_expansion(self) -> bool:
        """Toggle the expand/collapse state of parallel agent display.

        Returns:
            New expansion state (True = expanded)
        """
        self._parallel_expanded = not self._parallel_expanded
        return self._parallel_expanded

    def has_expandable_single_agent(self) -> bool:
        """Check if there's a completed single agent that can be expanded."""
        return (
            self._completed_single_agent is not None
            and len(self._completed_single_agent.tool_records) > 0
        )

    def toggle_single_agent_expansion(self) -> bool:
        """Toggle expand/collapse of the last completed single agent's tool calls.

        Returns:
            New expansion state (True = expanded)
        """
        agent = self._completed_single_agent
        if agent is None or not agent.tool_records:
            return False

        self._single_agent_expanded = not self._single_agent_expanded

        if self._single_agent_expanded:
            # Expand: insert tool call lines after the tool_line
            insert_at = agent.tool_line + 1
            lines_to_insert = []
            for record in agent.tool_records:
                row = Text()
                row.append("    ", style="")
                status_char = "✓" if record.success else "✗"
                status_color = SUCCESS if record.success else ERROR
                row.append(f"{status_char} ", style=status_color)
                row.append(record.display_text, style=SUBTLE)
                lines_to_insert.append(self._text_to_strip(row))

            # Insert lines into the log
            for i, strip in enumerate(lines_to_insert):
                self.log.lines.insert(insert_at + i, strip)

            # Update tool_line summary hint
            self._update_single_agent_summary(agent, "(ctrl+o to collapse)")

            if hasattr(self.log, "_recalculate_virtual_size"):
                self.log._recalculate_virtual_size()
            self.log.refresh()
        else:
            # Collapse: remove the inserted tool call lines
            insert_at = agent.tool_line + 1
            count = len(agent.tool_records)
            del self.log.lines[insert_at:insert_at + count]

            # Restore tool_line summary hint
            hint = " (ctrl+o to expand)" if agent.status == "failed" else ""
            self._update_single_agent_summary(agent, hint)

            if hasattr(self.log, "_recalculate_virtual_size"):
                self.log._recalculate_virtual_size()
            self.log.refresh()

        return self._single_agent_expanded

    def _update_single_agent_summary(self, agent: SingleAgentInfo, hint: str) -> None:
        """Update the tool line summary with the given hint text."""
        tool_word = "tool use" if agent.tool_count == 1 else "tool uses"
        elapsed = int(time.monotonic() - agent.start_time)
        tool_row = Text()
        tool_row.append("  ⎿  ", style=GREY)
        status_text = (
            f"Failed ({agent.tool_count} {tool_word} · {elapsed}s)"
            if agent.status == "failed"
            else f"Done ({agent.tool_count} {tool_word} · {elapsed}s)"
        )
        style = ERROR if agent.status == "failed" else SUBTLE
        tool_row.append(status_text, style=style)
        if hint:
            tool_row.append(f" {hint}", style=f"{SUBTLE} italic")
        if agent.tool_line < len(self.log.lines):
            self.log.lines[agent.tool_line] = self._text_to_strip(tool_row)

    def on_single_agent_start(self, agent_type: str, description: str, tool_call_id: str) -> None:
        """Called when a single agent starts (non-parallel execution).

        This creates the same display structure as parallel agents but for a single agent.

        Args:
            agent_type: Type of agent (e.g., "Explore", "Code-Explorer")
            description: Task description
            tool_call_id: Unique ID for tracking
        """
        self._spacing.before_single_agent()

        # Header line: "⠋ Explore(description)" - Rotating spinner
        # Updated in-place with animation, don't re-wrap
        header = Text()
        header.append("⠋ ", style=CYAN)
        header.append(f"{agent_type}(", style=CYAN)
        header.append(description, style=PRIMARY)
        header.append(")", style=CYAN)
        self.log.write(header, scroll_end=True, animate=False, wrappable=False)
        header_line = len(self.log.lines) - 1

        # Tool line: "  ⎿  Initializing..."
        # Updated in-place, don't re-wrap
        tool_row = Text()
        tool_row.append("  ⎿  ", style=GREY)
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

        # Reset animation indices
        self._header_spinner_index = 0
        self._bullet_gradient_index = 0

        # Start animation timer for spinner and gradient effects
        self._start_nested_tool_timer()

    def _update_header_spinner(self) -> None:
        """Update header line with rotating spinner (⠋⠙⠹...)."""
        if self._single_agent is None:
            return

        agent = self._single_agent
        if agent.header_line >= len(self.log.lines):
            return

        # Get next spinner frame
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
        row.append("  ⎿  ", style=GREY)
        row.append(agent.current_tool, style=SUBTLE)

        strip = self._text_to_strip(row)
        self.log.lines[agent.tool_line] = strip
        self.log.refresh_line(agent.tool_line)

    def on_single_agent_complete(self, tool_call_id: str, success: bool = True) -> None:
        """Called when a single agent completes.

        Note: Keeps the same display style (⠋ header, ⏺ bullet). Just stops animation.

        Args:
            tool_call_id: Unique ID of the agent that completed
            success: Whether the agent succeeded
        """
        if self._interrupted:
            # interrupt_cleanup already handled display; just clear state
            self._single_agent = None
            return

        if self._single_agent is None:
            return

        # Verify tool_call_id matches (if provided)
        if tool_call_id and self._single_agent.tool_call_id != tool_call_id:
            return

        agent = self._single_agent
        agent.status = "completed" if success else "failed"

        # Update header from spinner to green bullet on completion
        header_row = Text()
        header_row.append("⏺ ", style=GREEN_BRIGHT if success else ERROR)
        header_row.append(f"{agent.agent_type}(", style=CYAN)
        header_row.append(agent.description, style=PRIMARY)
        header_row.append(")", style=CYAN)

        strip = self._text_to_strip(header_row)
        if agent.header_line < len(self.log.lines):
            self.log.lines[agent.header_line] = strip
            self.log.refresh_line(agent.header_line)

        # Update tool line to show "Done (N tool uses · Xs)"
        elapsed = int(time.monotonic() - agent.start_time)
        tool_word = "tool use" if agent.tool_count == 1 else "tool uses"
        tool_row = Text()
        tool_row.append("  ⎿  ", style=GREY)
        if success:
            tool_row.append(
                f"Done ({agent.tool_count} {tool_word} · {elapsed}s)", style=SUBTLE
            )
        else:
            tool_row.append(
                f"Failed ({agent.tool_count} {tool_word} · {elapsed}s)", style=ERROR
            )
            tool_row.append(" (ctrl+o to expand)", style=f"{SUBTLE} italic")

        strip = self._text_to_strip(tool_row)
        if agent.tool_line < len(self.log.lines):
            self.log.lines[agent.tool_line] = strip
            self.log.refresh_line(agent.tool_line)

        # Add blank line for spacing before next content
        self._spacing.after_single_agent()

        # Store for Ctrl+O expansion (keep tool records for user to inspect)
        self._completed_single_agent = agent
        self._single_agent_expanded = False
        self._single_agent = None

    def has_active_parallel_group(self) -> bool:
        """Check if there's an active parallel agent group.

        Returns:
            True if a parallel group is currently active
        """
        return self._parallel_group is not None and not self._parallel_group.completed

    def _rebuild_streaming_box_with_truncation(
        self,
        is_error: bool,
        content_lines: list[str],
    ) -> None:
        """Rebuild the streaming output with head+tail truncation."""
        if self._streaming_box_top_line is None:
            return

        # Remove all lines from top of output to current position
        self._truncate_from(self._streaming_box_top_line)

        # Apply truncation
        head_count = self._box_renderer.MAIN_AGENT_HEAD_LINES
        tail_count = self._box_renderer.MAIN_AGENT_TAIL_LINES
        head_lines, tail_lines, hidden_count = self._box_renderer.truncate_lines_head_tail(
            content_lines, head_count, tail_count
        )

        # Output lines with ⎿ prefix for first line, spaces for rest
        is_first = True
        for line in head_lines:
            self._write_bash_output_line(line, "", is_error, is_first)
            is_first = False

        if hidden_count > 0:
            hidden_text = Text(
                f"       ... {hidden_count} lines hidden ...", style=f"{SUBTLE} italic"
            )
            self.log.write(hidden_text, wrappable=False)

        for line in tail_lines:
            self._write_bash_output_line(line, "", is_error, is_first)
            is_first = False

    def _truncate_from(self, index: int) -> None:
        if index >= len(self.log.lines):
            return

        # Access protected lines from log if available
        protected_lines = getattr(self.log, "_protected_lines", set())

        # Check if any protected lines would be affected
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

        # Sync block registry so stale blocks don't re-render on resize
        if hasattr(self.log, "_block_registry"):
            self.log._block_registry.remove_blocks_from(index)

        # Clear cache if available
        if hasattr(self.log, "_line_cache"):
            self.log._line_cache.clear()

        # Update protected line indices
        if protected_lines:
            new_protected = set()
            for p in protected_lines:
                if p < index:
                    new_protected.add(p)
                elif p in protected_in_range:
                    deleted_before = len([i for i in range(index, p) if i not in protected_lines])
                    new_protected.add(p - deleted_before)

            # Update the set in place if possible, or verify how to update
            if hasattr(self.log, "_protected_lines"):
                self.log._protected_lines.clear()
                self.log._protected_lines.update(new_protected)

        # Trigger refresh logic similar to ConversationLog
        if hasattr(self.log, "virtual_size"):
            # RichLog usually recalculates virtual size on write.
            # Manual deletion might desync it.
            # We can't easily call internal _calculate_virtual_size
            # But self.log.refresh() usually handles repainting.
            pass
        self.log.refresh()

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
        elif prefix == "⏺":
            style = GREEN_BRIGHT
        else:
            style = GREEN_BRIGHT

        formatted.append(f"{prefix} ", style=style)
        formatted.append_text(self._tool_display)
        formatted.append(elapsed_str, style=GREY)

        from rich.console import Console

        # Use actual widget width instead of hardcoded 1000
        width = self._get_box_width()
        console = Console(width=width, force_terminal=True, no_color=False)
        segments = list(formatted.render(console))
        strip = Strip(segments)

        self.log.lines[self._tool_call_start] = strip
        self.log.refresh_line(self._tool_call_start)
        if self.app and hasattr(self.app, "refresh"):
            self.app.refresh()

    def _write_tool_call_line(self, prefix: str) -> None:
        # Initial write, just delegates to _replace mostly or simple write?
        # ConversationLog wrote "⏺" initially.
        # But standard log write appends. We need to append.
        # So we can fabricate it and write.

        # Logic from ConversationLog:
        # self._write_tool_call_line("⏺") -> calls _replace logic? No.
        # It constructs Text and calls self.write().

        formatted = Text()
        formatted.append(f"{prefix} ", style=GREEN_BRIGHT)
        if self._tool_display:
            formatted.append_text(self._tool_display)
        formatted.append(" (0s)", style=GREY)

        # Tool call lines are updated in-place with spinners, don't re-wrap
        self.log.write(formatted, wrappable=False)

    # --- Tool Result Parsing Helpers ---

    def _extract_edit_payload(self, text: str) -> Tuple[str, List[str]]:
        lines = text.splitlines()
        if not lines:
            return "", []

        # Simple heuristic to detect diff/edit output
        if lines[0].startswith("<<<<") or lines[0].startswith("Replaced lines"):
            # This is weak parsing, but matching ConversationLog's assumed logic
            # Actually ConversationLog had specific logic.
            # Let's inspect ConversationLog's _extract_edit_payload to be exact.
            # I should have read it more carefully. I'll copy it from previous context if possible.
            # Or just implement generic logic for now.
            pass

        # Re-implementing based on typical diff formats
        header = ""
        diff_lines = []

        if "Editing file" in lines[0] or "Applied edit" in lines[0] or "Updated " in lines[0]:
            header = lines[0]
            diff_lines = lines[1:]
            return header, diff_lines

        return "", []

    def _write_edit_result(self, header: str, diff_lines: list[str]) -> None:
        # Write header with ⎿ prefix to match other tool results - header can wrap
        self.log.write(Text(f"  ⎿  {header}", style=SUBTLE), wrappable=True)

        # Write diff lines with proper formatting - diff lines should NOT wrap
        # Lines come from _format_edit_file_result after ANSI stripping:
        #   Addition: "NNN + content"  (line number right-aligned in 3 chars)
        #   Deletion: "NNN - content"
        #   Context:  "NNN   content"
        # The + or - is at position 4 (0-indexed) after the 3-char line number
        for line in diff_lines:
            formatted = Text("     ")  # 5 spaces to align with ⎿ content
            # Check position 4 for + or - (after "NNN " prefix)
            is_addition = len(line) > 4 and line[4] == "+"
            is_deletion = len(line) > 4 and line[4] == "-"
            if is_addition:
                formatted.append(line, style=GREEN_BRIGHT)
            elif is_deletion:
                formatted.append(line, style=ERROR)
            else:
                formatted.append(line, style=SUBTLE)
            self.log.write(formatted, wrappable=False)

    def _write_generic_tool_result(self, text: str) -> None:
        lines = text.rstrip("\n").splitlines() or [text]
        for i, raw_line in enumerate(lines):
            # First line gets ⎿ prefix, subsequent lines get spaces for alignment
            prefix = "  ⎿  " if i == 0 else "     "
            line = Text(prefix, style=GREY)
            message = raw_line.rstrip("\n")
            is_error = False
            is_interrupted = False

            # Use constant if imported, else literal check
            if message.startswith(TOOL_ERROR_SENTINEL):
                is_error = True
                message = message[len(TOOL_ERROR_SENTINEL) :].lstrip()
            elif message.startswith("::interrupted::"):
                is_interrupted = True
                message = message[len("::interrupted::") :].lstrip()

            if is_interrupted:
                line.append(message, style=f"bold {ERROR}")
            else:
                # Use dim for normal, red for error
                line.append(message, style=ERROR if is_error else SUBTLE)
            # Tool result text - don't re-wrap to preserve output formatting
            self.log.write(line, wrappable=False)

    # --- Bash Box Output ---

    def add_bash_output_box(
        self,
        output: str,
        is_error: bool = False,
        command: str = "",
        working_dir: str = ".",
        depth: int = 0,
    ) -> None:
        """Render bash output with collapsible support for long output."""
        lines = output.rstrip("\n").splitlines()
        if not lines:
            lines = ["Completed"]

        # Apply truncation based on depth
        if depth == 0:
            head_count = self._box_renderer.MAIN_AGENT_HEAD_LINES
            tail_count = self._box_renderer.MAIN_AGENT_TAIL_LINES
        else:
            head_count = self._box_renderer.SUBAGENT_HEAD_LINES
            tail_count = self._box_renderer.SUBAGENT_TAIL_LINES

        max_lines = head_count + tail_count
        should_collapse = len(lines) > max_lines

        indent = "  " * depth

        if should_collapse:
            # Store full content and render collapsed summary
            start_line = len(self.log.lines)

            # Write collapsed summary line - summary text can wrap
            summary = summarize_output(lines, "bash")
            hint = get_expansion_hint()
            summary_line = Text(f"{indent}  \u23bf  ", style=GREY)
            summary_line.append(summary, style=SUBTLE)
            summary_line.append(f" {hint}", style=f"{SUBTLE} italic")
            self.log.write(summary_line, wrappable=False)

            end_line = len(self.log.lines) - 1

            # Track collapsible region
            collapsible = CollapsibleOutput(
                start_line=start_line,
                end_line=end_line,
                full_content=lines,
                summary=summary,
                is_expanded=False,
                output_type="bash",
                command=command,
                working_dir=working_dir,
                is_error=is_error,
                depth=depth,
            )
            self._collapsible_outputs[start_line] = collapsible
            self._most_recent_collapsible = start_line
        else:
            # Small output - render normally without collapse
            is_first = True
            for line in lines:
                self._write_bash_output_line(line, indent, is_error, is_first)
                is_first = False

        # Add blank line for spacing after output
        self._spacing.after_bash_output_box()

    def _write_bash_output_line(
        self, line: str, indent: str, is_error: bool, is_first: bool = False
    ) -> None:
        """Write a single bash output line with proper indentation."""
        normalized = self._box_renderer.normalize_line(line)
        # Use ⎿ prefix for first line, spaces for rest
        prefix = f"{indent}  \u23bf  " if is_first else f"{indent}     "
        output_line = Text(prefix, style=GREY)
        output_line.append(normalized, style=ERROR if is_error else GREY)
        # Bash output preserves formatting, don't re-wrap
        self.log.write(output_line, wrappable=False)

    def add_plan_content_box(self, plan_content: str) -> None:
        """Render plan content in a bordered Markdown panel."""
        from rich.markdown import Markdown
        from rich.panel import Panel

        md = Markdown(plan_content)
        panel = Panel(md, title="Plan", border_style="bright_cyan", padding=(1, 2))
        self.log.write(panel, wrappable=False)

    def start_streaming_bash_box(self, command: str = "", working_dir: str = ".") -> None:
        """Start streaming bash output with minimal style."""
        self._streaming_box_command = command
        self._streaming_box_working_dir = working_dir
        self._streaming_box_content_lines = []

        # Track start position for rebuild
        self._streaming_box_top_line = len(self.log.lines)
        self._streaming_box_header_line = len(self.log.lines)

    def append_to_streaming_box(self, line: str, is_stderr: bool = False) -> None:
        """Append a content line to the streaming output."""
        if self._streaming_box_header_line is None:
            return

        # Check if this is the first line (⎿ prefix)
        is_first = len(self._streaming_box_content_lines) == 0

        # Store for rebuild
        self._streaming_box_content_lines.append((line, is_stderr))

        # Write output line with ⎿ for first line, spaces for rest
        self._write_bash_output_line(line, "", is_stderr, is_first)

    def close_streaming_bash_box(self, is_error: bool, exit_code: int) -> None:
        """Close streaming bash output, collapsing if it exceeds threshold."""
        content_lines = [line for line, _ in self._streaming_box_content_lines]
        head_count = self._box_renderer.MAIN_AGENT_HEAD_LINES
        tail_count = self._box_renderer.MAIN_AGENT_TAIL_LINES
        max_lines = head_count + tail_count

        if len(content_lines) > max_lines and self._streaming_box_top_line is not None:
            # Rebuild with collapsed summary instead of truncation
            self._rebuild_streaming_box_as_collapsed(is_error, content_lines)

        # Reset state
        self._streaming_box_header_line = None
        self._streaming_box_top_line = None
        self._streaming_box_config = None
        self._streaming_box_command = ""
        self._streaming_box_working_dir = "."
        self._streaming_box_content_lines = []

    def _rebuild_streaming_box_as_collapsed(
        self,
        is_error: bool,
        content_lines: list[str],
    ) -> None:
        """Rebuild streaming output as a collapsed summary."""
        if self._streaming_box_top_line is None:
            return

        # Remove all lines from top of output to current position
        self._truncate_from(self._streaming_box_top_line)

        start_line = len(self.log.lines)

        # Write collapsed summary line - don't re-wrap
        summary = summarize_output(content_lines, "bash")
        hint = get_expansion_hint()
        summary_line = Text("  \u23bf  ", style=GREY)
        summary_line.append(summary, style=SUBTLE)
        summary_line.append(f" {hint}", style=f"{SUBTLE} italic")
        self.log.write(summary_line, wrappable=False)

        end_line = len(self.log.lines) - 1

        # Track collapsible region
        collapsible = CollapsibleOutput(
            start_line=start_line,
            end_line=end_line,
            full_content=content_lines,
            summary=summary,
            is_expanded=False,
            output_type="bash",
            command=self._streaming_box_command,
            working_dir=self._streaming_box_working_dir,
            is_error=is_error,
            depth=0,
        )
        self._collapsible_outputs[start_line] = collapsible
        self._most_recent_collapsible = start_line

    # --- Collapsible Output Toggle Methods ---

    def toggle_most_recent_collapsible(self) -> bool:
        """Toggle the most recent collapsible output region.

        Returns:
            True if a region was toggled, False if none found.
        """
        if self._most_recent_collapsible is None:
            return False

        collapsible = self._collapsible_outputs.get(self._most_recent_collapsible)
        if collapsible is None:
            return False

        return self._toggle_collapsible(collapsible)

    def toggle_output_at_line(self, line_index: int) -> bool:
        """Toggle collapsible output containing the given line.

        Args:
            line_index: Line index in the conversation log.

        Returns:
            True if a region was toggled, False if none found.
        """
        # Find collapsible region containing this line
        for start, collapsible in self._collapsible_outputs.items():
            if collapsible.contains_line(line_index):
                return self._toggle_collapsible(collapsible)
        return False

    def _toggle_collapsible(self, collapsible: CollapsibleOutput) -> bool:
        """Toggle a specific collapsible output region.

        Args:
            collapsible: CollapsibleOutput to toggle.

        Returns:
            True on success.
        """
        if collapsible.is_expanded:
            self._collapse_output(collapsible)
        else:
            self._expand_output(collapsible)
        return True

    def _expand_output(self, collapsible: CollapsibleOutput) -> None:
        """Expand a collapsed output region to show full content."""
        old_start = collapsible.start_line
        old_end = collapsible.end_line

        # Save lines after the collapsible region
        after_lines = list(self.log.lines[old_end + 1 :])

        # Remove from start_line to end
        del self.log.lines[old_start:]

        # Sync block registry
        if hasattr(self.log, "_block_registry"):
            self.log._block_registry.remove_blocks_from(old_start)

        if hasattr(self.log, "_line_cache"):
            self.log._line_cache.clear()

        # Write expanded content (appends at end, which is now old_start)
        indent = "  " * collapsible.depth
        new_start = len(self.log.lines)
        is_first = True
        for line in collapsible.full_content:
            self._write_bash_output_line(line, indent, collapsible.is_error, is_first)
            is_first = False
        new_end = len(self.log.lines) - 1
        new_count = new_end - new_start + 1
        old_count = old_end - old_start + 1

        # Re-append lines that were after the collapsible
        self.log.lines.extend(after_lines)

        # Update collapsible state
        collapsible.is_expanded = True
        if collapsible.start_line in self._collapsible_outputs:
            del self._collapsible_outputs[collapsible.start_line]
        collapsible.start_line = new_start
        collapsible.end_line = new_end
        self._collapsible_outputs[new_start] = collapsible
        self._most_recent_collapsible = new_start

        # Shift any other collapsible outputs that were after this one
        delta = new_count - old_count
        if delta != 0:
            shifted = {}
            for key, c in list(self._collapsible_outputs.items()):
                if c is not collapsible and key > old_start:
                    del self._collapsible_outputs[key]
                    c.start_line += delta
                    c.end_line += delta
                    shifted[c.start_line] = c
            self._collapsible_outputs.update(shifted)

        self.log.refresh()

    def _collapse_output(self, collapsible: CollapsibleOutput) -> None:
        """Collapse an expanded output region to show just summary."""
        old_start = collapsible.start_line
        old_end = collapsible.end_line

        # Save lines after the collapsible region
        after_lines = list(self.log.lines[old_end + 1 :])

        # Remove from start_line to end
        del self.log.lines[old_start:]

        # Sync block registry
        if hasattr(self.log, "_block_registry"):
            self.log._block_registry.remove_blocks_from(old_start)

        if hasattr(self.log, "_line_cache"):
            self.log._line_cache.clear()

        # Write collapsed summary (appends at end, which is now old_start)
        indent = "  " * collapsible.depth
        new_start = len(self.log.lines)
        hint = get_expansion_hint()
        summary_line = Text(f"{indent}  \u23bf  ", style=GREY)
        summary_line.append(collapsible.summary, style=SUBTLE)
        summary_line.append(f" {hint}", style=f"{SUBTLE} italic")
        self.log.write(summary_line, wrappable=False)
        new_end = len(self.log.lines) - 1
        new_count = new_end - new_start + 1
        old_count = old_end - old_start + 1

        # Re-append lines that were after the collapsible
        self.log.lines.extend(after_lines)

        # Update collapsible state
        collapsible.is_expanded = False
        if collapsible.start_line in self._collapsible_outputs:
            del self._collapsible_outputs[collapsible.start_line]
        collapsible.start_line = new_start
        collapsible.end_line = new_end
        self._collapsible_outputs[new_start] = collapsible
        self._most_recent_collapsible = new_start

        # Shift any other collapsible outputs that were after this one
        delta = new_count - old_count
        if delta != 0:
            shifted = {}
            for key, c in list(self._collapsible_outputs.items()):
                if c is not collapsible and key > old_start:
                    del self._collapsible_outputs[key]
                    c.start_line += delta
                    c.end_line += delta
                    shifted[c.start_line] = c
            self._collapsible_outputs.update(shifted)

        self.log.refresh()

    def has_collapsible_output(self) -> bool:
        """Check if there are any collapsible output regions.

        Returns:
            True if at least one collapsible region exists.
        """
        return len(self._collapsible_outputs) > 0

    def get_collapsible_at_line(self, line_index: int) -> Optional[CollapsibleOutput]:
        """Get collapsible output at a specific line.

        Args:
            line_index: Line index to check.

        Returns:
            CollapsibleOutput if found, None otherwise.
        """
        for collapsible in self._collapsible_outputs.values():
            if collapsible.contains_line(line_index):
                return collapsible
        return None

    def add_nested_bash_output_box(
        self,
        output: str,
        is_error: bool = False,
        command: str = "",
        working_dir: str = "",
        depth: int = 1,
    ) -> None:
        """Render nested bash output with minimal style."""
        # Use the same add_bash_output_box with depth parameter
        self.add_bash_output_box(output, is_error, command, working_dir, depth)

    # --- Nested Tool Result Display Methods ---

    def add_todo_sub_result(self, text: str, depth: int, is_last_parent: bool = True) -> None:
        """Add a single sub-result line for todo operations.

        Args:
            text: The sub-result text (e.g., "○ Create project structure")
            depth: Nesting depth for indentation
            is_last_parent: If True, no vertical continuation line (parent is last tool)
        """
        formatted = Text()
        indent = "  " * depth
        formatted.append(indent)
        # Use ⎿ prefix to match main agent style
        formatted.append("  ⎿  ", style=GREY)
        formatted.append(text, style=SUBTLE)
        # Todo sub-results have tree indentation structure, don't re-wrap
        self.log.write(formatted, scroll_end=True, animate=False, wrappable=False)

    def add_todo_sub_results(self, items: list, depth: int, is_last_parent: bool = True) -> None:
        """Add multiple sub-result lines for todo list operations.

        Args:
            items: List of (symbol, title) tuples
            depth: Nesting depth for indentation
            is_last_parent: If True, no vertical continuation line (parent is last tool)
        """
        indent = "  " * depth

        for i, (symbol, title) in enumerate(items):
            formatted = Text()
            formatted.append(indent)

            # First line gets ⎿ prefix, subsequent lines get spaces for alignment
            prefix = "  ⎿  " if i == 0 else "     "
            formatted.append(prefix, style=GREY)
            formatted.append(f"{symbol} {title}", style=SUBTLE)
            # Todo sub-results have tree indentation structure, don't re-wrap
            self.log.write(formatted, scroll_end=True, animate=False, wrappable=False)

    def add_nested_tool_sub_results(
        self, lines: List[str], depth: int, is_last_parent: bool = True
    ) -> None:
        """Add tool result lines with proper nesting indentation.

        This is the unified method for displaying subagent tool results,
        using the same formatting as the main agent via StyleFormatter.

        Args:
            lines: List of result lines from StyleFormatter._format_*_result() methods
            depth: Nesting depth for indentation
            is_last_parent: If True, no vertical continuation line (parent is last tool)
        """
        indent = "  " * depth

        # Flatten any multi-line strings into individual lines
        all_lines = []
        for line in lines:
            if "\n" in line:
                all_lines.extend(line.split("\n"))
            else:
                all_lines.append(line)

        # Filter trailing empty lines
        while all_lines and not all_lines[-1].strip():
            all_lines.pop()

        # Filter out empty lines and track non-empty ones for proper formatting
        non_empty_lines = [(i, line) for i, line in enumerate(all_lines) if line.strip()]

        # Check if any line contains error or interrupted markers
        has_error = any(TOOL_ERROR_SENTINEL in line for _, line in non_empty_lines)
        has_interrupted = any("::interrupted::" in line for _, line in non_empty_lines)

        for idx, (orig_i, line) in enumerate(non_empty_lines):
            formatted = Text()
            formatted.append(indent)

            # First line gets ⎿ prefix, subsequent lines get spaces for alignment
            prefix = "  ⎿  " if idx == 0 else "     "
            formatted.append(prefix, style=GREY)

            # Strip markers from content
            clean_line = (
                line.replace(TOOL_ERROR_SENTINEL, "").replace("::interrupted::", "").strip()
            )
            # Strip ANSI codes for nested display (they don't render well)
            clean_line = re.sub(r"\x1b\[[0-9;]*m", "", clean_line)

            # Apply consistent styling based on error state
            if has_interrupted:
                formatted.append(clean_line, style=f"bold {ERROR}")
            elif has_error:
                formatted.append(clean_line, style=ERROR)
            else:
                formatted.append(clean_line, style=SUBTLE)

            # Nested tool sub-results have tree indentation structure, don't re-wrap
            self.log.write(formatted, scroll_end=True, animate=False, wrappable=False)

    def add_nested_tree_result(
        self,
        tool_outputs: List[str],
        depth: int,
        is_last_parent: bool = True,
        has_error: bool = False,
        has_interrupted: bool = False,
    ) -> None:
        """Add tool result with tree-style indentation (legacy support).

        Args:
            tool_outputs: List of output lines
            depth: Nesting depth for indentation
            is_last_parent: If True, no vertical continuation line
            has_error: Whether result indicates an error
            has_interrupted: Whether the operation was interrupted
        """
        # Delegate to add_nested_tool_sub_results for consistent styling
        self.add_nested_tool_sub_results(tool_outputs, depth, is_last_parent)

    def add_edit_diff_result(self, diff_text: str, depth: int, is_last_parent: bool = True) -> None:
        """Add diff lines for edit_file result in subagent output.

        Args:
            diff_text: The unified diff text
            depth: Nesting depth for indentation
            is_last_parent: If True, no vertical continuation line (parent is last tool)
        """
        from opendev.ui_textual.formatters_internal.utils import DiffParser

        diff_entries = DiffParser.parse_unified_diff(diff_text)
        if not diff_entries:
            return

        indent = "  " * depth
        hunks = DiffParser.group_by_hunk(diff_entries)
        total_hunks = len(hunks)

        # Track overall line index for ⎿ prefix logic
        line_idx = 0

        for hunk_idx, (start_line, hunk_entries) in enumerate(hunks):
            # Add hunk header for multiple hunks
            if total_hunks > 1:
                # Add blank line between hunks (except before first)
                if hunk_idx > 0:
                    self.log.write(Text(""), scroll_end=True, animate=False, wrappable=False)

                # Write hunk header
                formatted = Text()
                formatted.append(indent)
                prefix = "  ⎿  " if line_idx == 0 else "     "
                formatted.append(prefix, style=GREY)
                formatted.append(
                    f"[Edit {hunk_idx + 1}/{total_hunks} at line {start_line}]", style=CYAN
                )
                self.log.write(formatted, scroll_end=True, animate=False, wrappable=False)
                line_idx += 1

            for entry_type, line_no, content in hunk_entries:
                formatted = Text()
                formatted.append(indent)

                # First line gets ⎿ prefix, subsequent lines get spaces for alignment
                prefix = "  ⎿  " if line_idx == 0 else "     "
                formatted.append(prefix, style=GREY)

                if entry_type == "add":
                    display_no = f"{line_no:>4} " if line_no is not None else "     "
                    formatted.append(display_no, style=SUBTLE)
                    formatted.append("+ ", style=SUCCESS)
                    formatted.append(content.replace("\t", "    "), style=SUCCESS)
                elif entry_type == "del":
                    display_no = f"{line_no:>4} " if line_no is not None else "     "
                    formatted.append(display_no, style=SUBTLE)
                    formatted.append("- ", style=ERROR)
                    formatted.append(content.replace("\t", "    "), style=ERROR)
                else:
                    display_no = f"{line_no:>4} " if line_no is not None else "     "
                    formatted.append(display_no, style=SUBTLE)
                    formatted.append("  ", style=SUBTLE)
                    formatted.append(content.replace("\t", "    "), style=SUBTLE)

                # Diff lines have line numbers and fixed formatting, don't re-wrap
                self.log.write(formatted, scroll_end=True, animate=False, wrappable=False)
                line_idx += 1
