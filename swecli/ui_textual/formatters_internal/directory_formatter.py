"""Directory listing formatter."""

import json
from typing import Any, Dict, Optional
from rich.panel import Panel
from rich.tree import Tree

from swecli.ui_textual.style_tokens import ERROR, SUBTLE, GREEN_BRIGHT, STATUS_ICONS

from .formatter_base import BaseFormatter


class DirectoryFormatter(BaseFormatter):
    """Handles formatting for directory operations."""

    def _create_tree_view(self, files: Any, directory: str) -> Optional[Panel]:
        """Create tree view panel for directory listing.

        Args:
            files: List of files/directories
            directory: Directory path

        Returns:
            Panel with tree view, or None if creation fails
        """
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
            title=STATUS_ICONS["success"],
            title_align="left",
            border_style=GREEN_BRIGHT,
        )

    def _create_fallback_display(
        self,
        output: str,
        directory: str,
        status_icon: str,
    ) -> Panel:
        """Create fallback text display for directory listing.

        Args:
            output: Raw output text
            directory: Directory path
            status_icon: Status icon to display

        Returns:
            Panel with text display
        """
        lines = []
        lines.append(f"{status_icon} [bold]{directory}[/bold]")
        lines.append("")
        lines.append(output[:500] if len(output) > 500 else output)

        return Panel(
            "\n".join(lines),
            title=status_icon,
            title_align="left",
            border_style=GREEN_BRIGHT,
        )

    def format_list_directory(
        self,
        icon: str,
        tool_args: Dict[str, Any],
        result: Dict[str, Any],
    ) -> Panel:
        """Format list_directory result with tree view."""
        directory = tool_args.get("path", ".")
        status_icon = STATUS_ICONS["success"] if result.get("success") else STATUS_ICONS["error"]

        if not result.get("success"):
            error = result.get("error", "Unknown error")
            content_text = f"{status_icon} [bold]{directory}[/bold]\n[{ERROR}]{error}[/{ERROR}]"
            return Panel(content_text, title=status_icon, title_align="left", border_style=ERROR)

        output = result.get("output", "")

        # Try to create tree view from structured JSON output
        try:
            files = json.loads(output)
            if isinstance(files, list):
                tree_panel = self._create_tree_view(files, directory)
                if tree_panel:
                    return tree_panel
        except:
            pass

        # Fallback to text display
        return self._create_fallback_display(output, directory, status_icon)