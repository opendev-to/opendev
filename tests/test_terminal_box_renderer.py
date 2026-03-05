"""Tests for TerminalBoxRenderer."""
import os
from unittest.mock import Mock
from rich.text import Text

from swecli.ui_textual.widgets.terminal_box_renderer import TerminalBoxRenderer, TerminalBoxConfig


def test_format_path():
    """Test format_path utility."""
    renderer = TerminalBoxRenderer(lambda: 80)

    # Setup paths
    home = os.path.expanduser("~")
    path_in_home = os.path.join(home, "projects", "swecli")
    path_outside = "/usr/bin/python"

    # Test home replacement
    assert renderer.format_path(path_in_home) == "~/projects/swecli"

    # Test path outside home
    assert renderer.format_path(path_outside) == "/usr/bin/python"


def test_normalize_line():
    """Test normalize_line utility."""
    renderer = TerminalBoxRenderer(lambda: 80)

    # Test tab expansion
    assert renderer.normalize_line("col1\tcol2") == "col1    col2"

    # Test ANSI stripping
    ansi_text = "\x1b[31mError\x1b[0m message"
    assert renderer.normalize_line(ansi_text) == "Error message"


def test_render_top_border():
    """Test top border rendering."""
    renderer = TerminalBoxRenderer(lambda: 80)
    config = TerminalBoxConfig(box_width=40, depth=0)

    result = renderer.render_top_border(config)
    assert isinstance(result, Text)
    # Check width: indent (0) + content (40)
    # Actually the implementation is:
    # Text(f"{indent}  \u23bf ", style=self.POINTER_COLOR)
    # + box width chars
    # wait, the code says:
    # top = Text(f"{indent}  \u23bf ", style=self.POINTER_COLOR) -> "  ⎿ " (4 chars)
    # top.append("\u256d" + "\u2500" * (box_width - 2) + "\u256e", style=border) -> 1 + (W-2) + 1 = W chars
    # Total logical characters = 4 + W

    plain_text = result.plain
    assert "⎿" in plain_text
    assert "╭" in plain_text
    assert "╮" in plain_text
    assert len(plain_text) == 4 + 40


def test_render_prompt_line():
    """Test prompt line rendering."""
    renderer = TerminalBoxRenderer(lambda: 80)
    config = TerminalBoxConfig(
        command="ls -la",
        working_dir="/tmp",
        box_width=40,
        depth=0
    )

    result = renderer.render_prompt_line(config)
    plain_text = result.plain

    assert "/tmp" in plain_text
    assert "$ ls -la" in plain_text
    assert "│" in plain_text

    # Check padding ensures alignment
    # prefix is "      " (4 spaces indent + 2 spaces padding? No)
    # implementation:
    # prompt_line = Text(f"{indent}    ") -> 4 spaces
    # prompt_line.append("\u2502  ", style=border) -> "│  " (3 chars)
    # Total prefix before path = 7 chars
    # Then path + " $ " + command + padding + " │"

    assert plain_text.endswith("│")


def test_render_content_line_truncation():
    """Test that content lines are truncated to fit."""
    renderer = TerminalBoxRenderer(lambda: 80)
    # box_width 20
    # content_width = 20 - 5 = 15
    config = TerminalBoxConfig(box_width=20, depth=0)

    long_line = "This is a very long line that should be truncated"
    result = renderer.render_content_line(long_line, config)

    plain_text = result.plain
    content_part = plain_text.strip().strip("│").strip()

    assert len(content_part) <= 15
    assert "This is a very" in content_part
    assert "truncated" not in content_part


def test_render_content_line_error_style():
    """Test error styling on content line."""
    renderer = TerminalBoxRenderer(lambda: 80)
    config = TerminalBoxConfig(box_width=40, is_error=True)

    result = renderer.render_content_line("Error occurred", config, apply_error_style=True)

    # Find the span with error color
    error_spans = [
        span for span in result.spans
        if span.style == renderer.ERROR_COLOR
    ]
    assert len(error_spans) > 0


def test_render_complete_box():
    """Test full box rendering."""
    renderer = TerminalBoxRenderer(lambda: 80)
    config = TerminalBoxConfig(
        command="echo hello",
        working_dir="~",
        box_width=40,
        depth=1
    )

    lines = ["Hello world", "Line 2"]
    result = renderer.render_complete_box(lines, config)

    assert len(result) == 3 + len(lines) + 2
    # Top border (1)
    # Padding (1)
    # Prompt (1)
    # Content (2)
    # Padding (1)
    # Bottom border (1)
    # Total = 7 lines

    assert len(result) == 7

    # Check indentation (depth 1 = 2 spaces)
    assert result[0].plain.startswith("    ") # 2 spaces indent + 2 spaces before pointer?
    # implementation: f"{indent}  \u23bf " -> indent is "  ", so "    ⎿ "
    assert result[0].plain.startswith("    ⎿")
