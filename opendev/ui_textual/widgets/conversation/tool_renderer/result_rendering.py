"""Mixin for nested results, todo results, and edit diffs."""

from __future__ import annotations

import re
from typing import TYPE_CHECKING, List

from rich.text import Text

from opendev.ui_textual.style_tokens import (
    CYAN,
    ERROR,
    GREEN_BRIGHT,
    GREY,
    PRIMARY,
    SUBTLE,
    SUCCESS,
)

if TYPE_CHECKING:
    pass


class ResultRenderingMixin:
    """Nested tool result display: todo items, tree results, edit diffs."""

    # Attributes expected from DefaultToolRenderer.__init__:
    #   log

    # --- Nested Tool Result Display Methods ---

    def add_todo_sub_result(self, text: str, depth: int, is_last_parent: bool = True) -> None:
        """Add a single sub-result line for todo operations.

        Args:
            text: The sub-result text (e.g., "○ Create project structure")
            depth: Nesting depth for indentation
            is_last_parent: If True, no vertical continuation line (parent is last tool)
        """
        formatted = Text()
        indent = "  " * depth
        formatted.append(indent)
        formatted.append("  \u23bf  ", style=GREY)
        formatted.append(text, style=SUBTLE)
        self.log.write(formatted, scroll_end=True, animate=False, wrappable=False)

    def add_todo_sub_results(self, items: list, depth: int, is_last_parent: bool = True) -> None:
        """Add multiple sub-result lines for todo list operations.

        Args:
            items: List of (symbol, title) tuples
            depth: Nesting depth for indentation
            is_last_parent: If True, no vertical continuation line (parent is last tool)
        """
        indent = "  " * depth

        for i, (symbol, title) in enumerate(items):
            formatted = Text()
            formatted.append(indent)

            prefix = "  \u23bf  " if i == 0 else "     "
            formatted.append(prefix, style=GREY)
            formatted.append(f"{symbol} {title}", style=SUBTLE)
            self.log.write(formatted, scroll_end=True, animate=False, wrappable=False)

    def add_nested_tool_sub_results(
        self, lines: List[str], depth: int, is_last_parent: bool = True
    ) -> None:
        """Add tool result lines with proper nesting indentation.

        This is the unified method for displaying subagent tool results,
        using the same formatting as the main agent via StyleFormatter.

        Args:
            lines: List of result lines from StyleFormatter._format_*_result() methods
            depth: Nesting depth for indentation
            is_last_parent: If True, no vertical continuation line (parent is last tool)
        """
        from opendev.ui_textual.constants import TOOL_ERROR_SENTINEL

        indent = "  " * depth

        # Flatten any multi-line strings into individual lines
        all_lines: List[str] = []
        for line in lines:
            if "\n" in line:
                all_lines.extend(line.split("\n"))
            else:
                all_lines.append(line)

        # Filter trailing empty lines
        while all_lines and not all_lines[-1].strip():
            all_lines.pop()

        non_empty_lines = [(i, line) for i, line in enumerate(all_lines) if line.strip()]

        has_error = any(TOOL_ERROR_SENTINEL in line for _, line in non_empty_lines)
        has_interrupted = any("::interrupted::" in line for _, line in non_empty_lines)

        for idx, (orig_i, line) in enumerate(non_empty_lines):
            formatted = Text()
            formatted.append(indent)

            prefix = "  \u23bf  " if idx == 0 else "     "
            formatted.append(prefix, style=GREY)

            clean_line = (
                line.replace(TOOL_ERROR_SENTINEL, "").replace("::interrupted::", "").strip()
            )
            clean_line = re.sub(r"\x1b\[[0-9;]*m", "", clean_line)

            if has_interrupted:
                formatted.append(clean_line, style=f"bold {ERROR}")
            elif has_error:
                formatted.append(clean_line, style=ERROR)
            else:
                formatted.append(clean_line, style=SUBTLE)

            self.log.write(formatted, scroll_end=True, animate=False, wrappable=False)

    def add_nested_tree_result(
        self,
        tool_outputs: List[str],
        depth: int,
        is_last_parent: bool = True,
        has_error: bool = False,
        has_interrupted: bool = False,
    ) -> None:
        """Add tool result with tree-style indentation (legacy support).

        Args:
            tool_outputs: List of output lines
            depth: Nesting depth for indentation
            is_last_parent: If True, no vertical continuation line
            has_error: Whether result indicates an error
            has_interrupted: Whether the operation was interrupted
        """
        self.add_nested_tool_sub_results(tool_outputs, depth, is_last_parent)

    def add_edit_diff_result(self, diff_text: str, depth: int, is_last_parent: bool = True) -> None:
        """Add diff lines for edit_file result in subagent output.

        Args:
            diff_text: The unified diff text
            depth: Nesting depth for indentation
            is_last_parent: If True, no vertical continuation line (parent is last tool)
        """
        from opendev.ui_textual.formatters_internal.utils import DiffParser

        diff_entries = DiffParser.parse_unified_diff(diff_text)
        if not diff_entries:
            return

        indent = "  " * depth
        hunks = DiffParser.group_by_hunk(diff_entries)
        total_hunks = len(hunks)

        line_idx = 0

        for hunk_idx, (start_line, hunk_entries) in enumerate(hunks):
            if total_hunks > 1:
                if hunk_idx > 0:
                    self.log.write(Text(""), scroll_end=True, animate=False, wrappable=False)

                formatted = Text()
                formatted.append(indent)
                prefix = "  \u23bf  " if line_idx == 0 else "     "
                formatted.append(prefix, style=GREY)
                formatted.append(
                    f"[Edit {hunk_idx + 1}/{total_hunks} at line {start_line}]", style=CYAN
                )
                self.log.write(formatted, scroll_end=True, animate=False, wrappable=False)
                line_idx += 1

            for entry_type, line_no, content in hunk_entries:
                formatted = Text()
                formatted.append(indent)

                prefix = "  \u23bf  " if line_idx == 0 else "     "
                formatted.append(prefix, style=GREY)

                if entry_type == "add":
                    display_no = f"{line_no:>4} " if line_no is not None else "     "
                    formatted.append(display_no, style=SUBTLE)
                    formatted.append("+ ", style=SUCCESS)
                    formatted.append(content.replace("\t", "    "), style=SUCCESS)
                elif entry_type == "del":
                    display_no = f"{line_no:>4} " if line_no is not None else "     "
                    formatted.append(display_no, style=SUBTLE)
                    formatted.append("- ", style=ERROR)
                    formatted.append(content.replace("\t", "    "), style=ERROR)
                else:
                    display_no = f"{line_no:>4} " if line_no is not None else "     "
                    formatted.append(display_no, style=SUBTLE)
                    formatted.append("  ", style=SUBTLE)
                    formatted.append(content.replace("\t", "    "), style=SUBTLE)

                self.log.write(formatted, scroll_end=True, animate=False, wrappable=False)
                line_idx += 1

    def add_nested_extra_edit_block(
        self,
        file_path: str,
        start_line: int,
        additions: int,
        removals: int,
        hunk_entries: list,
        depth: int,
    ) -> None:
        """Render an additional edit hunk block in nested/subagent context."""
        indent = "  " * depth

        # Header: ⏺ Edit(file.py) at line N
        header = Text()
        header.append(indent)
        header.append("⏺ ", style=GREEN_BRIGHT)
        header.append("Edit", style=PRIMARY)
        header.append(f"({file_path}) at line {start_line}", style=GREY)
        self.log.write(header, scroll_end=True, animate=False, wrappable=False)

        # Summary line
        def _plural(count: int, singular: str) -> str:
            return f"{count} {singular}" if count == 1 else f"{count} {singular}s"

        parts = []
        if additions:
            parts.append(_plural(additions, "addition"))
        if removals:
            parts.append(_plural(removals, "removal"))
        summary_text = ", ".join(parts) if parts else "no changes"

        summary = Text()
        summary.append(indent)
        summary.append("  ⎿  ", style=GREY)
        summary.append(f"Updated {file_path} ({summary_text})", style=SUBTLE)
        self.log.write(summary, scroll_end=True, animate=False, wrappable=False)

        # Diff lines
        for entry_type, line_no, content in hunk_entries:
            formatted = Text()
            formatted.append(indent)
            formatted.append("     ")

            display_no = f"{line_no:>4} " if line_no is not None else "     "
            sanitized = content.replace("\t", "    ")

            if entry_type == "add":
                formatted.append(display_no, style=SUBTLE)
                formatted.append("+ ", style=SUCCESS)
                formatted.append(sanitized, style=SUCCESS)
            elif entry_type == "del":
                formatted.append(display_no, style=SUBTLE)
                formatted.append("- ", style=ERROR)
                formatted.append(sanitized, style=ERROR)
            else:
                formatted.append(display_no, style=SUBTLE)
                formatted.append("  ", style=SUBTLE)
                formatted.append(sanitized, style=SUBTLE)

            self.log.write(formatted, scroll_end=True, animate=False, wrappable=False)
