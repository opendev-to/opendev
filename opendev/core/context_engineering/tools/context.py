"""Shared context objects for tool execution."""

from __future__ import annotations

from dataclasses import dataclass
from typing import Any, Optional


@dataclass
class ToolExecutionContext:
    """Holds runtime managers supplied during tool execution."""

    mode_manager: Optional[Any] = None
    approval_manager: Optional[Any] = None
    undo_manager: Optional[Any] = None
    task_monitor: Optional[Any] = None
    session_manager: Optional[Any] = None
    ui_callback: Optional[Any] = None
    is_subagent: bool = False  # True when executing in subagent context
    file_time_tracker: Optional[Any] = None  # FileTimeTracker for stale-read detection
    formatter_manager: Optional[Any] = None  # FormatterManager for auto-format on save
