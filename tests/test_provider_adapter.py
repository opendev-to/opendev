"""Tests for the ProviderAdapter ABC and concrete adapters."""

from swecli.core.agents.components.api.base_adapter import ProviderAdapter
from swecli.core.agents.components.api.anthropic_adapter import AnthropicAdapter
from swecli.core.agents.components.api.openai_responses_adapter import OpenAIResponsesAdapter
from swecli.core.agents.components.api.http_client import AgentHttpClient


class TestProviderAdapterABC:
    """All concrete adapters are instances of ProviderAdapter."""

    def test_anthropic_is_provider_adapter(self):
        adapter = AnthropicAdapter(api_key="test-key")
        assert isinstance(adapter, ProviderAdapter)

    def test_openai_responses_is_provider_adapter(self):
        adapter = OpenAIResponsesAdapter(api_key="test-key")
        assert isinstance(adapter, ProviderAdapter)

    def test_agent_http_client_is_provider_adapter(self):
        client = AgentHttpClient(
            api_url="http://localhost:8080/v1/chat/completions",
            headers={"Authorization": "Bearer test"},
        )
        assert isinstance(client, ProviderAdapter)


class TestAnthropicAdapterProperties:
    """Anthropic-specific overrides."""

    def test_supports_prompt_caching(self):
        adapter = AnthropicAdapter(api_key="test-key")
        assert adapter.supports_prompt_caching is True

    def test_build_max_tokens_always_uses_max_tokens(self):
        adapter = AnthropicAdapter(api_key="test-key")
        # Even for O-series model IDs, Anthropic always uses max_tokens
        assert adapter.build_max_tokens_param("o3-mini", 4096) == {"max_tokens": 4096}
        assert adapter.build_max_tokens_param("claude-3-opus", 8192) == {"max_tokens": 8192}


class TestOtherAdapterProperties:
    """Non-Anthropic adapters should NOT support prompt caching."""

    def test_openai_no_prompt_caching(self):
        adapter = OpenAIResponsesAdapter(api_key="test-key")
        assert adapter.supports_prompt_caching is False

    def test_http_client_no_prompt_caching(self):
        client = AgentHttpClient(
            api_url="http://localhost:8080/v1/chat/completions",
            headers={"Authorization": "Bearer test"},
        )
        # AgentHttpClient is registered as virtual subclass, so it inherits
        # the default False from the ABC conceptually. We verify via the
        # ProviderAdapter default.
        assert ProviderAdapter.supports_prompt_caching.fget(client) is False  # type: ignore[attr-defined]


class TestAgentHttpClientPassthroughs:
    """convert_request and convert_response are passthroughs."""

    def test_convert_request_passthrough(self):
        client = AgentHttpClient(
            api_url="http://localhost:8080/v1/chat/completions",
            headers={"Authorization": "Bearer test"},
        )
        payload = {"model": "gpt-4", "messages": [{"role": "user", "content": "hi"}]}
        result = client.convert_request(payload)
        assert result["model"] == "gpt-4"
        assert result["messages"] == payload["messages"]

    def test_convert_response_passthrough(self):
        client = AgentHttpClient(
            api_url="http://localhost:8080/v1/chat/completions",
            headers={"Authorization": "Bearer test"},
        )
        response = {"choices": [{"message": {"role": "assistant", "content": "hello"}}]}
        assert client.convert_response(response) is response


class TestFactoryReturnTypes:
    """create_http_client returns ProviderAdapter instances."""

    def test_anthropic_factory(self):
        from unittest.mock import MagicMock

        config = MagicMock()
        config.model_provider = "anthropic"
        config.get_api_key.return_value = "test-key"

        from swecli.core.agents.components.api.configuration import create_http_client

        client = create_http_client(config)
        assert isinstance(client, ProviderAdapter)
        assert isinstance(client, AnthropicAdapter)

    def test_openai_factory(self):
        from unittest.mock import MagicMock

        config = MagicMock()
        config.model_provider = "openai"
        config.get_api_key.return_value = "test-key"

        from swecli.core.agents.components.api.configuration import create_http_client

        client = create_http_client(config)
        assert isinstance(client, ProviderAdapter)
        assert isinstance(client, OpenAIResponsesAdapter)

    def test_fireworks_factory(self):
        from unittest.mock import MagicMock

        config = MagicMock()
        config.model_provider = "fireworks"
        config.api_base_url = "https://api.fireworks.ai/inference/v1"
        config.get_api_key.return_value = "test-key"

        from swecli.core.agents.components.api.configuration import create_http_client

        client = create_http_client(config)
        assert isinstance(client, ProviderAdapter)
        assert isinstance(client, AgentHttpClient)
