"""Base classes for output formatters."""

from abc import ABC, abstractmethod
from typing import Any, Dict

from rich.console import Console
from rich.panel import Panel


# Status icons
STATUS_ICONS = {
    "success": "✓",
    "error": "✗",
    "warning": "⚠",
    "info": "ℹ",
}

# Action hints displayed in tool result panels
ACTION_HINTS = "r rerun • e export • y copy"


class BaseToolFormatter(ABC):
    """Base class for tool-specific formatters."""

    def __init__(self, console: Console):
        """Initialize formatter with console instance."""
        self.console = console

    @abstractmethod
    def format(
        self,
        tool_name: str,
        tool_args: Dict[str, Any],
        result: Dict[str, Any],
    ) -> Panel:
        """Format tool execution result as a rich panel.

        Args:
            tool_name: Name of the tool
            tool_args: Tool arguments
            result: Tool execution result

        Returns:
            Formatted panel
        """
        pass

    def _get_status_icon(self, result: Dict[str, Any]) -> str:
        """Get appropriate status icon based on result."""
        return STATUS_ICONS["success"] if result.get("success") else STATUS_ICONS["error"]

    def _get_border_style(self, result: Dict[str, Any]) -> str:
        """Get appropriate border style based on result."""
        return "green" if result.get("success") else "red"