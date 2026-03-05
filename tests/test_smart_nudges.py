"""Tests for smarter nudge system with error classification."""

import pytest

from swecli.core.agents.main_agent import MainAgent


class TestErrorClassification:
    """Tests for _classify_error static method."""

    def test_permission_error(self) -> None:
        assert MainAgent._classify_error("Permission denied: /etc/shadow") == "permission_error"

    def test_file_not_found(self) -> None:
        assert MainAgent._classify_error("No such file or directory: foo.py") == "file_not_found"

    def test_file_not_found_variant(self) -> None:
        assert MainAgent._classify_error("Error: File not found: bar.py") == "file_not_found"

    def test_edit_mismatch(self) -> None:
        assert (
            MainAgent._classify_error("Error: old_content not found in file") == "edit_mismatch"
        )

    def test_syntax_error(self) -> None:
        assert MainAgent._classify_error("SyntaxError: invalid syntax") == "syntax_error"

    def test_rate_limit(self) -> None:
        assert MainAgent._classify_error("HTTP 429: Rate limit exceeded") == "rate_limit"

    def test_timeout(self) -> None:
        assert MainAgent._classify_error("Command timed out after 30s") == "timeout"

    def test_generic_fallback(self) -> None:
        assert MainAgent._classify_error("Something went wrong") == "generic"


class TestSmartNudges:
    """Tests for _get_smart_nudge method."""

    @pytest.fixture()
    def agent(self) -> MainAgent:
        """Create a minimal MainAgent for testing."""
        from unittest.mock import MagicMock

        agent = MainAgent.__new__(MainAgent)
        agent.config = MagicMock()
        return agent

    def test_nudge_for_file_not_found(self, agent: MainAgent) -> None:
        """Nudge for file_not_found should suggest list_files/search."""
        nudge = agent._get_smart_nudge("Error: No such file or directory: foo.py")
        assert "list_files" in nudge or "search" in nudge

    def test_nudge_for_permission_error(self, agent: MainAgent) -> None:
        """Nudge for permission error should mention permissions."""
        nudge = agent._get_smart_nudge("Error: Permission denied: /etc/shadow")
        assert "permission" in nudge.lower()

    def test_nudge_for_edit_mismatch(self, agent: MainAgent) -> None:
        """Nudge for edit mismatch should suggest reading file again."""
        nudge = agent._get_smart_nudge("Error: old_content not found in file")
        assert "read" in nudge.lower()

    def test_generic_nudge_unchanged(self, agent: MainAgent) -> None:
        """Unclassified errors should use existing generic nudge."""
        nudge = agent._get_smart_nudge("Error: Something went wrong")
        assert "failed" in nudge.lower() or "fix" in nudge.lower()
