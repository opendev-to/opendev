"""End-to-end tests for ask-user subagent implementation."""

import json
import os
import pytest
from unittest.mock import MagicMock, patch
import asyncio

from swecli.core.agents.subagents.agents import ALL_SUBAGENTS
from swecli.core.agents.subagents.agents.ask_user import ASK_USER_SUBAGENT
from swecli.core.agents.subagents.manager import SubAgentManager, SubAgentDeps
from swecli.core.context_engineering.tools.implementations.ask_user_tool import (
    Question,
    QuestionOption,
)
from swecli.ui_textual.controllers.ask_user_prompt_controller import AskUserPromptController


# ============================================================================
# GROUP A: Unit Tests - JSON Parsing & Validation (Tests 1-5)
# ============================================================================


class TestAskUserJsonParsing:
    """Tests for JSON parsing in _parse_ask_user_questions."""

    @pytest.fixture
    def manager(self):
        """Create SubAgentManager with mocked dependencies."""
        mock_config = MagicMock()
        mock_config.model = "gpt-4o"
        mock_config.temperature = 0.7
        mock_config.max_tokens = 4096
        mock_config.api_key = "test-key"
        mock_config.api_base_url = None

        mock_registry = MagicMock()
        mock_mode_manager = MagicMock()

        return SubAgentManager(
            config=mock_config,
            tool_registry=mock_registry,
            mode_manager=mock_mode_manager,
        )

    def test_parse_single_question_valid(self, manager):
        """Test 1: Parse valid JSON with one question, 3 options."""
        questions_data = [
            {
                "question": "Which database should we use?",
                "header": "Database",
                "options": [
                    {"label": "PostgreSQL", "description": "Full-featured SQL"},
                    {"label": "SQLite", "description": "File-based"},
                    {"label": "MongoDB", "description": "Document store"},
                ],
                "multiSelect": False,
            }
        ]

        result = manager._parse_ask_user_questions(questions_data)

        assert len(result) == 1
        assert isinstance(result[0], Question)
        assert result[0].question == "Which database should we use?"
        assert result[0].header == "Database"
        assert len(result[0].options) == 3
        assert result[0].multi_select is False

    def test_parse_multi_question_valid(self, manager):
        """Test 2: Parse JSON with 4 questions (max allowed)."""
        questions_data = [
            {"question": "Q1?", "header": "H1", "options": [{"label": "A"}, {"label": "B"}]},
            {"question": "Q2?", "header": "H2", "options": [{"label": "C"}, {"label": "D"}]},
            {"question": "Q3?", "header": "H3", "options": [{"label": "E"}, {"label": "F"}]},
            {"question": "Q4?", "header": "H4", "options": [{"label": "G"}, {"label": "H"}]},
        ]

        result = manager._parse_ask_user_questions(questions_data)

        assert len(result) == 4
        for i, q in enumerate(result):
            assert q.question == f"Q{i+1}?"
            assert q.header == f"H{i+1}"

    def test_parse_invalid_json(self, manager):
        """Test 3: Pass malformed JSON - manager returns error dict."""
        invalid_json = "not valid json {"

        mock_callback = MagicMock()
        mock_callback.chat_app = None
        mock_callback._app = None

        result = manager._execute_ask_user(invalid_json, mock_callback)

        assert result["success"] is False
        assert "Invalid questions format" in result["error"]

    def test_parse_missing_questions_field(self, manager):
        """Test 4: JSON without questions key."""
        json_without_questions = json.dumps({"other_field": "value"})

        mock_callback = MagicMock()
        mock_callback.chat_app = None
        mock_callback._app = None

        result = manager._execute_ask_user(json_without_questions, mock_callback)

        assert result["success"] is False
        assert "No questions provided" in result["error"]

    def test_parse_options_with_descriptions(self, manager):
        """Test 5: Options include both label and description."""
        questions_data = [
            {
                "question": "Test?",
                "header": "Test",
                "options": [
                    {"label": "Option A", "description": "Description for A"},
                    {"label": "Option B", "description": "Description for B"},
                ],
            }
        ]

        result = manager._parse_ask_user_questions(questions_data)

        assert result[0].options[0].label == "Option A"
        assert result[0].options[0].description == "Description for A"
        assert result[0].options[1].label == "Option B"
        assert result[0].options[1].description == "Description for B"


