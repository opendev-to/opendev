"""API integration layer for OpenDev agents.

This subpackage contains HTTP client wrappers, API configuration utilities,
and provider-specific adapters (Anthropic, OpenAI, Fireworks).
"""

from .anthropic_adapter import AnthropicAdapter
from .base_adapter import ProviderAdapter
from .openai_responses_adapter import OpenAIResponsesAdapter
from .configuration import (
    build_max_tokens_param,
    build_temperature_param,
    create_http_client,
    create_http_client_for_provider,
    resolve_api_config,
)
from .http_client import AgentHttpClient, HttpResult

# Register AgentHttpClient as a virtual subclass of ProviderAdapter.
# AgentHttpClient can't inherit directly because ProviderAdapter imports
# HttpResult from the same module (circular import).
ProviderAdapter.register(AgentHttpClient)

__all__ = [
    "AgentHttpClient",
    "AnthropicAdapter",
    "HttpResult",
    "OpenAIResponsesAdapter",
    "ProviderAdapter",
    "build_max_tokens_param",
    "build_temperature_param",
    "create_http_client",
    "create_http_client_for_provider",
    "resolve_api_config",
]
