"""Tests for LLM-based topic detection."""

import json
from dataclasses import dataclass
from typing import Optional
from unittest.mock import MagicMock, patch

from swecli.core.context_engineering.history.topic_detector import TopicDetector
from swecli.models.config import AppConfig


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------


@dataclass
class FakeResponse:
    """Minimal response object matching what AgentHttpClient returns."""

    status_code: int
    _json_data: dict

    def json(self) -> dict:
        return self._json_data


@dataclass
class FakeHttpResult:
    success: bool
    response: Optional[FakeResponse] = None
    error: Optional[str] = None


def _make_openai_response(content: str) -> dict:
    """Build an OpenAI-style chat completion response body."""
    return {"choices": [{"message": {"role": "assistant", "content": content}}]}


# ---------------------------------------------------------------------------
# TestTopicDetectorInit
# ---------------------------------------------------------------------------


class TestTopicDetectorInit:
    """Tests for TopicDetector initialization and model resolution."""

    @patch.dict("os.environ", {"OPENAI_API_KEY": "sk-test"}, clear=False)
    @patch("swecli.core.agents.components.api.configuration" ".create_http_client_for_provider")
    def test_openai_provider_selects_gpt4o_mini(self, mock_create):
        mock_create.return_value = MagicMock()
        config = AppConfig(model_provider="openai", model="gpt-4o")
        detector = TopicDetector(config)

        assert detector._model_id == "gpt-4o-mini"
        assert detector._provider_id == "openai"
        assert detector._client is not None

    @patch.dict("os.environ", {"ANTHROPIC_API_KEY": "sk-ant-test"}, clear=False)
    @patch("swecli.core.agents.components.api.configuration" ".create_http_client_for_provider")
    def test_anthropic_provider_selects_haiku(self, mock_create):
        mock_create.return_value = MagicMock()
        config = AppConfig(model_provider="anthropic", model="claude-3-opus-20240229")
        detector = TopicDetector(config)

        assert detector._model_id == "claude-3-5-haiku-20241022"
        assert detector._provider_id == "anthropic"

    @patch.dict(
        "os.environ",
        {"OPENAI_API_KEY": "", "ANTHROPIC_API_KEY": "", "FIREWORKS_API_KEY": ""},
        clear=False,
    )
    def test_no_api_key_results_in_unavailable(self):
        config = AppConfig(model_provider="openai", model="gpt-4o", api_key=None)
        # Clear any env keys that might be set in the real environment
        with patch.dict(
            "os.environ",
            {"OPENAI_API_KEY": "", "ANTHROPIC_API_KEY": "", "FIREWORKS_API_KEY": ""},
        ):
            detector = TopicDetector(config)
        assert detector._client is None

    @patch.dict("os.environ", {"OPENAI_API_KEY": "sk-test"}, clear=False)
    @patch("swecli.core.agents.components.api.configuration" ".create_http_client_for_provider")
    def test_disabled_config_is_noop(self, mock_create):
        mock_create.return_value = MagicMock()
        config = AppConfig(model_provider="openai", model="gpt-4o", topic_detection=False)
        detector = TopicDetector(config)

        session_manager = MagicMock()
        detector.detect(session_manager, "abc123", [{"role": "user", "content": "hi"}])
        # Should not start any thread because topic_detection is disabled
        session_manager.set_title.assert_not_called()


# ---------------------------------------------------------------------------
# TestTopicDetection
# ---------------------------------------------------------------------------


