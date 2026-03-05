"""Anthropic API adapter for handling Anthropic-specific request/response formats."""

import logging
import time
from typing import Any, Dict, List, Optional

import httpx

from swecli.core.agents.components.api.base_adapter import ProviderAdapter
from swecli.core.agents.components.api.http_client import (
    MAX_RETRIES,
    RETRYABLE_STATUS_CODES,
    RETRY_DELAYS,
)

logger = logging.getLogger(__name__)


class AnthropicAdapter(ProviderAdapter):
    """Adapter for Anthropic's API which uses a different format than OpenAI."""

    def __init__(self, api_key: str, api_url: str = "https://api.anthropic.com/v1/messages"):
        self.api_key = api_key
        self.api_url = api_url
        self.headers = {
            "Content-Type": "application/json",
            "x-api-key": api_key,
            "anthropic-version": "2023-06-01",
        }
        self._client = httpx.Client(
            headers=self.headers,
            timeout=httpx.Timeout(connect=10.0, read=300.0, write=10.0, pool=10.0),
        )

    @property
    def supports_prompt_caching(self) -> bool:
        """Anthropic supports prompt caching via cache_control blocks."""
        return True

    def build_max_tokens_param(self, model_id: str, max_tokens: int) -> Dict[str, int]:
        """Anthropic always uses ``max_tokens`` (never ``max_completion_tokens``)."""
        return {"max_tokens": max_tokens}

    def convert_request(self, openai_payload: Dict[str, Any]) -> Dict[str, Any]:
        """Convert OpenAI-style payload to Anthropic format.

        Anthropic differences:
        - Requires max_tokens (not optional)
        - tool_choice format is different: {"type": "auto"} instead of "auto"
        - System message must be extracted from messages array
        """
        messages = openai_payload.get("messages", [])

        # Extract system message if present
        system_content = None
        filtered_messages = []
        for msg in messages:
            if msg.get("role") == "system":
                system_content = msg.get("content", "")
            else:
                filtered_messages.append(msg)

        anthropic_payload = {
            "model": openai_payload["model"],
            "max_tokens": openai_payload.get("max_tokens", 4096),
            "messages": filtered_messages,
        }

        if system_content:
            anthropic_payload["system"] = self._build_system_with_cache(
                system_content,
                openai_payload.get("_system_dynamic", ""),
            )

        # Convert temperature if present
        if "temperature" in openai_payload:
            anthropic_payload["temperature"] = openai_payload["temperature"]

        # Convert tools if present
        if "tools" in openai_payload and openai_payload["tools"]:
            anthropic_payload["tools"] = self._convert_tools(openai_payload["tools"])

        # Convert tool_choice if present
        if "tool_choice" in openai_payload:
            tool_choice = openai_payload["tool_choice"]
            if tool_choice == "auto":
                anthropic_payload["tool_choice"] = {"type": "auto"}
            elif tool_choice == "none":
                # Anthropic doesn't have explicit "none", so omit tools instead
                pass
            elif isinstance(tool_choice, dict):
                # Specific tool choice: {"type": "function", "function": {"name": "tool_name"}}
                anthropic_payload["tool_choice"] = {
                    "type": "tool",
                    "name": tool_choice.get("function", {}).get("name"),
                }

        return anthropic_payload

    @staticmethod
    def _build_system_with_cache(stable_content: str, dynamic_content: str = "") -> Any:
        """Build system prompt with Anthropic cache_control for cost savings.

        Structures the system prompt as a 2-element array:
        - Part 1 (stable): Base system prompt, tool descriptions, security policy.
          Gets cache_control: {"type": "ephemeral"} for ~90% discount on cached
          input tokens on subsequent turns.
        - Part 2 (dynamic): Session-specific context, reminders. No caching.

        If there's no dynamic content, uses a single cached block.
        """
        if not stable_content:
            if dynamic_content:
                return dynamic_content
            return ""

        # Build as array of content blocks for Anthropic
        blocks = [
            {
                "type": "text",
                "text": stable_content,
                "cache_control": {"type": "ephemeral"},
            }
        ]

        if dynamic_content:
            blocks.append(
                {
                    "type": "text",
                    "text": dynamic_content,
                }
            )

        return blocks

    def _convert_tools(self, openai_tools: List[Dict[str, Any]]) -> List[Dict[str, Any]]:
        """Convert OpenAI tool format to Anthropic format."""
        anthropic_tools = []
        for tool in openai_tools:
            if tool.get("type") == "function":
                func = tool.get("function", {})
                anthropic_tools.append(
                    {
                        "name": func.get("name", ""),
                        "description": func.get("description", ""),
                        "input_schema": func.get("parameters", {}),
                    }
                )
        return anthropic_tools

    def convert_response(self, anthropic_response: Dict[str, Any]) -> Dict[str, Any]:
        """Convert Anthropic response to OpenAI format.

        Anthropic response structure:
        {
          "id": "msg_...",
          "type": "message",
          "role": "assistant",
          "content": [
            {"type": "text", "text": "..."},
            {"type": "tool_use", "id": "...", "name": "...", "input": {...}}
          ],
          "stop_reason": "end_turn" or "tool_use"
        }

        OpenAI format:
        {
          "choices": [{
            "message": {
              "role": "assistant",
              "content": "...",
              "tool_calls": [...]
            }
          }]
        }
        """
        content_blocks = anthropic_response.get("content", [])

        # Extract text content and tool calls
        text_content = ""
        tool_calls = []

        for block in content_blocks:
            if block.get("type") == "text":
                text_content += block.get("text", "")
            elif block.get("type") == "tool_use":
                tool_calls.append(
                    {
                        "id": block.get("id", ""),
                        "type": "function",
                        "function": {
                            "name": block.get("name", ""),
                            "arguments": self._serialize_arguments(block.get("input", {})),
                        },
                    }
                )

        # Build OpenAI-style response
        message = {
            "role": "assistant",
            "content": text_content or None,
        }

        if tool_calls:
            message["tool_calls"] = tool_calls

        return {
            "id": anthropic_response.get("id", ""),
            "object": "chat.completion",
            "model": anthropic_response.get("model", ""),
            "choices": [
                {
                    "index": 0,
                    "message": message,
                    "finish_reason": self._convert_stop_reason(
                        anthropic_response.get("stop_reason")
                    ),
                }
            ],
            "usage": self._convert_usage(anthropic_response.get("usage", {})),
        }

    def _serialize_arguments(self, args: Dict[str, Any]) -> str:
        """Serialize arguments to JSON string."""
        import json

        return json.dumps(args)

    def _convert_stop_reason(self, anthropic_reason: Optional[str]) -> str:
        """Convert Anthropic stop reason to OpenAI finish_reason."""
        mapping = {
            "end_turn": "stop",
            "tool_use": "tool_calls",
            "max_tokens": "length",
            "stop_sequence": "stop",
        }
        return mapping.get(anthropic_reason or "", "stop")

    def _convert_usage(self, anthropic_usage: Dict[str, Any]) -> Dict[str, int]:
        """Convert Anthropic usage to OpenAI format.

        Includes cache hit/miss tokens if Anthropic returns them.
        """
        result = {
            "prompt_tokens": anthropic_usage.get("input_tokens", 0),
            "completion_tokens": anthropic_usage.get("output_tokens", 0),
            "total_tokens": (
                anthropic_usage.get("input_tokens", 0) + anthropic_usage.get("output_tokens", 0)
            ),
        }
        # Preserve Anthropic cache metrics for cost tracking
        cache_read = anthropic_usage.get("cache_read_input_tokens", 0)
        cache_create = anthropic_usage.get("cache_creation_input_tokens", 0)
        if cache_read or cache_create:
            result["cache_read_input_tokens"] = cache_read
            result["cache_creation_input_tokens"] = cache_create
        return result

    def post_json(self, payload: Dict[str, Any], *, task_monitor: Any = None) -> Any:
        """Make a request to Anthropic API with retry logic.

        Retries on HTTP 429/503 with exponential backoff. Converts the payload
        and response to match OpenAI format for compatibility.
        """
        from dataclasses import dataclass
        from typing import Union
        import json

        @dataclass
        class HttpResult:
            success: bool
            response: Union[httpx.Response, None] = None
            error: Union[str, None] = None
            interrupted: bool = False

        @dataclass
        class MockResponse:
            """Mock response object that mimics httpx.Response for compatibility."""

            status_code: int
            _json_data: Dict[str, Any]
            text: str
            headers: Dict[str, str]

            def json(self) -> Dict[str, Any]:
                return self._json_data

        anthropic_payload = self.convert_request(payload)
        last_result: Any = None

        for attempt in range(MAX_RETRIES + 1):
            try:
                response = self._client.post(self.api_url, json=anthropic_payload)

                if response.status_code in RETRYABLE_STATUS_CODES:
                    last_result = HttpResult(success=True, response=response)
                    if attempt < MAX_RETRIES:
                        retry_after = response.headers.get("Retry-After")
                        if retry_after is not None:
                            try:
                                delay = max(0.0, float(retry_after))
                            except ValueError:
                                delay = RETRY_DELAYS[min(attempt, len(RETRY_DELAYS) - 1)]
                        else:
                            delay = RETRY_DELAYS[min(attempt, len(RETRY_DELAYS) - 1)]
                        logger.warning(
                            "Anthropic HTTP %d — retrying in %.1fs (attempt %d/%d)",
                            response.status_code,
                            delay,
                            attempt + 1,
                            MAX_RETRIES,
                        )
                        time.sleep(delay)
                        continue
                    return last_result

                if response.status_code != 200:
                    return HttpResult(success=True, response=response)

                anthropic_data = response.json()
                openai_data = self.convert_response(anthropic_data)

                mock_response = MockResponse(
                    status_code=200,
                    _json_data=openai_data,
                    text=json.dumps(openai_data),
                    headers=dict(response.headers),
                )

                return HttpResult(success=True, response=mock_response)

            except Exception as exc:
                return HttpResult(success=False, error=str(exc))

        return last_result or HttpResult(success=False, error="Unexpected retry exhaustion")
