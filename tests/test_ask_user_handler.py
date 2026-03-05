"""Unit tests for AskUserHandler, verifying answers are included in output."""

import pytest
from unittest.mock import MagicMock

from swecli.core.context_engineering.tools.handlers.ask_user_handler import AskUserHandler


class TestAskUserHandlerOutputFormat:
    """Test suite for AskUserHandler output formatting."""

    @pytest.fixture
    def mock_tool(self):
        """Create a mock AskUserTool."""
        return MagicMock()

    @pytest.fixture
    def handler(self, mock_tool):
        """Create an AskUserHandler with mock tool."""
        return AskUserHandler(mock_tool)

    def test_output_includes_answers_with_headers(self, handler, mock_tool):
        """Test that output field includes actual answers with headers in compact format."""
        # Mock the tool to return answers
        mock_tool.ask.return_value = {
            "success": True,
            "cancelled": False,
            "answers": {"0": "PostgreSQL", "1": "REST API"},
        }

        questions = [
            {
                "question": "Which database?",
                "header": "Database",
                "options": [
                    {"label": "PostgreSQL", "description": "SQL database"},
                    {"label": "MongoDB", "description": "NoSQL database"},
                ],
            },
            {
                "question": "Which API style?",
                "header": "API",
                "options": [
                    {"label": "REST API", "description": "Traditional REST"},
                    {"label": "GraphQL", "description": "Query language"},
                ],
            },
        ]

        result = handler.ask_questions({"questions": questions})

        assert result["success"] is True
        assert result["cancelled"] is False
        assert "2/2" in result["output"]
        # Compact format: [Header]=Answer
        assert "[Database]=PostgreSQL" in result["output"]
        assert "[API]=REST API" in result["output"]
        # Should be single line (no newlines)
        assert "\n" not in result["output"]

    def test_output_includes_answers_without_headers(self, handler, mock_tool):
        """Test that output field uses Q# format when no header provided."""
        mock_tool.ask.return_value = {
            "success": True,
            "cancelled": False,
            "answers": {"0": "Option A", "1": "Option B"},
        }

        questions = [
            {
                "question": "First question?",
                "header": "",  # Empty header -> uses Q1
                "options": [{"label": "Option A"}, {"label": "Option B"}],
            },
            {
                "question": "Second question?",
                # No header at all -> uses Q2
                "options": [{"label": "Option C"}, {"label": "Option D"}],
            },
        ]

        result = handler.ask_questions({"questions": questions})

        assert result["success"] is True
        # Empty header falls back to Q# format
        assert "[Q1]=Option A" in result["output"]
        assert "[Q2]=Option B" in result["output"]

    def test_output_shows_not_answered_for_missing_answers(self, handler, mock_tool):
        """Test that missing answers show (not answered)."""
        mock_tool.ask.return_value = {
            "success": True,
            "cancelled": False,
            "answers": {"0": "Selected option"},  # Only first question answered
        }

        questions = [
            {
                "question": "First question?",
                "header": "First",
                "options": [{"label": "Option A"}, {"label": "Option B"}],
            },
            {
                "question": "Second question?",
                "header": "Second",
                "options": [{"label": "Option C"}, {"label": "Option D"}],
            },
        ]

        result = handler.ask_questions({"questions": questions})

        assert result["success"] is True
        assert "1/2" in result["output"]
        assert "[First]=Selected option" in result["output"]
        assert "[Second]=(not answered)" in result["output"]

    def test_cancelled_output_unchanged(self, handler, mock_tool):
        """Test that cancelled response output is unchanged."""
        mock_tool.ask.return_value = {
            "success": True,
            "cancelled": True,
            "answers": {},
        }

        questions = [
            {
                "question": "Which option?",
                "header": "Choice",
                "options": [{"label": "A"}, {"label": "B"}],
            }
        ]

        result = handler.ask_questions({"questions": questions})

        assert result["success"] is True
        assert result["cancelled"] is True
        assert result["output"] == "User cancelled the questions"

    def test_answers_dict_still_included(self, handler, mock_tool):
        """Test that the answers dict is still returned separately."""
        mock_tool.ask.return_value = {
            "success": True,
            "cancelled": False,
            "answers": {"0": "Answer 1", "1": "Answer 2"},
        }

        questions = [
            {"question": "Q1?", "header": "H1", "options": [{"label": "A"}, {"label": "B"}]},
            {"question": "Q2?", "header": "H2", "options": [{"label": "C"}, {"label": "D"}]},
        ]

        result = handler.ask_questions({"questions": questions})

        # Both output string AND answers dict should be present
        assert "answers" in result
        assert result["answers"] == {"0": "Answer 1", "1": "Answer 2"}
        assert "[H1]=Answer 1" in result["output"]

    def test_no_tool_returns_error(self, mock_tool):
        """Test that missing tool returns error."""
        handler = AskUserHandler(ask_user_tool=None)

        result = handler.ask_questions({"questions": [{"question": "Test?"}]})

        assert result["success"] is False
        assert "not available" in result["error"]

    def test_no_questions_returns_error(self, handler, mock_tool):
        """Test that empty questions list returns error."""
        result = handler.ask_questions({"questions": []})

        assert result["success"] is False
        assert "At least one question is required" in result["error"]

    def test_tool_error_propagates(self, handler, mock_tool):
        """Test that tool errors are propagated correctly."""
        mock_tool.ask.return_value = {
            "success": False,
            "error": "UI not available",
        }

        questions = [
            {"question": "Test?", "header": "Test", "options": [{"label": "A"}, {"label": "B"}]}
        ]

        result = handler.ask_questions({"questions": questions})

        assert result["success"] is False
        assert "UI not available" in result["error"]
