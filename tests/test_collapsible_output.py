"""Tests for CollapsibleOutput model."""

import pytest
from swecli.ui_textual.models.collapsible_output import CollapsibleOutput


class TestCollapsibleOutputBasic:
    """Test basic CollapsibleOutput functionality."""

    def test_creation(self):
        """Test basic creation of CollapsibleOutput."""
        co = CollapsibleOutput(
            start_line=10,
            end_line=15,
            full_content=["line1", "line2", "line3"],
            summary="3 lines",
        )
        assert co.start_line == 10
        assert co.end_line == 15
        assert co.line_count == 3
        assert co.summary == "3 lines"
        assert not co.is_expanded

    def test_default_values(self):
        """Test default values are set correctly."""
        co = CollapsibleOutput(
            start_line=0,
            end_line=0,
            full_content=["test"],
            summary="1 line",
        )
        assert co.output_type == "bash"
        assert co.command == ""
        assert co.working_dir == "."
        assert not co.is_error
        assert co.depth == 0


class TestCollapsibleOutputToggle:
    """Test toggle functionality."""

    def test_toggle_expands(self):
        """Test toggle from collapsed to expanded."""
        co = CollapsibleOutput(
            start_line=0,
            end_line=0,
            full_content=["test"],
            summary="1 line",
            is_expanded=False,
        )
        result = co.toggle()
        assert result is True
        assert co.is_expanded is True

    def test_toggle_collapses(self):
        """Test toggle from expanded to collapsed."""
        co = CollapsibleOutput(
            start_line=0,
            end_line=0,
            full_content=["test"],
            summary="1 line",
            is_expanded=True,
        )
        result = co.toggle()
        assert result is False
        assert co.is_expanded is False

    def test_expand(self):
        """Test explicit expand."""
        co = CollapsibleOutput(
            start_line=0,
            end_line=0,
            full_content=["test"],
            summary="1 line",
        )
        co.expand()
        assert co.is_expanded is True

    def test_collapse(self):
        """Test explicit collapse."""
        co = CollapsibleOutput(
            start_line=0,
            end_line=0,
            full_content=["test"],
            summary="1 line",
            is_expanded=True,
        )
        co.collapse()
        assert co.is_expanded is False


class TestCollapsibleOutputContainsLine:
    """Test contains_line method."""

    def test_contains_start_line(self):
        """Test line at start is contained."""
        co = CollapsibleOutput(
            start_line=10,
            end_line=20,
            full_content=["test"],
            summary="1 line",
        )
        assert co.contains_line(10) is True

    def test_contains_end_line(self):
        """Test line at end is contained."""
        co = CollapsibleOutput(
            start_line=10,
            end_line=20,
            full_content=["test"],
            summary="1 line",
        )
        assert co.contains_line(20) is True

    def test_contains_middle_line(self):
        """Test line in middle is contained."""
        co = CollapsibleOutput(
            start_line=10,
            end_line=20,
            full_content=["test"],
            summary="1 line",
        )
        assert co.contains_line(15) is True

    def test_does_not_contain_before(self):
        """Test line before range is not contained."""
        co = CollapsibleOutput(
            start_line=10,
            end_line=20,
            full_content=["test"],
            summary="1 line",
        )
        assert co.contains_line(5) is False

    def test_does_not_contain_after(self):
        """Test line after range is not contained."""
        co = CollapsibleOutput(
            start_line=10,
            end_line=20,
            full_content=["test"],
            summary="1 line",
        )
        assert co.contains_line(25) is False


class TestCollapsibleOutputDisplayLines:
    """Test get_display_lines method."""

    def test_short_content_returns_all(self):
        """Test short content returns all lines."""
        co = CollapsibleOutput(
            start_line=0,
            end_line=0,
            full_content=["line1", "line2", "line3"],
            summary="3 lines",
        )
        display = co.get_display_lines(head_count=5, tail_count=5)
        assert len(display) == 3
        assert display == ["line1", "line2", "line3"]

    def test_long_content_truncated(self):
        """Test long content is truncated."""
        lines = [f"line{i}" for i in range(20)]
        co = CollapsibleOutput(
            start_line=0,
            end_line=0,
            full_content=lines,
            summary="20 lines",
        )
        display = co.get_display_lines(head_count=3, tail_count=3)
        # Should have 3 head + 1 hidden message + 3 tail = 7 lines
        assert len(display) == 7
        assert "hidden" in display[3]

    def test_expanded_returns_all(self):
        """Test expanded content returns all lines."""
        lines = [f"line{i}" for i in range(20)]
        co = CollapsibleOutput(
            start_line=0,
            end_line=0,
            full_content=lines,
            summary="20 lines",
            is_expanded=True,
        )
        display = co.get_display_lines(head_count=3, tail_count=3)
        assert len(display) == 20


class TestCollapsibleOutputMemoryLimit:
    """Test memory limit enforcement."""

    def test_content_within_limit(self):
        """Test content within limit is not truncated."""
        lines = [f"line{i}" for i in range(100)]
        co = CollapsibleOutput(
            start_line=0,
            end_line=0,
            full_content=lines,
            summary="100 lines",
        )
        assert co.line_count == 100

    def test_content_exceeds_limit_truncated(self):
        """Test content exceeding limit is truncated."""
        lines = [f"line{i}" for i in range(15000)]
        co = CollapsibleOutput(
            start_line=0,
            end_line=0,
            full_content=lines,
            summary="15000 lines",
        )
        # Should be truncated to MAX_STORED_LINES (10000)
        assert co.line_count == 10000
