"""Unit tests for VLM auto-routing and image format conversion."""

import json
import pytest
from unittest.mock import MagicMock, PropertyMock, patch

from swecli.core.agents.components.api.http_client import AgentHttpClient
from swecli.core.agents.components.api.openai_responses_adapter import OpenAIResponsesAdapter
from swecli.core.agents.main_agent import MainAgent


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

def _make_text_message(role: str = "user", text: str = "hello") -> dict:
    return {"role": role, "content": text}


def _make_image_message(role: str = "user") -> dict:
    return {
        "role": role,
        "content": [
            {"type": "text", "text": "Describe this image"},
            {
                "type": "image",
                "source": {
                    "type": "base64",
                    "media_type": "image/png",
                    "data": "iVBOR...",
                },
            },
        ],
    }


def _make_multimodal_text_only(role: str = "user") -> dict:
    """Multimodal content list but no image blocks."""
    return {
        "role": role,
        "content": [
            {"type": "text", "text": "Just text"},
            {"type": "text", "text": "More text"},
        ],
    }


def _build_agent(
    model: str = "gpt-4o",
    model_provider: str = "openai",
    model_vlm: str | None = None,
    model_vlm_provider: str | None = None,
    vlm_info: tuple | None = None,
) -> MainAgent:
    """Create a MainAgent with mocked dependencies.

    Args:
        vlm_info: Return value for config.get_vlm_model_info(). If None and
                  model_vlm is set, a default tuple is constructed.
    """
    config = MagicMock()
    config.model = model
    config.model_provider = model_provider
    config.model_vlm = model_vlm
    config.model_vlm_provider = model_vlm_provider
    config.model_thinking_provider = None
    config.model_thinking = None
    config.model_critique = None
    config.model_critique_provider = None
    config.temperature = 0.0
    config.max_tokens = 4096

    if vlm_info is not None:
        config.get_vlm_model_info.return_value = vlm_info
    elif model_vlm and model_vlm_provider:
        mock_model_info = MagicMock()
        mock_model_info.capabilities = ["vision"]
        config.get_vlm_model_info.return_value = (model_vlm_provider, model_vlm, mock_model_info)
    else:
        config.get_vlm_model_info.return_value = None

    tool_registry = MagicMock()
    tool_registry.get_all_tool_schemas.return_value = []
    tool_registry.get_tool_schemas.return_value = []

    mode_manager = MagicMock()

    with (
        patch.object(MainAgent, "build_system_prompt", return_value="sys"),
        patch.object(MainAgent, "build_tool_schemas", return_value=[]),
    ):
        agent = MainAgent(config, tool_registry, mode_manager)

    return agent


# ===========================================================================
# TestMessagesContainImages
# ===========================================================================

class TestMessagesContainImages:
    """Tests for MainAgent._messages_contain_images()."""

    def test_text_only_returns_false(self):
        msgs = [_make_text_message(), _make_text_message("assistant", "hi")]
        assert MainAgent._messages_contain_images(msgs) is False

    def test_image_message_returns_true(self):
        msgs = [_make_text_message(), _make_image_message()]
        assert MainAgent._messages_contain_images(msgs) is True

    def test_multimodal_text_only_returns_false(self):
        msgs = [_make_multimodal_text_only()]
        assert MainAgent._messages_contain_images(msgs) is False

    def test_image_in_earlier_message_returns_true(self):
        msgs = [_make_image_message(), _make_text_message("assistant", "I see a cat")]
        assert MainAgent._messages_contain_images(msgs) is True

    def test_empty_messages_returns_false(self):
        assert MainAgent._messages_contain_images([]) is False

    def test_system_message_ignored(self):
        msgs = [{"role": "system", "content": "You are helpful"}]
        assert MainAgent._messages_contain_images(msgs) is False

    def test_multiple_images_returns_true(self):
        msgs = [_make_image_message(), _make_image_message()]
        assert MainAgent._messages_contain_images(msgs) is True


# ===========================================================================
# TestResolveVlmModelAndClient
# ===========================================================================

