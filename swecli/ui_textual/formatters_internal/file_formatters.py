"""File operation formatters for write, read, and edit operations."""

from pathlib import Path
from typing import Dict, Any, List, Optional, Tuple
from rich.panel import Panel
from rich.syntax import Syntax
from rich.text import Text

from .formatter_base import BaseFormatter, STATUS_ICONS
from swecli.ui_textual.style_tokens import ERROR, SUBTLE, SUCCESS, GREEN_BRIGHT, PANEL_BORDER, PRIMARY


class FileFormatter(BaseFormatter):
    """Handles formatting for file-related operations."""

    def _create_file_preview(
        self,
        content: str,
        file_path: str,
        num_lines: int,
    ) -> Optional[Panel]:
        """Create syntax-highlighted preview panel if applicable.

        Args:
            content: File content
            file_path: Path to file
            num_lines: Total number of lines in file

        Returns:
            Panel with syntax highlighting, or None if plain text should be used
        """
        preview_lines = content.split("\n")[:5]
        ext = Path(file_path).suffix
        language = self._detect_language(ext)

        if language and len(content) < 1000:
            syntax = Syntax(
                "\n".join(preview_lines),
                language,
                theme="monokai",
                line_numbers=True,
                start_line=1,
            )
            return Panel(
                syntax,
                title=STATUS_ICONS["success"],
                title_align="left",
                border_style=GREEN_BRIGHT,
            )
        return None

    def _add_plain_preview_lines(
        self,
        lines: List[str],
        content: str,
        num_lines: int,
    ) -> None:
        """Add plain text preview lines to output.

        Args:
            lines: Output lines list to append to
            content: File content
            num_lines: Total number of lines
        """
        preview_lines = content.split("\n")[:5]
        for i, line in enumerate(preview_lines, 1):
            lines.append(f"  [{SUBTLE}]{i:2d} │[/{SUBTLE}] {line[:60]}")

        if num_lines > 5:
            lines.append(f"[{SUBTLE}]  ... ({num_lines - 5} more lines)[/{SUBTLE}]")

    def format_write_file(
        self,
        icon: str,
        tool_args: Dict[str, Any],
        result: Dict[str, Any],
    ) -> Panel:
        """Format write_file result."""
        file_path = tool_args.get("file_path", "unknown")
        content = tool_args.get("content", "")
        status_icon = STATUS_ICONS["success"] if result.get("success") else STATUS_ICONS["error"]

        lines = []
        lines.append(f"{status_icon} [bold]{file_path}[/bold]")

        if result.get("success"):
            # File statistics
            size = len(content)
            num_lines = content.count("\n") + 1 if content else 0
            size_display = self._format_size(size)
            lines.append(f"[{SUBTLE}]Created • {size_display} • {num_lines} lines[/{SUBTLE}]")

            # Show preview
            if content:
                lines.append("")
                lines.append(f"[{SUBTLE}]Preview:[/{SUBTLE}]")

                # Try syntax-highlighted preview first
                syntax_panel = self._create_file_preview(content, file_path, num_lines)
                if syntax_panel:
                    return syntax_panel

                # Fallback to plain text preview
                self._add_plain_preview_lines(lines, content, num_lines)
        else:
            error = result.get("error", "Unknown error")
            lines.append(f"[{ERROR}]{error}[/{ERROR}]")

        border_style = GREEN_BRIGHT if result.get("success") else ERROR
        return Panel(
            "\n".join(lines),
            title=status_icon,
            title_align="left",
            border_style=border_style,
        )

    def _format_diff_entry(self, entry_type: str, line_no: Optional[int], content: str) -> Text:
        """Format a single diff entry line.

        Args:
            entry_type: Type of entry ('add', 'del', 'ctx', 'hunk')
            line_no: Line number or None
            content: Line content

        Returns:
            Formatted Text object
        """
        if entry_type == "hunk":
            return Text(f"  {content}\n", style=SUBTLE)

        display_no = f"{line_no:>6}" if line_no is not None else "      "
        sanitized = content.replace("\t", "    ")

        if entry_type == "add":
            prefix, style = "+", GREEN_BRIGHT
        elif entry_type == "del":
            prefix, style = "-", ERROR
        else:
            prefix, style = " ", SUBTLE

        line_text = Text("  ")
        line_text.append(display_no, style=SUBTLE)
        line_text.append(" ")
        line_text.append(prefix, style=style)
        line_text.append(" ")
        line_text.append(sanitized.rstrip(), style=style)
        line_text.append("\n")
        return line_text

    def _parse_unified_diff(self, diff_text: str) -> List[Tuple[str, Optional[int], str]]:
        """Parse unified diff text into structured entries.

        Returns:
            List of tuples: (entry_type, line_number, content)
        """
        import re

        entries: List[Tuple[str, Optional[int], str]] = []
        old_line: Optional[int] = None
        new_line: Optional[int] = None

        hunk_pattern = re.compile(r"@@ -(\d+)(?:,\d+)? \+(\d+)(?:,\d+)? @@")

        for raw_line in diff_text.splitlines():
            if raw_line.startswith("---") or raw_line.startswith("+++"):
                continue

            if raw_line.startswith("@@"):
                match = hunk_pattern.match(raw_line)
                if match:
                    old_line = int(match.group(1))
                    new_line = int(match.group(2))
                entries.append(("hunk", None, raw_line))
                continue

            if raw_line.startswith("+"):
                content = raw_line[1:]
                entries.append(("add", new_line, content))
                if new_line is not None:
                    new_line += 1
                continue

            if raw_line.startswith("-"):
                content = raw_line[1:]
                entries.append(("del", old_line, content))
                if old_line is not None:
                    old_line += 1
                continue

            # Context line
            content = raw_line[1:] if raw_line.startswith(" ") else raw_line
            entries.append(("ctx", old_line, content))
            if old_line is not None:
                old_line += 1
            if new_line is not None:
                new_line += 1

        return entries

    def format_edit_file(
        self,
        icon: str,
        tool_args: Dict[str, Any],
        result: Dict[str, Any],
    ) -> Panel:
        """Format edit_file result with diff."""
        file_path = tool_args.get("file_path", "unknown")
        success = result.get("success", False)
        status_icon = STATUS_ICONS["success"] if success else STATUS_ICONS["error"]
        border_style = GREEN_BRIGHT if success else ERROR

        if not success:
            error = result.get("error", "Unknown error")
            content = f"{status_icon} [bold]{file_path}[/bold]\n[{ERROR}]{error}[/{ERROR}]"
            return Panel(content, title=status_icon, title_align="left", border_style=border_style)

        lines_added = result.get("lines_added", 0) or 0
        lines_removed = result.get("lines_removed", 0) or 0
        diff_text = result.get("diff") or ""

        header = f"✏️ Update({file_path})"
        summary = (
            f"  ⎿  Updated {file_path} with {self._pluralize(lines_added, 'addition')}"
            f" and {self._pluralize(lines_removed, 'removal')}"
        )

        body = Text()
        body.append(header + "\n", style="bold")
        body.append(summary + "\n", style=SUBTLE)

        diff_entries: List[Tuple[str, Optional[int], str]] = []
        if diff_text:
            diff_entries = self._parse_unified_diff(diff_text)

        if diff_entries:
            body.append("\n")
            for entry_type, line_no, content in diff_entries:
                line_text = self._format_diff_entry(entry_type, line_no, content)
                body.append(line_text)
        else:
            body.append(f"\n[{SUBTLE}](Diff preview unavailable)[/{SUBTLE}]\n")

        return Panel(body, title=status_icon, title_align="left", border_style=border_style)

    def format_read_file(
        self,
        icon: str,
        tool_args: Dict[str, Any],
        result: Dict[str, Any],
    ) -> Panel:
        """Format read_file result."""
        file_path = tool_args.get("file_path", "unknown")

        status_icon = STATUS_ICONS["success"] if result.get("success") else STATUS_ICONS["error"]
        title = status_icon

        lines = []
        lines.append(f"{status_icon} [bold]{file_path}[/bold]")

        if result.get("success"):
            output = result.get("output", "")

            # File statistics
            size = len(output)
            num_lines = output.count("\n") + 1 if output else 0

            size_display = self._format_size(size)
            lines.append(f"[{SUBTLE}]Read • {size_display} • {num_lines} lines[/{SUBTLE}]")

            # Show truncated content
            if len(output) > 500:
                lines.append("")
                lines.append(f"[{SUBTLE}](Content too long, showing first 500 chars)[/{SUBTLE}]")
                preview = output[:500] + "..."
                lines.append(f"[{SUBTLE}]{preview}[/{SUBTLE}]")
            else:
                lines.append("")
                lines.append(f"[{SUBTLE}]{output}[/{SUBTLE}]")
        else:
            error = result.get("error", "Unknown error")
            lines.append(f"[{ERROR}]{error}[/{ERROR}]")

        content_text = "\n".join(lines)
        border_style = GREEN_BRIGHT if result.get("success") else ERROR

        return Panel(
            content_text,
            title=title,
            title_align="left",
            border_style=border_style,
        )