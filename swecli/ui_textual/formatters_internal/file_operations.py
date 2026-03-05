"""Formatters for file operation tools."""

from pathlib import Path
from typing import Any, Dict

from rich.panel import Panel
from rich.syntax import Syntax
from rich.text import Text

from swecli.ui_textual.style_tokens import ERROR, SUBTLE, SUCCESS, GREEN_BRIGHT, PANEL_BORDER

from .base import BaseToolFormatter
from .utils import LanguageDetector, SizeFormatter


class WriteFileFormatter(BaseToolFormatter):
    """Formatter for write_file tool results."""

    def format(self, tool_name: str, tool_args: Dict[str, Any], result: Dict[str, Any]) -> Panel:
        """Format write_file result."""
        file_path = tool_args.get("file_path", "unknown")
        content = tool_args.get("content", "")

        # Build title
        status_icon = self._get_status_icon(result)

        # Build content
        lines = []

        # File info
        lines.append(f"{status_icon} [bold]{file_path}[/bold]")

        if result.get("success"):
            # File statistics
            size = len(content)
            num_lines = content.count("\n") + 1 if content else 0

            size_display = SizeFormatter.format_size(size)
            lines.append(f"[{SUBTLE}]Created • {size_display} • {num_lines} lines[/{SUBTLE}]")

            # Show preview (first 5 lines)
            if content:
                lines.append("")
                lines.append(f"[{SUBTLE}]Preview:[/{SUBTLE}]")
                preview_lines = content.split("\n")[:5]

                # Detect language from file extension
                ext = Path(file_path).suffix
                language = LanguageDetector.detect(ext)

                if language and len(content) < 1000:
                    # Syntax highlight
                    syntax = Syntax(
                        "\n".join(preview_lines),
                        language,
                        theme="monokai",
                        line_numbers=True,
                        start_line=1,
                    )
                    return Panel(
                        syntax,
                        title=status_icon,
                        title_align="left",
                        border_style=GREEN_BRIGHT,
                    )
                else:
                    # Plain text preview
                    for i, line in enumerate(preview_lines, 1):
                        lines.append(f"  [{SUBTLE}]{i:2d} │[/{SUBTLE}] {line[:60]}")

                    if num_lines > 5:
                        lines.append(f"[{SUBTLE}]  ... ({num_lines - 5} more lines)[/{SUBTLE}]")
        else:
            # Error message
            error = result.get("error", "Unknown error")
            lines.append(f"[{ERROR}]{error}[/{ERROR}]")

        content_text = "\n".join(lines)
        border_style = self._get_border_style(result)

        return Panel(
            content_text,
            title=status_icon,
            title_align="left",
            border_style=border_style,
        )


class ReadFileFormatter(BaseToolFormatter):
    """Formatter for read_file tool results."""

    def format(self, tool_name: str, tool_args: Dict[str, Any], result: Dict[str, Any]) -> Panel:
        """Format read_file result."""
        file_path = tool_args.get("file_path", "unknown")

        status_icon = self._get_status_icon(result)

        lines = []
        lines.append(f"{status_icon} [bold]{file_path}[/bold]")

        if result.get("success"):
            output = result.get("output", "")

            # File statistics
            size = len(output)
            num_lines = output.count("\n") + 1 if output else 0

            size_display = SizeFormatter.format_size(size)
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
        border_style = self._get_border_style(result)

        return Panel(
            content_text,
            title=status_icon,
            title_align="left",
            border_style=border_style,
        )


class EditFileFormatter(BaseToolFormatter):
    """Formatter for edit_file tool results."""

    def format(self, tool_name: str, tool_args: Dict[str, Any], result: Dict[str, Any]) -> Panel:
        """Format edit_file result with diff."""
        from .utils import DiffParser

        file_path = tool_args.get("file_path", "unknown")

        success = result.get("success", False)
        status_icon = self._get_status_icon(result)
        border_style = self._get_border_style(result)

        if not success:
            error = result.get("error", "Unknown error")
            content = f"{status_icon} [bold]{file_path}[/bold]\n[{ERROR}]{error}[/{ERROR}]"
            return Panel(content, title=status_icon, title_align="left", border_style=border_style)

        lines_added = result.get("lines_added", 0) or 0
        lines_removed = result.get("lines_removed", 0) or 0
        diff_text = result.get("diff") or ""

        def _plural(count: int, singular: str, plural: str = None) -> str:
            word = singular if count == 1 else (plural or f"{singular}s")
            return f"{count} {word}"

        header = f"✏️ Update({file_path})"
        summary = (
            f"  ⎿  Updated {file_path} with {_plural(lines_added, 'addition')}"
            f" and {_plural(lines_removed, 'removal')}"
        )

        body = Text()
        body.append(header + "\n", style="bold")
        body.append(summary + "\n", style=SUBTLE)

        diff_entries = []
        if diff_text:
            diff_entries = DiffParser.parse_unified_diff(diff_text)

        if diff_entries:
            body.append("\n")
            for entry_type, line_no, content in diff_entries:
                if entry_type == "hunk":
                    body.append(f"  {content}\n", style=SUBTLE)
                    continue

                display_no = f"{line_no:>6}" if line_no is not None else "      "
                sanitized = content.replace("\t", "    ")

                if entry_type == "add":
                    prefix = "+"
                    style = GREEN_BRIGHT
                elif entry_type == "del":
                    prefix = "-"
                    style = ERROR
                else:
                    prefix = " "
                    style = SUBTLE

                line_text = Text("  ")
                line_text.append(display_no, style=SUBTLE)
                line_text.append(" ")
                line_text.append(prefix, style=style)
                line_text.append(" ")
                line_text.append(sanitized.rstrip(), style=style)
                line_text.append("\n")
                body.append(line_text)
        else:
            body.append(f"\n[{SUBTLE}](Diff preview unavailable)[/{SUBTLE}]\n")

        return Panel(body, title=status_icon, title_align="left", border_style=border_style)