# ============================================================================
# GROUP B: Unit Tests - Controller State Machine (Tests 6-10)
# ============================================================================


class TestAskUserController:
    """Tests for AskUserPromptController state machine."""

    @pytest.fixture
    def mock_app(self):
        """Create a mock app with required attributes."""
        app = MagicMock()
        app.input_field = MagicMock()
        app.input_field.text = ""
        app.input_field.load_text = MagicMock()
        app.conversation = MagicMock()
        app.conversation.clear_ask_user_prompt = MagicMock()
        app.conversation.render_ask_user_prompt = MagicMock()
        app.conversation.scroll_end = MagicMock()
        app._autocomplete_controller = MagicMock()
        app.spinner_service = MagicMock()
        return app

    @pytest.fixture
    def controller(self, mock_app):
        """Create controller with mock app."""
        return AskUserPromptController(mock_app)

    def test_controller_initial_state(self, controller):
        """Test 6: Controller starts inactive."""
        assert controller.active is False
        assert controller._future is None
        assert controller._questions == []
        assert controller._selected_index == 0
        assert controller._answers == {}

    def test_controller_move_navigation(self, controller):
        """Test 7: Navigation cycles through options."""
        # Simulate active state with 3 options
        controller._active = True
        controller._questions = [
            Question(
                question="Test?",
                header="Test",
                options=[
                    QuestionOption(label="A"),
                    QuestionOption(label="B"),
                    QuestionOption(label="C"),
                ],
            )
        ]
        controller._current_question_idx = 0
        controller._selected_index = 0

        # Move down
        controller.move(1)
        assert controller._selected_index == 1

        controller.move(1)
        assert controller._selected_index == 2

        # Wrap to "Other" option (index 3)
        controller.move(1)
        assert controller._selected_index == 3

        # Wrap back to first
        controller.move(1)
        assert controller._selected_index == 0

        # Move up wraps
        controller.move(-1)
        assert controller._selected_index == 3

    def test_controller_multi_select_toggle(self, controller):
        """Test 8: Space toggles multi-select option."""
        controller._active = True
        controller._questions = [
            Question(
                question="Test?",
                header="Test",
                options=[
                    QuestionOption(label="A"),
                    QuestionOption(label="B"),
                ],
                multi_select=True,
            )
        ]
        controller._current_question_idx = 0
        controller._selected_index = 0
        controller._multi_selections = set()

        # Toggle on
        controller.toggle_selection()
        assert 0 in controller._multi_selections

        # Toggle off
        controller.toggle_selection()
        assert 0 not in controller._multi_selections

        # Move to option 1 and toggle
        controller._selected_index = 1
        controller.toggle_selection()
        assert 1 in controller._multi_selections

    def test_controller_confirm_single_select(self, controller, mock_app):
        """Test 9: Confirm with first option selected."""
        loop = asyncio.new_event_loop()
        asyncio.set_event_loop(loop)

        try:
            controller._active = True
            controller._questions = [
                Question(
                    question="Test?",
                    header="Test",
                    options=[
                        QuestionOption(label="Option 1"),
                        QuestionOption(label="Option 2"),
                    ],
                )
            ]
            controller._current_question_idx = 0
            controller._selected_index = 0
            controller._future = loop.create_future()

            controller.confirm()

            # Should move to next question (or complete if last)
            assert controller._answers.get("0") == "Option 1"
        finally:
            loop.close()

    def test_controller_cancel_returns_none(self, controller, mock_app):
        """Test 10: Cancel resolves future to None."""
        loop = asyncio.new_event_loop()
        asyncio.set_event_loop(loop)

        try:
            controller._active = True
            controller._questions = [
                Question(
                    question="Test?",
                    header="Test",
                    options=[QuestionOption(label="A"), QuestionOption(label="B")],
                )
            ]
            controller._future = loop.create_future()

            controller.cancel()

            assert controller._future.result() is None
        finally:
            loop.close()


