"""Tests for markdown rendering helpers used by Textual and CLI output."""

from rich.table import Table
from rich.text import Text

from swecli.ui_textual.renderers.markdown import (
    render_markdown_text_segment,
    _is_table_row,
    _is_table_separator,
    _parse_row,
    _parse_table,
    _render_table,
)
from swecli.ui_textual.formatters_internal.markdown_formatter import markdown_to_plain_text


def _render_plain(content: str, leading: bool = False) -> list[str]:
    renderables, _ = render_markdown_text_segment(content, leading=leading)
    plains: list[str] = []
    for renderable in renderables:
        if isinstance(renderable, Text):
            plains.append(renderable.plain)
        else:
            plains.append(str(renderable))
    return plains


def test_heading_rendering():
    plains = _render_plain("# Title")
    # Headings now have 4 spaces of indentation (2 extra + 2 standard)
    assert plains == ["    Title"]


def test_nested_bullet_rendering():
    plains = _render_plain("- item\n  - sub item")
    # Bullets now have standard 2-space indent plus bullet prefix
    # Root: "  " + "  - " = "    - " (but leading bullet removes 2 spaces) = "  - item"
    # Nested: "  " + "    - " = "        - sub item" (8 spaces before dash)
    assert plains == ["  - item", "        - sub item"]


def test_blockquote_rendering():
    plains = _render_plain("> quoted text")
    # Blockquotes now have standard 2-space indent
    assert plains == ["  ❝ quoted text"]


def test_horizontal_rule_rendering():
    plains = _render_plain("---")
    # Horizontal rules now have standard 2-space indent
    assert plains == ["  ────────────────────────────────────────"]


def test_ordered_list_rendering():
    plains = _render_plain("1. first\n   1. nested")
    # Ordered lists have standard 2-space indent
    # Nested: "  " + "  " * 1 + "– " = "     – nested" (5 spaces before dash)
    assert plains == ["1. first", "     – nested"]


def test_markdown_to_plain_text_alignment():
    content = """# Heading

- First
  - Nested
> Quote here
"""
    result = markdown_to_plain_text(content)
    lines = [line for line in result.splitlines() if line]
    assert "HEADING" in lines[0]
    assert "• First" in lines[1]
    assert "  – Nested" in lines[2]
    assert lines[-1].startswith(" ❝ Quote")


def test_leading_bullet_with_response_starting_with_bullets():
    """Test that the leading bullet (⏺) appears on the first bullet when response starts with bullets."""
    plains = _render_plain("- First item\n- Second item\n- Third item", leading=True)
    # First bullet should have the leading bullet
    assert plains[0].startswith("⏺")
    assert "First item" in plains[0]
    # Subsequent bullets should NOT have the leading bullet
    assert not plains[1].startswith("⏺")
    assert "Second item" in plains[1]
    assert not plains[2].startswith("⏺")
    assert "Third item" in plains[2]


def test_leading_bullet_with_paragraph_then_bullets():
    """Test that the leading bullet (⏺) appears on the paragraph, not on bullets."""
    plains = _render_plain("Some paragraph\n- First item\n- Second item", leading=True)
    # First paragraph should have the leading bullet
    assert plains[0].startswith("⏺")
    assert "Some paragraph" in plains[0]
    # Bullets should NOT have the leading bullet
    assert not plains[1].startswith("⏺")
    assert "First item" in plains[1]


def test_leading_bullet_with_ordered_list():
    """Test that the leading bullet (⏺) appears on the first ordered item when response starts with list."""
    plains = _render_plain("1. First item\n2. Second item\n3. Third item", leading=True)
    # First item should have the leading bullet
    assert plains[0].startswith("⏺")
    assert "First item" in plains[0]
    # Subsequent items should NOT have the leading bullet
    assert not plains[1].startswith("⏺")
    assert "Second item" in plains[1]


def test_leading_bullet_not_on_nested_bullets():
    """Test that the leading bullet only appears on root-level bullets, not nested ones."""
    plains = _render_plain("  - Nested item\n- Root item", leading=True)
    # First bullet is nested (indent_level > 0), should NOT get leading bullet
    assert not plains[0].startswith("⏺")
    # Root level bullet should get the leading bullet
    assert plains[1].startswith("⏺")
    assert "Root item" in plains[1]


def test_leading_bullet_no_extra_indent():
    """First bullet with ⏺ should not have extra spacing (hanging indent style)."""
    plains = _render_plain("- First\n- Second", leading=True)
    # First bullet should start with "⏺ -" (no extra space between ⏺ and -)
    assert plains[0].startswith("⏺ -")
    # Should NOT have double space between ⏺ and dash
    assert not plains[0].startswith("⏺  -")
    # Second bullet should be indented to create hanging indent effect
    assert plains[1].startswith("  ")
    assert "- Second" in plains[1]


