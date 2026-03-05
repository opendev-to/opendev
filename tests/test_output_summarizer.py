"""Tests for output_summarizer utility."""

import pytest
from swecli.ui_textual.utils.output_summarizer import (
    summarize_output,
    get_expansion_hint,
    _format_line_count,
    _detect_pytest_summary,
    _detect_npm_summary,
    _detect_git_log_summary,
    _detect_git_status_summary,
)


class TestSummarizeOutput:
    """Test summarize_output function."""

    def test_empty_input(self):
        """Test empty input returns 'empty output'."""
        assert summarize_output([]) == "empty output"

    def test_pytest_passed(self):
        """Test pytest output with passing tests."""
        lines = ["===== 5 passed in 1.23s ====="]
        assert summarize_output(lines, "bash") == "5 passed"

    def test_pytest_passed_and_failed(self):
        """Test pytest output with passing and failing tests."""
        lines = ["5 passed, 2 failed"]
        assert summarize_output(lines, "bash") == "5 passed, 2 failed"

    def test_npm_install(self):
        """Test npm install output."""
        lines = ["added 142 packages in 5s"]
        assert summarize_output(lines, "bash") == "142 packages installed"

    def test_npm_up_to_date(self):
        """Test npm already up to date."""
        lines = ["up to date, audited 100 packages in 1s"]
        assert summarize_output(lines, "bash") == "up to date"

    def test_git_log(self):
        """Test git log output detection."""
        lines = [
            "a1b2c3d Fix bug",
            "e4f5a6b Add feature",
            "c7d8e9f Initial commit",
        ]
        assert summarize_output(lines, "bash") == "3 commits"

    def test_git_status_clean(self):
        """Test git status with clean working tree."""
        lines = ["On branch main", "nothing to commit, working tree clean"]
        assert summarize_output(lines, "bash") == "clean working tree"

    def test_generic_line_count(self):
        """Test fallback to generic line count."""
        lines = ["some random output"] * 50
        assert summarize_output(lines, "bash") == "50 lines"

    def test_single_line_singular(self):
        """Test single line uses singular form."""
        lines = ["hello world"]
        assert summarize_output(lines, "bash") == "1 line"


class TestExpansionHint:
    """Test expansion hint text."""

    def test_hint_text(self):
        """Test hint contains ctrl+o."""
        assert "ctrl+o" in get_expansion_hint()


class TestFormatLineCount:
    """Test line count formatting."""

    def test_single_line(self):
        """Test single line formatting."""
        assert _format_line_count(1) == "1 line"

    def test_multiple_lines(self):
        """Test multiple lines formatting."""
        assert _format_line_count(42) == "42 lines"

    def test_zero_lines(self):
        """Test zero lines formatting."""
        assert _format_line_count(0) == "0 lines"


class TestPytestSummary:
    """Test pytest summary detection."""

    def test_passed_only(self):
        """Test only passed tests."""
        text = "10 passed"
        assert _detect_pytest_summary(text) == "10 passed"

    def test_passed_and_failed(self):
        """Test passed and failed tests."""
        text = "5 passed, 3 failed"
        result = _detect_pytest_summary(text)
        assert result == "5 passed, 3 failed"

    def test_with_warnings(self):
        """Test with warnings."""
        text = "5 passed, 2 warnings"
        result = _detect_pytest_summary(text)
        assert "5 passed" in result
        assert "2 warnings" in result

    def test_no_match(self):
        """Test no pytest output."""
        text = "hello world"
        assert _detect_pytest_summary(text) is None


class TestNpmSummary:
    """Test npm summary detection."""

    def test_added_packages(self):
        """Test added packages."""
        text = "added 142 packages in 5s"
        assert _detect_npm_summary(text) == "142 packages installed"

    def test_removed_packages(self):
        """Test removed packages."""
        text = "removed 5 packages in 1s"
        assert _detect_npm_summary(text) == "5 packages removed"

    def test_up_to_date(self):
        """Test up to date."""
        text = "up to date"
        assert _detect_npm_summary(text) == "up to date"


class TestGitLogSummary:
    """Test git log summary detection."""

    def test_short_format(self):
        """Test short commit format (hex chars only)."""
        lines = [
            "a1b2c3d Fix bug",
            "e4f5a6b Add feature",
        ]
        assert _detect_git_log_summary(lines) == "2 commits"

    def test_single_commit(self):
        """Test single commit."""
        lines = ["a1b2c3d Initial commit"]
        assert _detect_git_log_summary(lines) == "1 commit"

    def test_no_commits(self):
        """Test no commit patterns."""
        lines = ["not a commit", "just random text"]
        assert _detect_git_log_summary(lines) is None


class TestGitStatusSummary:
    """Test git status summary detection."""

    def test_clean_working_tree(self):
        """Test clean working tree."""
        text = "nothing to commit, working tree clean"
        assert _detect_git_status_summary(text) == "clean working tree"

    def test_modified_files(self):
        """Test modified files detected."""
        text = " M file1.py\n M file2.py"
        result = _detect_git_status_summary(text)
        assert result is not None
        assert "modified" in result

    def test_untracked_files(self):
        """Test untracked files detected."""
        text = "?? newfile.py\n?? otherfile.py"
        result = _detect_git_status_summary(text)
        assert result is not None
        assert "untracked" in result