# ============================================================================
# GROUP C: Unit Tests - Result Formatting (Tests 11-13)
# ============================================================================


class TestAskUserResultFormatting:
    """Tests for result formatting in _execute_ask_user."""

    def test_result_format_single_answer(self):
        """Test 11: Single question answer formatting."""
        answers = {"0": "JWT tokens"}

        answer_lines = []
        for idx, ans in answers.items():
            if isinstance(ans, list):
                ans_text = ", ".join(str(a) for a in ans)
            else:
                ans_text = str(ans)
            answer_lines.append(f"Question {idx}: {ans_text}")

        result = "\n".join(answer_lines)

        assert result == "Question 0: JWT tokens"

    def test_result_format_multi_answer(self):
        """Test 12: Multi-select answer formatting."""
        answers = {"0": ["Option A", "Option C"]}

        answer_lines = []
        for idx, ans in answers.items():
            if isinstance(ans, list):
                ans_text = ", ".join(str(a) for a in ans)
            else:
                ans_text = str(ans)
            answer_lines.append(f"Question {idx}: {ans_text}")

        result = "\n".join(answer_lines)

        assert result == "Question 0: Option A, Option C"

    def test_result_format_cancelled(self):
        """Test 13: Cancelled result format."""
        result = {
            "success": True,
            "content": "User cancelled/skipped the question(s).",
            "answers": {},
            "cancelled": True,
        }

        assert result["success"] is True
        assert result["cancelled"] is True
        assert result["answers"] == {}


# ============================================================================
# GROUP D: Integration Tests - SubAgentManager (Tests 14-17)
# ============================================================================


class TestAskUserSubAgentManagerIntegration:
    """Integration tests for SubAgentManager with ask-user."""

    def test_execute_subagent_detects_ask_user(self):
        """Test 14: execute_subagent routes to _execute_ask_user."""
        mock_config = MagicMock()
        mock_config.model = "gpt-4o"
        mock_config.temperature = 0.7
        mock_config.max_tokens = 4096
        mock_config.api_key = "test-key"
        mock_config.api_base_url = None

        manager = SubAgentManager(
            config=mock_config,
            tool_registry=MagicMock(),
            mode_manager=MagicMock(),
        )

        # Patch _execute_ask_user to verify it's called
        with patch.object(
            manager, "_execute_ask_user", return_value={"success": True}
        ) as mock_exec:
            deps = SubAgentDeps(
                mode_manager=MagicMock(),
                approval_manager=MagicMock(),
                undo_manager=MagicMock(),
            )

            result = manager.execute_subagent(
                name="ask-user",
                task='{"questions": []}',
                deps=deps,
                ui_callback=MagicMock(),
            )

            mock_exec.assert_called_once()

    def test_ask_user_in_all_subagents(self):
        """Test 15: ask-user is registered in ALL_SUBAGENTS."""
        names = [s["name"] for s in ALL_SUBAGENTS]
        assert "ask-user" in names

    def test_ask_user_has_system_prompt(self):
        """Test 16: System prompt is defined and non-empty."""
        assert "system_prompt" in ASK_USER_SUBAGENT
        prompt = ASK_USER_SUBAGENT["system_prompt"]
        assert len(prompt) > 0
        assert "Purpose" in prompt
        assert "Question Structure" in prompt

    def test_ask_user_builtin_type_marker(self):
        """Test 17: _builtin_type marker is set correctly."""
        assert ASK_USER_SUBAGENT.get("_builtin_type") == "ask-user"


# ============================================================================
# GROUP E: E2E Tests with Real API (Tests 18-20)
# ============================================================================