class TestResolveVlmModelAndClient:
    """Tests for MainAgent._resolve_vlm_model_and_client()."""

    def test_no_images_returns_normal(self):
        agent = _build_agent(model_vlm="gpt-4o", model_vlm_provider="openai")
        msgs = [_make_text_message()]
        model_id, client = agent._resolve_vlm_model_and_client(msgs)
        assert model_id == "gpt-4o"
        # get_vlm_model_info should NOT be called when no images
        agent.config.get_vlm_model_info.assert_not_called()

    def test_images_with_vlm_configured_returns_vlm(self):
        agent = _build_agent(
            model="gpt-4o-mini",
            model_provider="openai",
            model_vlm="gpt-4o",
            model_vlm_provider="openai",
        )
        msgs = [_make_image_message()]
        model_id, _ = agent._resolve_vlm_model_and_client(msgs)
        assert model_id == "gpt-4o"

    def test_images_no_vlm_returns_normal(self):
        agent = _build_agent(model="gpt-4o-mini", model_provider="openai")
        msgs = [_make_image_message()]
        model_id, _ = agent._resolve_vlm_model_and_client(msgs)
        assert model_id == "gpt-4o-mini"

    def test_vlm_same_provider_reuses_normal_client(self):
        agent = _build_agent(
            model="gpt-4o-mini",
            model_provider="openai",
            model_vlm="gpt-4o",
            model_vlm_provider="openai",
        )
        msgs = [_make_image_message()]
        _, client = agent._resolve_vlm_model_and_client(msgs)
        # Same provider -> should use the normal _http_client
        assert client is agent._http_client

    def test_vlm_different_provider_uses_vlm_client(self):
        mock_vlm_client = MagicMock()
        agent = _build_agent(
            model="gpt-4o-mini",
            model_provider="openai",
            model_vlm="gemini-2.0-flash",
            model_vlm_provider="google",
        )
        # Inject a pre-built VLM client
        agent._MainAgent__vlm_http_client = mock_vlm_client
        msgs = [_make_image_message()]
        _, client = agent._resolve_vlm_model_and_client(msgs)
        assert client is mock_vlm_client

    def test_vlm_different_provider_fallback_on_none(self):
        """If __vlm_http_client stays None (e.g. create failed), falls back to _http_client."""
        agent = _build_agent(
            model="gpt-4o-mini",
            model_provider="openai",
            model_vlm="gemini-2.0-flash",
            model_vlm_provider="google",
        )
        # Patch create_http_client_for_provider to raise ValueError
        with patch(
            "swecli.core.agents.main_agent.create_http_client_for_provider",
            side_effect=ValueError("No API key"),
        ):
            msgs = [_make_image_message()]
            _, client = agent._resolve_vlm_model_and_client(msgs)
            # Should fall back to _http_client
            assert client is agent._http_client


# ===========================================================================
# TestCallLlmVlmRouting
# ===========================================================================

class TestCallLlmVlmRouting:
    """Integration tests verifying call_llm uses VLM model when images present."""

    def _make_success_response(self, model_id: str = "gpt-4o"):
        """Build a mock HTTP response for a successful LLM call."""
        resp = MagicMock()
        resp.status_code = 200
        resp.json.return_value = {
            "choices": [
                {
                    "message": {
                        "content": "I see an image",
                        "role": "assistant",
                    }
                }
            ],
            "usage": {"total_tokens": 100},
        }
        return resp

    def test_call_llm_with_images_uses_vlm_model(self):
        agent = _build_agent(
            model="gpt-4o-mini",
            model_provider="openai",
            model_vlm="gpt-4o",
            model_vlm_provider="openai",
        )
        mock_client = MagicMock()
        result_obj = MagicMock()
        result_obj.success = True
        result_obj.response = self._make_success_response()
        result_obj.interrupted = False
        mock_client.post_json.return_value = result_obj
        agent._MainAgent__http_client = mock_client

        msgs = [
            {"role": "system", "content": "sys"},
            _make_image_message(),
        ]
        result = agent.call_llm(msgs)

        assert result["success"] is True
        # Check the payload sent to post_json used the VLM model
        call_args = mock_client.post_json.call_args
        payload = call_args[0][0] if call_args[0] else call_args[1].get("payload")
        assert payload["model"] == "gpt-4o"

    def test_call_llm_with_text_uses_normal_model(self):
        agent = _build_agent(
            model="gpt-4o-mini",
            model_provider="openai",
            model_vlm="gpt-4o",
            model_vlm_provider="openai",
        )
        mock_client = MagicMock()
        result_obj = MagicMock()
        result_obj.success = True
        result_obj.response = self._make_success_response()
        result_obj.interrupted = False
        mock_client.post_json.return_value = result_obj
        agent._MainAgent__http_client = mock_client

        msgs = [
            {"role": "system", "content": "sys"},
            _make_text_message(),
        ]
        result = agent.call_llm(msgs)

        assert result["success"] is True
        call_args = mock_client.post_json.call_args
        payload = call_args[0][0] if call_args[0] else call_args[1].get("payload")
        assert payload["model"] == "gpt-4o-mini"


