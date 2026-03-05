"""Markdown-to-rich rendering helpers used by the conversation log."""

from __future__ import annotations

import re
from typing import List, Optional, Tuple

from rich.console import RenderableType
from rich.table import Table
from rich.text import Text

from swecli.ui_textual.style_tokens import SUBTLE, ACCENT, TEXT_MUTED


def render_markdown_text_segment(
    content: str, *, leading: bool = False
) -> Tuple[List[RenderableType], bool]:
    """Convert markdown text into Rich renderables.

    Args:
        content: The text segment to render (code fences already stripped).
        leading: If True, the first non-empty block will be prefixed with ``⏺``.

    Returns:
        A tuple of (renderables, wrote_any) where ``wrote_any`` indicates whether
        any non-empty content was emitted (used to manage leading bullets).
    """

    lines = content.splitlines()
    total_lines = len(lines)
    index = 0
    renderables: List[RenderableType] = []
    wrote_any = False
    leading_consumed = not leading
    indent = "  "  # Consistent indentation for all response lines

    def emit(renderable: RenderableType, allow_leading: bool = True) -> None:
        nonlocal wrote_any, leading_consumed
        if isinstance(renderable, str):
            renderable = Text(renderable)
        if (
            leading
            and not leading_consumed
            and allow_leading
            and getattr(renderable, "plain", str(renderable)).strip()
        ):
            bullet = Text("⏺ ")
            bullet.append_text(
                renderable if isinstance(renderable, Text) else Text(str(renderable))
            )
            renderables.append(bullet)
            leading_consumed = True
        else:
            # Add indentation to non-leading lines
            if isinstance(renderable, Text):
                indented = Text(indent)
                indented.append_text(renderable)
                renderables.append(indented)
            else:
                renderables.append(renderable)
            if leading and allow_leading and not leading_consumed:
                text_value = getattr(renderable, "plain", str(renderable))
                if text_value.strip():
                    leading_consumed = True
        text_plain = getattr(renderable, "plain", str(renderable))
        if text_plain.strip():
            wrote_any = True

    def blank_line() -> None:
        if wrote_any:
            emit(Text(""), allow_leading=False)

    def is_heading(line: str) -> bool:
        return bool(re.match(r"^(#{1,6})\s+", line.strip()))

    def is_bullet(line: str) -> bool:
        return bool(re.match(r"^\s*[-*+]\s+", line))

    def is_ordered(line: str) -> bool:
        return bool(re.match(r"^\s*\d+\.\s+", line))

    while index < total_lines:
        raw_line = lines[index]
        stripped = raw_line.strip()

        if not stripped:
            blank_line()
            index += 1
            continue

        if re.fullmatch(r"^[\-\*_]{3,}$", stripped):
            hr = Text("─" * 40, style=SUBTLE)
            emit(hr, allow_leading=False)
            index += 1
            continue

        heading_match = re.match(r"^(#{1,6})\s+(.*)", stripped)
        if heading_match:
            # Add blank line before headings for visual separation
            blank_line()
            level = len(heading_match.group(1))
            title = heading_match.group(2).strip()
            style = "bold" if level == 1 else "bold"
            # Add 2 extra spaces (4 total) before headings to distinguish from bullet content
            heading_text = Text("  " + title, style=style)
            emit(heading_text)
            index += 1
            continue

        if stripped.startswith(">"):
            quote_lines: List[str] = []
            while index < total_lines and lines[index].strip().startswith(">"):
                quote_lines.append(lines[index].lstrip("> ").rstrip())
                index += 1
            quote_text = " ".join(quote_lines).strip()
            if quote_text:
                rendered = _render_inline_markdown(quote_text)
                quote_line = Text("❝ ", style=SUBTLE)
                rendered.stylize("dim italic")
                quote_line.append_text(rendered)
                emit(quote_line, allow_leading=False)
            continue

        bullet_match = re.match(r"^(\s*)[-*+]\s+(.*)", raw_line)
        if bullet_match:
            indent = bullet_match.group(1) or ""
            bullet_text = bullet_match.group(2).strip()
            rendered = _render_inline_markdown(bullet_text)
            indent_level = max(0, len(indent) // 2)
            bullet_line = Text()
            # Check if this bullet will get the leading marker
            allow_leading_for_bullet = leading and not leading_consumed and indent_level == 0
            # No leading spaces when getting ⏺, otherwise standard spacing
            if allow_leading_for_bullet:
                symbol = "- "
            else:
                symbol = "  - " if indent_level == 0 else "    - "
            bullet_line.append("  " * indent_level + symbol)
            bullet_line.append_text(rendered)
            emit(bullet_line, allow_leading=allow_leading_for_bullet)
            index += 1
            continue

        ordered_match = re.match(r"^(\s*)(\d+)\.\s+(.*)", raw_line)
        if ordered_match:
            indent = ordered_match.group(1) or ""
            number = ordered_match.group(2)
            item_text = ordered_match.group(3).strip()
            rendered = _render_inline_markdown(item_text)
            indent_level = max(0, len(indent) // 2)
            ordered_line = Text()
            # Check if this item will get the leading marker
            allow_leading_for_ordered = leading and not leading_consumed and indent_level == 0
            if indent_level == 0:
                ordered_line.append(f"{number}. ")
            else:
                ordered_line.append("  " * indent_level + "– ")
            ordered_line.append_text(rendered)
            emit(ordered_line, allow_leading=allow_leading_for_ordered)
            index += 1
            continue

        # Check for table
        if _is_table_row(stripped):
            parsed_rows, new_index = _parse_table(lines, index)
            if parsed_rows is not None:
                table = _render_table(parsed_rows)
                emit(table, allow_leading=False)
                index = new_index
                continue

        paragraph_lines = [stripped]
        index += 1
        while index < total_lines:
            probe = lines[index]
            probe_stripped = probe.strip()
            if not probe_stripped:
                break
            if (
                is_heading(probe)
                or is_bullet(probe)
                or is_ordered(probe)
                or probe_stripped.startswith(">")
                or _is_table_row(probe)
            ):
                break
            paragraph_lines.append(probe_stripped)
            index += 1

        paragraph = " ".join(paragraph_lines).strip()
        if paragraph:
            rendered = _render_inline_markdown(paragraph)
            emit(rendered)

        while index < total_lines and not lines[index].strip():
            blank_line()
            index += 1

    return renderables, wrote_any


def _render_inline_markdown(text: str) -> Text:
    """Render inline markdown markers within a single line.

    Args:
        text: The text to render.
    """

    result = Text()
    link_pattern = re.compile(r"\[([^\]]+)\]\(([^)]+)\)")
    inline_pattern = re.compile(r"(\*\*[^*]+\*\*|__[^_]+__|\*[^*]+\*|_[^_]+_|`[^`]+`)")

    def append_with_style(fragment: str) -> None:
        cursor = 0
        for token_match in inline_pattern.finditer(fragment):
            if token_match.start() > cursor:
                result.append(fragment[cursor : token_match.start()])
            token = token_match.group(0)
            inner = token.strip("*`_")
            if token.startswith(("**", "__")):
                result.append(inner, style="bold")
            elif token.startswith(("*", "_")):
                result.append(inner, style="italic")
            elif token.startswith("`"):
                result.append(inner, style=TEXT_MUTED)
            else:
                result.append(inner)
            cursor = token_match.end()
        if cursor < len(fragment):
            result.append(fragment[cursor:])

    cursor = 0
    for match in link_pattern.finditer(text):
        if match.start() > cursor:
            append_with_style(text[cursor : match.start()])
        label, url = match.group(1), match.group(2)
        result.append(label, style=f"bold {ACCENT}")
        if url:
            result.append(f" ({url})", style=SUBTLE)
        cursor = match.end()

    if cursor < len(text):
        append_with_style(text[cursor:])

    if not result:
        append_with_style(text)

    return result


def _is_table_row(line: str) -> bool:
    """Check if line is a table row (| cell | cell |)."""
    stripped = line.strip()
    return stripped.startswith("|") and stripped.endswith("|")


def _is_table_separator(line: str) -> bool:
    """Check if line is a table separator (|---|---|)."""
    stripped = line.strip()
    if not stripped.startswith("|") or not stripped.endswith("|"):
        return False
    # Remove pipes and check if all cells are dashes/colons (alignment markers)
    cells = stripped[1:-1].split("|")
    return all(re.match(r"^:?-+:?$", cell.strip()) for cell in cells if cell.strip())


def _parse_row(line: str) -> List[str]:
    """Parse a table row into cells."""
    stripped = line.strip()[1:-1]  # Remove leading/trailing pipes
    return [cell.strip() for cell in stripped.split("|")]


def _parse_table(lines: List[str], start_index: int) -> Tuple[Optional[List[List[str]]], int]:
    """Parse markdown table starting at given index.

    Returns:
        (rows, end_index) where rows is list of rows (each row is list of cells),
        or (None, start_index) if not a valid table.
    """
    rows: List[List[str]] = []
    index = start_index
    total_lines = len(lines)

    # Must start with a row
    if not _is_table_row(lines[index]):
        return None, start_index

    # Parse header row
    header = _parse_row(lines[index])
    rows.append(header)
    index += 1

    # Must have separator row
    if index >= total_lines or not _is_table_separator(lines[index]):
        return None, start_index  # Not a valid table
    index += 1

    # Parse data rows
    while index < total_lines and _is_table_row(lines[index]):
        rows.append(_parse_row(lines[index]))
        index += 1

    return rows, index


def _render_table(rows: List[List[str]]) -> Table:
    """Convert parsed table rows to Rich Table."""
    table = Table(
        show_header=True,
        header_style="bold",
        box=None,  # Minimal borders for cleaner look
        padding=(0, 1),
        expand=False,
    )

    # Add columns from header row
    if rows:
        for header_cell in rows[0]:
            # Render inline markdown in header cells
            table.add_column(_render_inline_markdown(header_cell))

    # Add data rows
    for row in rows[1:]:
        # Pad row if it has fewer cells than header
        padded = row + [""] * (len(rows[0]) - len(row))
        # Render inline markdown in each cell
        rendered_cells = [_render_inline_markdown(cell) for cell in padded[: len(rows[0])]]
        table.add_row(*rendered_cells)

    return table