class TestTopicDetection:
    """Tests for the actual topic detection logic."""

    def _make_detector(self) -> TopicDetector:
        """Create a detector with a mocked client."""
        with patch.dict("os.environ", {"OPENAI_API_KEY": "sk-test"}, clear=False):
            with patch(
                "swecli.core.agents.components.api.configuration" ".create_http_client_for_provider"
            ) as mock_create:
                mock_create.return_value = MagicMock()
                config = AppConfig(model_provider="openai", model="gpt-4o")
                detector = TopicDetector(config)
        return detector

    def test_new_topic_updates_title(self):
        detector = self._make_detector()
        response_body = _make_openai_response(
            json.dumps({"isNewTopic": True, "title": "Auth Refactor"})
        )
        detector._client.post_json.return_value = FakeHttpResult(
            success=True, response=FakeResponse(200, response_body)
        )

        session_manager = MagicMock()
        # Call synchronously for testing
        messages = [
            {"role": "user", "content": "Let's refactor the auth module"},
        ]
        detector._detect_and_update(session_manager, "sess1", messages)
        session_manager.set_title.assert_called_once_with("sess1", "Auth Refactor")

    def test_not_new_topic_keeps_title(self):
        detector = self._make_detector()
        response_body = _make_openai_response(json.dumps({"isNewTopic": False, "title": None}))
        detector._client.post_json.return_value = FakeHttpResult(
            success=True, response=FakeResponse(200, response_body)
        )

        session_manager = MagicMock()
        messages = [
            {"role": "user", "content": "Can you also add a test?"},
        ]
        detector._detect_and_update(session_manager, "sess1", messages)
        session_manager.set_title.assert_not_called()

    def test_llm_failure_keeps_existing_title(self):
        detector = self._make_detector()
        detector._client.post_json.return_value = FakeHttpResult(
            success=False, error="Connection timeout"
        )

        session_manager = MagicMock()
        messages = [{"role": "user", "content": "hello"}]
        detector._detect_and_update(session_manager, "sess1", messages)
        session_manager.set_title.assert_not_called()

    def test_malformed_json_keeps_title(self):
        detector = self._make_detector()
        response_body = _make_openai_response("not valid json {{{")
        detector._client.post_json.return_value = FakeHttpResult(
            success=True, response=FakeResponse(200, response_body)
        )

        session_manager = MagicMock()
        messages = [{"role": "user", "content": "hello"}]
        detector._detect_and_update(session_manager, "sess1", messages)
        session_manager.set_title.assert_not_called()

    def test_title_truncated_to_50_chars(self):
        detector = self._make_detector()
        long_title = "A" * 80
        response_body = _make_openai_response(json.dumps({"isNewTopic": True, "title": long_title}))
        detector._client.post_json.return_value = FakeHttpResult(
            success=True, response=FakeResponse(200, response_body)
        )

        session_manager = MagicMock()
        messages = [{"role": "user", "content": "hello"}]
        detector._detect_and_update(session_manager, "sess1", messages)
        session_manager.set_title.assert_called_once()
        actual_title = session_manager.set_title.call_args[0][1]
        assert len(actual_title) <= 50


# ---------------------------------------------------------------------------
# TestPromptTemplate
# ---------------------------------------------------------------------------


class TestPromptTemplate:
    """Tests for the prompt template loading and message format."""

    def test_prompt_loads_from_template_file(self):
        from swecli.core.agents.prompts.loader import load_prompt

        prompt = load_prompt("memory/topic_detection_prompt")
        assert "isNewTopic" in prompt
        assert "title" in prompt
        assert "JSON" in prompt

    def test_messages_sent_as_proper_api_format(self):
        """Verify that conversation messages are sent as proper role-based API messages."""
        with patch.dict("os.environ", {"OPENAI_API_KEY": "sk-test"}, clear=False):
            with patch(
                "swecli.core.agents.components.api.configuration" ".create_http_client_for_provider"
            ) as mock_create:
                mock_client = MagicMock()
                mock_create.return_value = mock_client

                # Return a valid but "not new topic" response
                response_body = _make_openai_response(
                    json.dumps({"isNewTopic": False, "title": None})
                )
                mock_client.post_json.return_value = FakeHttpResult(
                    success=True, response=FakeResponse(200, response_body)
                )

                config = AppConfig(model_provider="openai", model="gpt-4o")
                detector = TopicDetector(config)

        messages = [
            {"role": "user", "content": "Fix the login bug"},
            {"role": "assistant", "content": "I'll look into it."},
            {"role": "user", "content": "Now add dark mode"},
        ]
        detector._detect_and_update(MagicMock(), "sess1", messages)

        # Check the payload sent to post_json
        call_args = mock_client.post_json.call_args
        payload = call_args[0][0]

        # First message should be the system prompt
        assert payload["messages"][0]["role"] == "system"
        assert "isNewTopic" in payload["messages"][0]["content"]

        # Next 3 should be the conversation messages with proper roles
        assert payload["messages"][1] == {"role": "user", "content": "Fix the login bug"}
        assert payload["messages"][2] == {
            "role": "assistant",
            "content": "I'll look into it.",
        }
        assert payload["messages"][3] == {"role": "user", "content": "Now add dark mode"}

        # Last message should be the analysis prompt
        assert payload["messages"][4]["role"] == "user"
        assert "Analyze" in payload["messages"][4]["content"]

        # Model should be the cheap one
        assert payload["model"] == "gpt-4o-mini"
