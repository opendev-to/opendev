"""Helpers for resolving API endpoints and headers."""

from __future__ import annotations

from typing import Tuple, TYPE_CHECKING

from swecli.models.config import AppConfig

if TYPE_CHECKING:
    from swecli.core.agents.components.api.base_adapter import ProviderAdapter


# Models that require max_completion_tokens instead of max_tokens
_MAX_COMPLETION_TOKENS_PREFIXES = ("o1", "o3", "o4", "gpt-5")


def uses_max_completion_tokens(model: str) -> bool:
    """Check if a model requires max_completion_tokens instead of max_tokens.

    GPT-5 and O-series models (o1, o3, o4) use max_completion_tokens parameter
    instead of max_tokens for the OpenAI API.

    Args:
        model: The model ID string

    Returns:
        True if the model uses max_completion_tokens
    """
    return model.startswith(_MAX_COMPLETION_TOKENS_PREFIXES)


def build_max_tokens_param(model: str, max_tokens: int) -> dict[str, int]:
    """Build the appropriate max tokens parameter for a model.

    Args:
        model: The model ID string
        max_tokens: The max tokens value

    Returns:
        Dict with either {"max_completion_tokens": value} or {"max_tokens": value}
    """
    if uses_max_completion_tokens(model):
        return {"max_completion_tokens": max_tokens}
    return {"max_tokens": max_tokens}


_NO_TEMPERATURE_PATTERNS = ("o1", "o3", "o4", "codex")


def _is_reasoning_model(model_id: str) -> bool:
    """Check if model ID matches known reasoning model patterns."""
    lower = model_id.lower()
    for pattern in _NO_TEMPERATURE_PATTERNS:
        if lower == pattern or lower.startswith(f"{pattern}-") or f"/{pattern}" in lower:
            return True
    if "codex" in lower:
        return True
    return False


def build_temperature_param(model_id: str, temperature: float) -> dict[str, float]:
    """Build temperature parameter if the model supports it.

    Checks model registry for supports_temperature field.
    Falls back to name-based detection for known reasoning models (o1, o3, o4, codex).

    Args:
        model_id: The model ID string
        temperature: The temperature value

    Returns:
        Dict with {"temperature": value} or empty dict for models that don't support it
    """
    if _is_reasoning_model(model_id):
        return {}

    from swecli.config.models import get_model_registry

    registry = get_model_registry()
    result = registry.find_model_by_id(model_id)
    if result:
        _, _, model_info = result
        if not model_info.supports_temperature:
            return {}
    return {"temperature": temperature}


def resolve_api_config(config: AppConfig) -> Tuple[str, dict[str, str]]:
    """Return the API URL and headers according to the configured provider.

    Note: This is used for OpenAI-compatible providers (Fireworks, OpenAI).
    Anthropic uses a different client (AnthropicAdapter).
    """
    api_key = config.get_api_key()
    headers = {
        "Content-Type": "application/json",
        "Authorization": f"Bearer {api_key}",
    }

    if config.model_provider == "fireworks":
        api_url = "https://api.fireworks.ai/inference/v1/chat/completions"
    elif config.model_provider == "openai":
        api_url = "https://api.openai.com/v1/chat/completions"
    elif config.model_provider == "anthropic":
        # Anthropic will use AnthropicAdapter, but provide URL for reference
        api_url = "https://api.anthropic.com/v1/messages"
    else:
        api_url = f"{config.api_base_url}/chat/completions"

    return api_url, headers


def create_http_client(config: AppConfig) -> "ProviderAdapter":
    """Create the appropriate HTTP client based on the provider.

    Returns:
        OpenAIResponsesAdapter for OpenAI (all models use /v1/responses)
        AnthropicAdapter for Anthropic
        AgentHttpClient for other OpenAI-compatible APIs (Fireworks, etc.)
    """
    if config.model_provider == "anthropic":
        from .anthropic_adapter import AnthropicAdapter

        api_key = config.get_api_key()
        return AnthropicAdapter(api_key)

    if config.model_provider == "openai":
        from .openai_responses_adapter import OpenAIResponsesAdapter

        return OpenAIResponsesAdapter(config.get_api_key())

    from .http_client import AgentHttpClient

    api_url, headers = resolve_api_config(config)
    return AgentHttpClient(api_url, headers)


def create_http_client_for_provider(provider_id: str, config: AppConfig) -> "ProviderAdapter":
    """Create HTTP client for a specific provider (for Thinking model slot).

    This allows using a different provider for the Thinking model than the Normal model.
    For example, Normal could use Fireworks while Thinking uses OpenAI o1.

    Args:
        provider_id: Provider ID ("openai", "anthropic", "fireworks")
        config: AppConfig for getting API keys

    Returns:
        OpenAIResponsesAdapter for OpenAI (all models use /v1/responses)
        AnthropicAdapter for Anthropic
        AgentHttpClient for other OpenAI-compatible APIs (Fireworks, etc.)

    Raises:
        ValueError: If provider is unknown or API key is missing
    """
    import os

    if provider_id == "openai":
        api_key = os.getenv("OPENAI_API_KEY")
        if not api_key:
            raise ValueError("OPENAI_API_KEY environment variable not set")
        from .openai_responses_adapter import OpenAIResponsesAdapter

        return OpenAIResponsesAdapter(api_key)
    elif provider_id == "anthropic":
        api_key = os.getenv("ANTHROPIC_API_KEY")
        if not api_key:
            raise ValueError("ANTHROPIC_API_KEY environment variable not set")
        from .anthropic_adapter import AnthropicAdapter

        return AnthropicAdapter(api_key)
    elif provider_id == "fireworks":
        api_key = os.getenv("FIREWORKS_API_KEY")
        api_url = "https://api.fireworks.ai/inference/v1/chat/completions"
    else:
        raise ValueError(f"Unknown provider: {provider_id}")

    if not api_key:
        raise ValueError(f"{provider_id.upper()}_API_KEY environment variable not set")

    headers = {
        "Content-Type": "application/json",
        "Authorization": f"Bearer {api_key}",
    }

    from .http_client import AgentHttpClient

    return AgentHttpClient(api_url, headers)
