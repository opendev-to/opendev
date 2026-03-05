"""Bash command execution formatter."""

from typing import Dict, Any
from rich.panel import Panel

from .formatter_base import BaseFormatter, STATUS_ICONS


class BashFormatter(BaseFormatter):
    """Handles formatting for bash command execution results."""

    def format_bash_execute(
        self,
        icon: str,
        tool_args: Dict[str, Any],
        result: Dict[str, Any],
    ) -> Panel:
        """Format bash_execute result."""
        command = tool_args.get("command", "")

        status_icon = STATUS_ICONS["success"] if result.get("success") else STATUS_ICONS["error"]
        title = status_icon

        lines = []
        lines.append(f"{status_icon} [bold cyan]$ {command}[/bold cyan]")

        if result.get("success"):
            output = result.get("output", "")

            if output:
                lines.append("")
                # Truncate long outputs
                if len(output) > 500:
                    lines.append("[dim]Output (first 500 chars):[/dim]")
                    lines.append(output[:500] + "...")
                else:
                    lines.append("[dim]Output:[/dim]")
                    lines.append(output)
            else:
                lines.append("[dim](No output)[/dim]")
        else:
            error = result.get("error", "Unknown error")
            lines.append("")
            lines.append(f"[red]{error}[/red]")

        content_text = "\n".join(lines)
        border_style = "green" if result.get("success") else "red"

        return Panel(
            content_text,
            title=title,
            title_align="left",
            border_style=border_style,
        )