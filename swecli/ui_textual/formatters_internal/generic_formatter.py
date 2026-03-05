"""Generic formatter for tool results."""

from typing import Dict, Any
from rich.panel import Panel

from .formatter_base import BaseFormatter, STATUS_ICONS


class GenericFormatter(BaseFormatter):
    """Handles formatting for generic tool results."""

    def format_generic(
        self,
        icon: str,
        tool_name: str,
        tool_args: Dict[str, Any],
        result: Dict[str, Any],
    ) -> Panel:
        """Format generic tool result."""
        status_icon = STATUS_ICONS["success"] if result.get("success") else STATUS_ICONS["error"]

        # Simple title with just status icon (tool name already shown outside box)
        title = status_icon

        lines = []

        if result.get("success"):
            output = result.get("output", "")
            if output:
                if len(output) > 300:
                    lines.append(f"{output[:300]}... ({len(output)} chars total)")
                else:
                    lines.append(output)
            else:
                lines.append("(Completed successfully)")
        else:
            error = result.get("error", "Unknown error")
            lines.append(f"[red]Error: {error}[/red]")

        content_text = "\n".join(lines)
        border_style = "green" if result.get("success") else "red"

        return Panel(
            content_text,
            title=title,
            title_align="left",
            border_style=border_style,
        )