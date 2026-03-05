"""Formatters for system operation tools."""

import json
from typing import Any, Dict

from rich.panel import Panel
from rich.tree import Tree

from swecli.ui_textual.style_tokens import ERROR, SUBTLE, GREEN_BRIGHT, GREEN_PROMPT

from .base import BaseToolFormatter


class BashExecuteFormatter(BaseToolFormatter):
    """Formatter for bash_execute tool results."""

    def format(self, tool_name: str, tool_args: Dict[str, Any], result: Dict[str, Any]) -> Panel:
        """Format bash_execute result."""
        command = tool_args.get("command", "")

        status_icon = self._get_status_icon(result)

        lines = []
        lines.append(f"{status_icon} [bold {GREEN_PROMPT}]$ {command}[/]")

        if result.get("success"):
            output = result.get("output", "")

            if output:
                lines.append("")
                # Truncate long outputs
                if len(output) > 500:
                    lines.append(f"[{SUBTLE}]Output (first 500 chars):[/{SUBTLE}]")
                    lines.append(output[:500] + "...")
                else:
                    lines.append(f"[{SUBTLE}]Output:[/{SUBTLE}]")
                    lines.append(output)
            else:
                lines.append(f"[{SUBTLE}](No output)[/{SUBTLE}]")
        else:
            error = result.get("error", "Unknown error")
            lines.append("")
            lines.append(f"[{ERROR}]{error}[/{ERROR}]")

        content_text = "\n".join(lines)
        border_style = self._get_border_style(result)

        return Panel(
            content_text,
            title=status_icon,
            title_align="left",
            border_style=border_style,
        )


class ListDirectoryFormatter(BaseToolFormatter):
    """Formatter for list_directory tool results."""

    def format(self, tool_name: str, tool_args: Dict[str, Any], result: Dict[str, Any]) -> Panel:
        """Format list_directory result with tree view."""
        directory = tool_args.get("path", ".")

        status_icon = self._get_status_icon(result)

        if result.get("success"):
            output = result.get("output", "")

            # Try to parse as JSON (if structured output)
            try:
                files = json.loads(output)
                if isinstance(files, list):
                    # Create tree view
                    tree = Tree(f"[bold]{directory}[/bold]")

                    for item in files[:20]:  # Limit to 20 items
                        if isinstance(item, dict):
                            name = item.get("name", "")
                            is_dir = item.get("is_dir", False)
                            prefix = "/" if is_dir else ""
                            tree.add(f"{prefix}{name}")
                        else:
                            tree.add(f"{item}")

                    if len(files) > 20:
                        tree.add(f"[{SUBTLE}]... ({len(files) - 20} more items)[/{SUBTLE}]")

                    return Panel(
                        tree,
                        title=status_icon,
                        title_align="left",
                        border_style=GREEN_BRIGHT,
                    )
            except:
                pass

            # Fallback to text display
            lines = []
            lines.append(f"{status_icon} [bold]{directory}[/bold]")
            lines.append("")
            lines.append(output[:500] if len(output) > 500 else output)

            content_text = "\n".join(lines)
            return Panel(
                content_text,
                title=status_icon,
                title_align="left",
                border_style=GREEN_BRIGHT,
            )
        else:
            error = result.get("error", "Unknown error")
            content_text = f"{status_icon} [bold]{directory}[/bold]\n[{ERROR}]{error}[/{ERROR}]"

            return Panel(
                content_text,
                title=status_icon,
                title_align="left",
                border_style=ERROR,
            )


class GenericToolFormatter(BaseToolFormatter):
    """Generic formatter for tools that don't have specific formatters."""

    def format(self, tool_name: str, tool_args: Dict[str, Any], result: Dict[str, Any]) -> Panel:
        """Format generic tool result."""
        status_icon = self._get_status_icon(result)

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
            lines.append(f"[{ERROR}]Error: {error}[/{ERROR}]")

        content_text = "\n".join(lines)
        border_style = self._get_border_style(result)

        return Panel(
            content_text,
            title=title,
            title_align="left",
            border_style=border_style,
        )