# ===== Table Detection Tests =====


def test_is_table_row():
    """Test table row detection."""
    assert _is_table_row("| Header 1 | Header 2 |")
    assert _is_table_row("| cell | cell |")
    assert _is_table_row("  | cell | cell |  ")  # With whitespace
    assert not _is_table_row("| incomplete")
    assert not _is_table_row("no pipes here")
    assert not _is_table_row("incomplete |")


def test_is_table_separator():
    """Test table separator detection."""
    assert _is_table_separator("|---|---|")
    assert _is_table_separator("| --- | --- |")
    assert _is_table_separator("|:---|---:|")  # Alignment markers
    assert _is_table_separator("|:---:|:---:|")  # Center alignment
    assert _is_table_separator("| ----- | ----------- |")  # Variable dashes
    assert not _is_table_separator("| text | text |")
    assert _is_table_separator("|---|")  # Single column is valid


def test_parse_row():
    """Test parsing a table row into cells."""
    assert _parse_row("| Header 1 | Header 2 |") == ["Header 1", "Header 2"]
    assert _parse_row("|  spaced  |  content  |") == ["spaced", "content"]
    assert _parse_row("| one | two | three |") == ["one", "two", "three"]
    assert _parse_row("| empty ||") == ["empty", ""]


def test_parse_table_basic():
    """Test parsing a basic markdown table."""
    lines = [
        "| Name | Age |",
        "|---|---|",
        "| Alice | 30 |",
        "| Bob | 25 |",
    ]
    rows, end_index = _parse_table(lines, 0)
    assert rows is not None
    assert rows == [["Name", "Age"], ["Alice", "30"], ["Bob", "25"]]
    assert end_index == 4


def test_parse_table_no_separator():
    """Test that table without separator is not parsed as table."""
    lines = [
        "| Name | Age |",
        "| Alice | 30 |",
    ]
    rows, end_index = _parse_table(lines, 0)
    assert rows is None
    assert end_index == 0


def test_parse_table_partial():
    """Test parsing table followed by other content."""
    lines = [
        "| Col1 | Col2 |",
        "|---|---|",
        "| data | data |",
        "Some paragraph after table",
    ]
    rows, end_index = _parse_table(lines, 0)
    assert rows is not None
    assert len(rows) == 2  # Header + 1 data row
    assert end_index == 3


# ===== Table Rendering Tests =====


def test_render_table_basic():
    """Test rendering a basic table."""
    rows = [["Name", "Value"], ["foo", "bar"]]
    table = _render_table(rows)
    assert isinstance(table, Table)
    assert table.row_count == 1  # Data rows only (header is separate)


def test_render_table_with_mismatched_columns():
    """Test that shorter rows are padded."""
    rows = [["A", "B", "C"], ["1", "2"]]  # Missing one cell
    table = _render_table(rows)
    assert isinstance(table, Table)
    assert table.row_count == 1


def test_table_rendering_integration():
    """Test full integration of table rendering in markdown parser."""
    content = """| Repo | Stars |
|---|---|
| project-a | 1000 |
| project-b | 2000 |"""
    renderables, wrote_any = render_markdown_text_segment(content)
    assert wrote_any
    # Should have rendered a Table object
    assert any(isinstance(r, Table) for r in renderables)


def test_table_with_inline_markdown():
    """Test table cells with inline markdown formatting."""
    content = """| Name | Description |
|---|---|
| **bold** | _italic_ |
| `code` | [link](url) |"""
    renderables, wrote_any = render_markdown_text_segment(content)
    assert wrote_any
    assert any(isinstance(r, Table) for r in renderables)


def test_malformed_table_fallback():
    """Test that malformed table is rendered as paragraph."""
    content = "| incomplete header"
    renderables, wrote_any = render_markdown_text_segment(content)
    # Should be rendered as plain text, not a table
    assert not any(isinstance(r, Table) for r in renderables)


def test_table_followed_by_text():
    """Test table followed by regular text."""
    content = """| Col |
|---|
| data |

Some text after the table."""
    renderables, wrote_any = render_markdown_text_segment(content)
    assert wrote_any
    # Should have both table and text
    has_table = any(isinstance(r, Table) for r in renderables)
    has_text = any(isinstance(r, Text) and "Some text" in r.plain for r in renderables)
    assert has_table
    assert has_text