# ===========================================================================
# TestRunSyncVlmRouting
# ===========================================================================

class TestRunSyncVlmRouting:
    """Tests verifying run_sync uses VLM model routing in its payload."""

    def test_run_sync_with_image_history_uses_vlm(self):
        agent = _build_agent(
            model="gpt-4o-mini",
            model_provider="openai",
            model_vlm="gpt-4o",
            model_vlm_provider="openai",
        )

        mock_client = MagicMock()
        resp = MagicMock()
        resp.status_code = 200
        resp.json.return_value = {
            "choices": [
                {
                    "message": {
                        "content": "Done",
                        "role": "assistant",
                    }
                }
            ],
            "usage": {"total_tokens": 50},
        }
        result_obj = MagicMock()
        result_obj.success = True
        result_obj.response = resp
        result_obj.interrupted = False
        mock_client.post_json.return_value = result_obj
        agent._MainAgent__http_client = mock_client

        # Stub _maybe_compact to passthrough
        agent._compactor = MagicMock()
        agent._compactor.should_compact.return_value = False

        deps = MagicMock()
        history = [
            {"role": "system", "content": "sys"},
            _make_image_message(),
        ]

        result = agent.run_sync("describe this", deps, message_history=history)

        assert result["success"] is True
        call_args = mock_client.post_json.call_args
        payload = call_args[0][0] if call_args[0] else call_args[1].get("payload")
        assert payload["model"] == "gpt-4o"


# ===========================================================================
# TestResponsesAdapterContentBlocks
# ===========================================================================


class TestResponsesAdapterContentBlocks:
    """Tests for OpenAIResponsesAdapter._convert_content_blocks()."""

    def test_string_content_passthrough(self):
        result = OpenAIResponsesAdapter._convert_content_blocks("hello")
        assert result == "hello"

    def test_text_block_converted_to_input_text(self):
        blocks = [{"type": "text", "text": "describe this"}]
        result = OpenAIResponsesAdapter._convert_content_blocks(blocks)
        assert result == [{"type": "input_text", "text": "describe this"}]

    def test_image_block_converted_to_input_image(self):
        blocks = [
            {
                "type": "image",
                "source": {
                    "type": "base64",
                    "media_type": "image/png",
                    "data": "iVBOR...",
                },
            }
        ]
        result = OpenAIResponsesAdapter._convert_content_blocks(blocks)
        assert len(result) == 1
        assert result[0]["type"] == "input_image"
        assert result[0]["image_url"] == "data:image/png;base64,iVBOR..."

    def test_mixed_content_converted(self):
        blocks = [
            {"type": "text", "text": "What is in this image?"},
            {
                "type": "image",
                "source": {
                    "type": "base64",
                    "media_type": "image/jpeg",
                    "data": "ABCD1234",
                },
            },
        ]
        result = OpenAIResponsesAdapter._convert_content_blocks(blocks)
        assert len(result) == 2
        assert result[0] == {"type": "input_text", "text": "What is in this image?"}
        assert result[1]["type"] == "input_image"
        assert result[1]["image_url"] == "data:image/jpeg;base64,ABCD1234"

    def test_unknown_block_type_passthrough(self):
        blocks = [{"type": "audio", "data": "..."}]
        result = OpenAIResponsesAdapter._convert_content_blocks(blocks)
        assert result == blocks

    def test_convert_request_with_image_message(self):
        adapter = OpenAIResponsesAdapter(api_key="test-key")
        payload = {
            "model": "gpt-4o",
            "messages": [
                {"role": "system", "content": "You are helpful."},
                {
                    "role": "user",
                    "content": [
                        {"type": "text", "text": "Describe this"},
                        {
                            "type": "image",
                            "source": {
                                "type": "base64",
                                "media_type": "image/png",
                                "data": "iVBOR...",
                            },
                        },
                    ],
                },
            ],
        }
        result = adapter.convert_request(payload)
        user_item = [i for i in result["input"] if i.get("role") == "user"][0]
        content = user_item["content"]
        assert isinstance(content, list)
        assert content[0]["type"] == "input_text"
        assert content[1]["type"] == "input_image"