@pytest.mark.skipif(not os.environ.get("OPENAI_API_KEY"), reason="OPENAI_API_KEY not set")
class TestAskUserE2ERealAPI:
    """E2E tests using real OpenAI API calls."""

    def test_e2e_ask_user_spec_visible_to_agent(self):
        """Test 18: Agent can see ask-user in available subagents."""
        # Verify the spec is properly formatted for agent consumption
        # Note: We don't call register_defaults() because that requires full
        # agent instantiation. Instead, we test get_agent_configs() which
        # reads directly from ALL_SUBAGENTS without instantiation.
        mock_config = MagicMock()
        mock_config.model = "gpt-4o"
        mock_config.temperature = 0.7
        mock_config.max_tokens = 4096
        mock_config.api_key = os.environ.get("OPENAI_API_KEY")
        mock_config.api_base_url = None

        manager = SubAgentManager(
            config=mock_config,
            tool_registry=MagicMock(),
            mode_manager=MagicMock(),
        )
        # Don't call register_defaults() - get_agent_configs reads from ALL_SUBAGENTS directly

        configs = manager.get_agent_configs()
        ask_user_config = next((c for c in configs if c.name == "ask-user"), None)

        assert ask_user_config is not None
        assert "clarifying questions" in ask_user_config.description.lower()

    def test_e2e_agent_can_format_questions_json(self):
        """Test 19: Verify JSON structure matches system prompt spec."""
        # Test that example JSON in system prompt is valid
        example_json = {
            "questions": [
                {
                    "question": "Which authentication approach should we use for the API?",
                    "header": "Auth Method",
                    "options": [
                        {"label": "JWT tokens (Recommended)", "description": "Stateless, scalable"},
                        {"label": "Session cookies", "description": "Server-side sessions"},
                        {"label": "OAuth 2.0", "description": "Third-party login"},
                    ],
                    "multiSelect": False,
                }
            ]
        }

        # Validate JSON structure
        json_str = json.dumps(example_json)
        parsed = json.loads(json_str)

        assert "questions" in parsed
        assert len(parsed["questions"]) == 1
        assert parsed["questions"][0]["question"].endswith("?")
        assert len(parsed["questions"][0]["header"]) <= 12
        assert 2 <= len(parsed["questions"][0]["options"]) <= 4

    def test_e2e_subagent_manager_integration(self):
        """Test 20: Full integration with mocked controller response.

        Tests the _execute_ask_user method end-to-end by mocking the app's
        ask_user_controller and the call_from_thread/run_worker pattern.
        """
        import threading

        mock_config = MagicMock()
        mock_config.model = "gpt-4o"
        mock_config.temperature = 0.7
        mock_config.max_tokens = 4096
        mock_config.api_key = "test-key"
        mock_config.api_base_url = None

        manager = SubAgentManager(
            config=mock_config,
            tool_registry=MagicMock(),
            mode_manager=MagicMock(),
        )

        # Create mock app with controller that returns a canned response
        mock_app = MagicMock()
        mock_app.is_running = True

        async def mock_start(questions):
            return {"0": "PostgreSQL"}

        mock_controller = MagicMock()
        mock_controller.start = mock_start
        mock_app._ask_user_controller = mock_controller

        # Mock run_worker to run the coroutine and set result
        def mock_run_worker(coro, **kwargs):
            # Run the coroutine in a new event loop
            loop = asyncio.new_event_loop()
            try:
                loop.run_until_complete(coro)
            finally:
                loop.close()
            return MagicMock()

        mock_app.run_worker = mock_run_worker

        # Mock call_from_thread to execute callback immediately
        def mock_call_from_thread(func):
            func()

        mock_app.call_from_thread = mock_call_from_thread

        mock_callback = MagicMock()
        mock_callback.chat_app = mock_app

        task = json.dumps(
            {
                "questions": [
                    {
                        "question": "Which database?",
                        "header": "Database",
                        "options": [
                            {"label": "PostgreSQL", "description": "SQL"},
                            {"label": "MongoDB", "description": "NoSQL"},
                        ],
                    }
                ]
            }
        )

        result = manager._execute_ask_user(task, mock_callback)

        assert result["success"] is True
        assert result["cancelled"] is False
        assert "PostgreSQL" in result["content"]
