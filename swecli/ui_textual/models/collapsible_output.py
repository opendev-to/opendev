"""Collapsible output model for tracking expandable output sections."""

from __future__ import annotations

from dataclasses import dataclass, field
from typing import List


@dataclass
class CollapsibleOutput:
    """Tracks a collapsible output region in the conversation log.

    Stores full content and metadata for output that can be expanded/collapsed.
    """

    start_line: int
    """Starting line index in the conversation log."""

    end_line: int
    """Ending line index (inclusive) in the conversation log."""

    full_content: List[str]
    """Full output content lines (stored for expansion)."""

    summary: str
    """Summary text shown when collapsed (e.g., '142 lines')."""

    is_expanded: bool = False
    """Whether the output is currently expanded."""

    output_type: str = "bash"
    """Type of output: 'bash', 'tool_result', 'file_content'."""

    command: str = ""
    """Original command (for bash output)."""

    working_dir: str = "."
    """Working directory (for bash output)."""

    is_error: bool = False
    """Whether the output represents an error."""

    depth: int = 0
    """Nesting depth for subagent output."""

    # Maximum lines to store for expansion (memory limit)
    MAX_STORED_LINES: int = field(default=10000, repr=False)

    def __post_init__(self) -> None:
        """Apply memory limits to stored content."""
        if len(self.full_content) > self.MAX_STORED_LINES:
            # Keep first half and last half within limit
            half = self.MAX_STORED_LINES // 2
            self.full_content = self.full_content[:half] + self.full_content[-half:]

    def toggle(self) -> bool:
        """Toggle expanded state.

        Returns:
            New expanded state after toggle.
        """
        self.is_expanded = not self.is_expanded
        return self.is_expanded

    def collapse(self) -> None:
        """Collapse the output section."""
        self.is_expanded = False

    def expand(self) -> None:
        """Expand the output section."""
        self.is_expanded = True

    @property
    def line_count(self) -> int:
        """Number of lines in the full content."""
        return len(self.full_content)

    def contains_line(self, line_index: int) -> bool:
        """Check if a line index falls within this collapsible region.

        Args:
            line_index: Line index in the conversation log.

        Returns:
            True if the line is within this region's bounds.
        """
        return self.start_line <= line_index <= self.end_line

    def get_display_lines(self, head_count: int = 5, tail_count: int = 5) -> List[str]:
        """Get lines for truncated display.

        Args:
            head_count: Number of lines to show from start.
            tail_count: Number of lines to show from end.

        Returns:
            List of lines for display (with truncation message if needed).
        """
        if self.is_expanded or len(self.full_content) <= head_count + tail_count:
            return self.full_content

        head = self.full_content[:head_count]
        tail = self.full_content[-tail_count:]
        hidden = len(self.full_content) - head_count - tail_count

        return head + [f"... {hidden} lines hidden ..."] + tail