# ===========================================================================
# TestHttpClientNormalizeImageBlocks
# ===========================================================================


class TestHttpClientNormalizeImageBlocks:
    """Tests for AgentHttpClient._normalize_image_blocks()."""

    def test_text_only_payload_unchanged(self):
        payload = {
            "model": "gpt-4o",
            "messages": [{"role": "user", "content": "hello"}],
        }
        result = AgentHttpClient._normalize_image_blocks(payload)
        assert result is payload  # Same object, no copy needed

    def test_no_messages_passthrough(self):
        payload = {"model": "gpt-4o"}
        result = AgentHttpClient._normalize_image_blocks(payload)
        assert result is payload

    def test_image_block_converted_to_image_url(self):
        payload = {
            "model": "gpt-4o",
            "messages": [
                {
                    "role": "user",
                    "content": [
                        {"type": "text", "text": "What is this?"},
                        {
                            "type": "image",
                            "source": {
                                "type": "base64",
                                "media_type": "image/png",
                                "data": "iVBOR...",
                            },
                        },
                    ],
                }
            ],
        }
        result = AgentHttpClient._normalize_image_blocks(payload)
        # Should be a new payload, not mutating original
        assert result is not payload
        content = result["messages"][0]["content"]
        assert content[0] == {"type": "text", "text": "What is this?"}
        assert content[1]["type"] == "image_url"
        assert content[1]["image_url"]["url"] == "data:image/png;base64,iVBOR..."

    def test_original_payload_not_mutated(self):
        original_block = {
            "type": "image",
            "source": {
                "type": "base64",
                "media_type": "image/png",
                "data": "iVBOR...",
            },
        }
        payload = {
            "model": "gpt-4o",
            "messages": [
                {"role": "user", "content": [original_block]},
            ],
        }
        AgentHttpClient._normalize_image_blocks(payload)
        # Original should still have the Anthropic format
        assert payload["messages"][0]["content"][0]["type"] == "image"

    def test_list_content_without_images_unchanged(self):
        payload = {
            "model": "gpt-4o",
            "messages": [
                {
                    "role": "user",
                    "content": [
                        {"type": "text", "text": "hello"},
                        {"type": "text", "text": "world"},
                    ],
                }
            ],
        }
        result = AgentHttpClient._normalize_image_blocks(payload)
        # No image blocks, so should return original payload
        assert result is payload


# ===========================================================================
# TestMultimodalContentExtraction
# ===========================================================================


class TestMultimodalContentExtraction:
    """Tests for multimodal content extraction in short-term memory/context."""

    def test_extract_text_from_multimodal_list(self):
        """Verify that text extraction from multimodal lists works correctly."""
        content = [
            {"type": "text", "text": "Describe this image"},
            {
                "type": "image",
                "source": {
                    "type": "base64",
                    "media_type": "image/png",
                    "data": "iVBOR" + "A" * 10000,  # Simulates large base64
                },
            },
        ]
        # Simulate the extraction logic used in both react_executor and agent_executor
        extracted = "\n".join(
            block.get("text", "")
            for block in content
            if isinstance(block, dict) and block.get("type") == "text"
        )
        assert extracted == "Describe this image"
        assert "iVBOR" not in extracted

    def test_extract_multiple_text_blocks(self):
        content = [
            {"type": "text", "text": "First part"},
            {"type": "text", "text": "Second part"},
        ]
        extracted = "\n".join(
            block.get("text", "")
            for block in content
            if isinstance(block, dict) and block.get("type") == "text"
        )
        assert extracted == "First part\nSecond part"

    def test_extract_from_string_content(self):
        """String content should not be affected by the list extraction logic."""
        content = "Just a plain string"
        if isinstance(content, list):
            extracted = "\n".join(
                block.get("text", "")
                for block in content
                if isinstance(block, dict) and block.get("type") == "text"
            )
        else:
            extracted = content
        assert extracted == "Just a plain string"
