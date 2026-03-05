"""Diff preview system for showing file changes."""

import difflib
from typing import Optional

from rich.console import Console
from rich.syntax import Syntax
from rich.panel import Panel


class Diff:
    """Represents a diff between two file versions."""

    def __init__(
        self,
        file_path: str,
        original: str,
        modified: str,
        original_lines: Optional[list[str]] = None,
        modified_lines: Optional[list[str]] = None,
    ):
        """Initialize diff.

        Args:
            file_path: Path to file
            original: Original content
            modified: Modified content
            original_lines: Original lines (computed if not provided)
            modified_lines: Modified lines (computed if not provided)
        """
        self.file_path = file_path
        self.original = original
        self.modified = modified
        self.original_lines = original_lines or original.splitlines()
        self.modified_lines = modified_lines or modified.splitlines()

    def generate_unified_diff(self, context_lines: int = 3) -> str:
        """Generate unified diff format.

        Args:
            context_lines: Number of context lines to show

        Returns:
            Unified diff string
        """
        diff = difflib.unified_diff(
            self.original_lines,
            self.modified_lines,
            fromfile=f"a/{self.file_path}",
            tofile=f"b/{self.file_path}",
            lineterm="",
            n=context_lines,
        )
        return "\n".join(diff)

    def get_stats(self) -> dict[str, int]:
        """Get diff statistics.

        Returns:
            Dict with lines_added, lines_removed, lines_changed
        """
        diff = list(
            difflib.unified_diff(
                self.original_lines, self.modified_lines, lineterm=""
            )
        )

        added = sum(1 for line in diff if line.startswith("+") and not line.startswith("+++"))
        removed = sum(1 for line in diff if line.startswith("-") and not line.startswith("---"))

        return {
            "lines_added": added,
            "lines_removed": removed,
            "lines_changed": added + removed,
        }


class DiffPreview:
    """Tool for generating and displaying file diffs."""

    def __init__(self, console: Optional[Console] = None):
        """Initialize diff preview.

        Args:
            console: Rich console for output (creates new if not provided)
        """
        self.console = console or Console()

    def generate_diff(
        self, file_path: str, original: str, modified: str
    ) -> Diff:
        """Generate a diff object.

        Args:
            file_path: Path to file
            original: Original content
            modified: Modified content

        Returns:
            Diff object
        """
        return Diff(file_path, original, modified)

    def render_diff(
        self,
        diff: Diff,
        format: str = "unified",
        show_stats: bool = True,
    ) -> str:
        """Render diff as string.

        Args:
            diff: Diff object to render
            format: Output format ("unified" only for now)
            show_stats: Whether to show statistics

        Returns:
            Rendered diff string
        """
        output = []

        # File header
        output.append(f"File: {diff.file_path}")
        output.append("─" * 50)

        # Generate diff
        if format == "unified":
            unified = diff.generate_unified_diff()
            # Skip the file headers from unified diff
            lines = unified.split("\n")[2:]  # Skip --- and +++ lines
            output.extend(lines)
        else:
            raise ValueError(f"Unsupported format: {format}")

        # Statistics
        if show_stats:
            output.append("─" * 50)
            stats = diff.get_stats()
            output.append(
                f"Changes: +{stats['lines_added']} -{stats['lines_removed']}"
            )

        return "\n".join(output)

    def display_diff(
        self,
        diff: Diff,
        format: str = "unified",
        show_stats: bool = True,
        syntax_highlight: bool = True,
    ) -> None:
        """Display diff in the console.

        Args:
            diff: Diff object to display
            format: Output format
            show_stats: Whether to show statistics
            syntax_highlight: Whether to syntax highlight the diff
        """
        content = self.render_diff(diff, format, show_stats)

        if syntax_highlight:
            # Use diff syntax highlighting
            syntax = Syntax(content, "diff", theme="monokai", line_numbers=False)
            self.console.print(Panel(syntax, title=f"Changes: {diff.file_path}", border_style="cyan"))
        else:
            self.console.print(Panel(content, title=f"Changes: {diff.file_path}", border_style="cyan"))

    def preview_edit(
        self,
        file_path: str,
        original: str,
        modified: str,
        syntax_highlight: bool = True,
    ) -> None:
        """Preview an edit operation.

        Args:
            file_path: Path to file
            original: Original content
            modified: Modified content
            syntax_highlight: Whether to syntax highlight
        """
        diff = self.generate_diff(file_path, original, modified)
        self.display_diff(diff, syntax_highlight=syntax_highlight)
