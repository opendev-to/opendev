"""Data classes for tool display formatting."""

from __future__ import annotations

from dataclasses import dataclass, field
from typing import Any, Optional


@dataclass
class BashOutputData:
    """Formatted bash output data for display.

    Attributes:
        output: The formatted output text to display.
        is_error: Whether the command failed (exit code != 0).
        command: The original command that was executed.
        working_dir: The working directory where the command ran.
        is_truncated: Whether the output was truncated.
        hidden_count: Number of lines hidden in the middle (for head/tail truncation).
    """

    output: str
    is_error: bool
    command: str
    working_dir: str
    is_truncated: bool = False
    hidden_count: int = 0


@dataclass
class ToolResultData:
    """Structured tool result for display.

    This dataclass provides a unified representation of tool results
    that can be consumed by both live and replay mode adapters.

    Attributes:
        success: Whether the tool execution succeeded.
        lines: Result lines to display (already formatted).
        special_type: Type of special handling needed ("bash", "ask_user", "subagent", "").
        is_interrupted: Whether the operation was interrupted by user.
        is_rejected: Whether the operation was rejected by user.
        bash_data: Additional data for bash command results.
        nested_calls: List of nested tool calls (for subagent results).
        diff_text: Raw diff text for edit_file results (for colored display).
        continuation_lines: Additional lines after the main result (e.g., diff content).
    """

    success: bool
    lines: list[str] = field(default_factory=list)
    special_type: str = ""
    is_interrupted: bool = False
    is_rejected: bool = False
    bash_data: Optional[BashOutputData] = None
    nested_calls: list[Any] = field(default_factory=list)
    diff_text: str = ""
    continuation_lines: list[str] = field(default_factory=list)
