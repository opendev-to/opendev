"""Abstract base class for provider adapters.

All provider-specific HTTP clients (Anthropic, OpenAI Responses, generic
OpenAI-compatible) inherit from this ABC so that the agent layer can
program against a single interface.
"""

from __future__ import annotations

from abc import ABC, abstractmethod
from typing import Any, Dict

from swecli.core.agents.components.api.http_client import HttpResult


class ProviderAdapter(ABC):
    """Unified interface for LLM provider HTTP clients.

    Concrete subclasses must implement:
    - post_json: send a request and return an HttpResult
    - convert_request: translate the internal payload to the provider format
    - convert_response: translate the provider response back to OpenAI format
    """

    @abstractmethod
    def post_json(self, payload: Dict[str, Any], *, task_monitor: Any = None) -> HttpResult:
        """Send a JSON payload to the provider and return an HttpResult."""
        ...

    @abstractmethod
    def convert_request(self, payload: Dict[str, Any]) -> Dict[str, Any]:
        """Convert an internal (OpenAI Chat Completions) payload to provider format."""
        ...

    @abstractmethod
    def convert_response(self, response_data: Dict[str, Any]) -> Dict[str, Any]:
        """Convert a provider response back to OpenAI Chat Completions format."""
        ...

    # ------------------------------------------------------------------
    # Default implementations (overridable by concrete adapters)
    # ------------------------------------------------------------------

    @property
    def supports_prompt_caching(self) -> bool:
        """Whether this provider supports prompt caching (e.g. Anthropic)."""
        return False

    def build_temperature_param(self, model_id: str, temperature: float) -> Dict[str, float]:
        """Build the temperature parameter dict for *model_id*.

        Default delegates to the standalone function in ``configuration``.
        """
        from swecli.core.agents.components.api.configuration import (
            build_temperature_param,
        )

        return build_temperature_param(model_id, temperature)

    def build_max_tokens_param(self, model_id: str, max_tokens: int) -> Dict[str, int]:
        """Build the max-tokens parameter dict for *model_id*.

        Default delegates to the standalone function in ``configuration``.
        """
        from swecli.core.agents.components.api.configuration import (
            build_max_tokens_param,
        )

        return build_max_tokens_param(model_id, max_tokens)
