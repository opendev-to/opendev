"""Base output formatter with shared functionality."""

import json
from typing import Any, Optional

from rich.console import Console
from rich.panel import Panel


from rich.panel import Panel

from swecli.ui_textual.style_tokens import STATUS_ICONS, TOOL_ICONS

# Action hints displayed in tool result panels
ACTION_HINTS = "r rerun • e export • y copy"


class BaseFormatter:
    """Base class for tool output formatters."""

    def __init__(self, console: Console):
        """Initialize formatter.

        Args:
            console: Rich console for output
        """
        self.console = console

    def _pluralize(self, count: int, singular: str, plural: Optional[str] = None) -> str:
        """Pluralize a word based on count.

        Args:
            count: Number to base pluralization on
            singular: Singular form of word
            plural: Optional plural form (defaults to singular + 's')

        Returns:
            Formatted string with count and word
        """
        word = singular if count == 1 else (plural or f"{singular}s")
        return f"{count} {word}"

    def _detect_language(self, ext: str) -> Optional[str]:
        """Detect programming language from file extension.

        Args:
            ext: File extension (e.g., ".py")

        Returns:
            Language name for syntax highlighting
        """
        language_map = {
            ".py": "python",
            ".js": "javascript",
            ".ts": "typescript",
            ".jsx": "jsx",
            ".tsx": "tsx",
            ".json": "json",
            ".yaml": "yaml",
            ".yml": "yaml",
            ".md": "markdown",
            ".sh": "bash",
            ".bash": "bash",
            ".zsh": "zsh",
            ".fish": "fish",
            ".html": "html",
            ".css": "css",
            ".scss": "scss",
            ".go": "go",
            ".rs": "rust",
            ".java": "java",
            ".c": "c",
            ".cpp": "cpp",
            ".h": "c",
            ".hpp": "cpp",
            ".rb": "ruby",
            ".php": "php",
            ".sql": "sql",
            ".xml": "xml",
            ".toml": "toml",
            ".ini": "ini",
            ".conf": "ini",
        }

        return language_map.get(ext.lower())

    def _format_size(self, size: int) -> str:
        """Format file size in human-readable format.

        Args:
            size: Size in bytes

        Returns:
            Formatted size string
        """
        if size < 1024:
            return f"{size} B"
        elif size < 1024 * 1024:
            return f"{size / 1024:.1f} KB"
        elif size < 1024 * 1024 * 1024:
            return f"{size / (1024 * 1024):.1f} MB"
        else:
            return f"{size / (1024 * 1024 * 1024):.1f} GB"

    def _summarize_value(self, value: Any) -> str:
        """Provide a concise representation of a value for display."""
        if isinstance(value, str):
            sanitized = value.replace("\n", "\\n")
            if len(sanitized) > 80:
                sanitized = sanitized[:77] + "…"
            return sanitized

        try:
            serialized = json.dumps(value, default=str)
        except TypeError:
            serialized = str(value)

        if len(serialized) > 80:
            serialized = serialized[:77] + "…"
        return serialized

    def _create_basic_panel(
        self,
        content: str,
        title: str,
        border_style: str = "blue",
        success: bool = True
    ) -> Panel:
        """Create a basic panel with content.

        Args:
            content: Panel content
            title: Panel title
            border_style: Border style
            success: Whether operation was successful

        Returns:
            Formatted panel
        """
        return Panel(
            content,
            title=title,
            title_align="left",
            border_style=border_style,
        )