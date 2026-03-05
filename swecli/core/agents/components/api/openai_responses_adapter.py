"""OpenAI Responses API adapter — used for ALL OpenAI models.

The Responses API (/v1/responses) is OpenAI's recommended replacement
for Chat Completions.  This adapter transparently converts the internal
Chat Completions-shaped payload to the Responses API format and converts
responses back so the rest of the agent code is unaffected.

See: https://platform.openai.com/docs/guides/migrate-to-responses
"""

import json
import logging
import time
from typing import Any, Dict, List, Optional, Union

import httpx

from swecli.core.agents.components.api.base_adapter import ProviderAdapter
from swecli.core.agents.components.api.http_client import (
    MAX_RETRIES,
    RETRYABLE_STATUS_CODES,
    RETRY_DELAYS,
)

logger = logging.getLogger(__name__)


class OpenAIResponsesAdapter(ProviderAdapter):
    """Adapter that translates Chat Completions payloads to the Responses API."""

    def __init__(
        self,
        api_key: str,
        api_url: str = "https://api.openai.com/v1/responses",
    ):
        self.api_key = api_key
        self.api_url = api_url
        self.headers = {
            "Content-Type": "application/json",
            "Authorization": f"Bearer {api_key}",
        }
        self._client = httpx.Client(
            headers=self.headers,
            timeout=httpx.Timeout(connect=10.0, read=300.0, write=10.0, pool=10.0),
        )

    # ------------------------------------------------------------------
    # Request conversion: Chat Completions -> Responses API
    # ------------------------------------------------------------------

    def convert_request(self, openai_payload: Dict[str, Any]) -> Dict[str, Any]:
        """Convert an OpenAI Chat Completions payload to Responses API format.

        Key mappings:
        - system message -> ``instructions``
        - user/assistant/tool messages -> ``input`` items
        - ``max_tokens`` / ``max_completion_tokens`` -> ``max_output_tokens``
        - ``tools`` flattened from ``{type: function, function: {...}}``
          to ``{type: function, name: ..., ...}``
        """
        messages = openai_payload.get("messages", [])

        instructions: Optional[str] = None
        input_items: List[Dict[str, Any]] = []

        for msg in messages:
            role = msg.get("role")
            if role == "system":
                instructions = msg.get("content", "")
            elif role == "user":
                content = msg.get("content", "")
                input_items.append(
                    {
                        "type": "message",
                        "role": "user",
                        "content": self._convert_content_blocks(content),
                    }
                )
            elif role == "assistant":
                # Text content -> message item
                content = msg.get("content")
                if content:
                    input_items.append({"type": "message", "role": "assistant", "content": content})
                # Tool calls -> function_call items
                for tc in msg.get("tool_calls", []):
                    func = tc.get("function", {})
                    input_items.append(
                        {
                            "type": "function_call",
                            "call_id": tc.get("id", ""),
                            "name": func.get("name", ""),
                            "arguments": func.get("arguments", "{}"),
                        }
                    )
            elif role == "tool":
                input_items.append(
                    {
                        "type": "function_call_output",
                        "call_id": msg.get("tool_call_id", ""),
                        "output": msg.get("content", ""),
                    }
                )

        responses_payload: Dict[str, Any] = {
            "model": openai_payload["model"],
            "input": input_items,
            "store": False,  # We manage conversation state ourselves
        }

        if instructions:
            responses_payload["instructions"] = instructions

        # max_tokens / max_completion_tokens -> max_output_tokens
        max_tok = openai_payload.get("max_completion_tokens") or openai_payload.get("max_tokens")
        if max_tok:
            responses_payload["max_output_tokens"] = max_tok

        # Temperature (only if model supports it – caller already filters)
        if "temperature" in openai_payload:
            responses_payload["temperature"] = openai_payload["temperature"]

        # Tools
        if openai_payload.get("tools"):
            responses_payload["tools"] = self._convert_tools(openai_payload["tools"])

        return responses_payload

    def _convert_tools(self, chat_tools: List[Dict[str, Any]]) -> List[Dict[str, Any]]:
        """Flatten Chat Completions tool defs to Responses API format."""
        result = []
        for tool in chat_tools:
            if tool.get("type") == "function":
                func = tool.get("function", {})
                result.append(
                    {
                        "type": "function",
                        "name": func.get("name", ""),
                        "description": func.get("description", ""),
                        "parameters": func.get("parameters", {}),
                    }
                )
        return result

    @staticmethod
    def _convert_content_blocks(
        content: Union[str, List[Dict[str, Any]]],
    ) -> Union[str, List[Dict[str, Any]]]:
        """Convert internal (Anthropic-format) content blocks to Responses API format.

        - ``{"type": "text", ...}`` → ``{"type": "input_text", ...}``
        - ``{"type": "image", "source": {"type": "base64", ...}}``
          → ``{"type": "input_image", "image_url": "data:<media_type>;base64,<data>"}``

        If *content* is a plain string it is returned unchanged.
        """
        if isinstance(content, str):
            return content

        converted: List[Dict[str, Any]] = []
        for block in content:
            if not isinstance(block, dict):
                converted.append(block)
                continue

            block_type = block.get("type")
            if block_type == "text":
                converted.append({"type": "input_text", "text": block.get("text", "")})
            elif block_type == "image":
                source = block.get("source", {})
                media_type = source.get("media_type", "image/png")
                data = source.get("data", "")
                converted.append(
                    {
                        "type": "input_image",
                        "image_url": f"data:{media_type};base64,{data}",
                    }
                )
            else:
                # Pass through unknown block types unchanged
                converted.append(block)
        return converted

    # ------------------------------------------------------------------
    # Response conversion: Responses API -> Chat Completions
    # ------------------------------------------------------------------

    def convert_response(self, responses_data: Dict[str, Any]) -> Dict[str, Any]:
        """Convert Responses API output back to Chat Completions format.

        The ``output`` array can contain items of type:
        - ``message`` -> ``choices[0].message.content``
        - ``function_call`` -> ``choices[0].message.tool_calls``
        - ``reasoning`` -> ``choices[0].message.reasoning_content``
        """
        output_items = responses_data.get("output", [])

        text_parts: List[str] = []
        tool_calls: List[Dict[str, Any]] = []
        reasoning_parts: List[str] = []

        for item in output_items:
            item_type = item.get("type")

            if item_type == "message":
                # Content can be a string or list of content blocks
                content = item.get("content", "")
                if isinstance(content, list):
                    for block in content:
                        if isinstance(block, dict) and block.get("type") == "output_text":
                            text_parts.append(block.get("text", ""))
                        elif isinstance(block, str):
                            text_parts.append(block)
                elif isinstance(content, str):
                    text_parts.append(content)

            elif item_type == "function_call":
                tool_calls.append(
                    {
                        "id": item.get("call_id", item.get("id", "")),
                        "type": "function",
                        "function": {
                            "name": item.get("name", ""),
                            "arguments": item.get("arguments", "{}"),
                        },
                    }
                )

            elif item_type == "reasoning":
                # Reasoning/thinking traces
                summary = item.get("summary", [])
                if isinstance(summary, list):
                    for s in summary:
                        if isinstance(s, dict):
                            reasoning_parts.append(s.get("text", ""))
                        elif isinstance(s, str):
                            reasoning_parts.append(s)

        content = "\n".join(text_parts) if text_parts else None
        reasoning_content = "\n".join(reasoning_parts) if reasoning_parts else None

        message: Dict[str, Any] = {
            "role": "assistant",
            "content": content,
        }

        if reasoning_content:
            message["reasoning_content"] = reasoning_content

        if tool_calls:
            message["tool_calls"] = tool_calls

        # Determine finish_reason
        finish_reason = "stop"
        status = responses_data.get("status", "completed")
        if tool_calls:
            finish_reason = "tool_calls"
        elif status == "incomplete":
            finish_reason = "length"

        # Usage
        usage_raw = responses_data.get("usage", {})

        return {
            "id": responses_data.get("id", ""),
            "object": "chat.completion",
            "model": responses_data.get("model", ""),
            "choices": [
                {
                    "index": 0,
                    "message": message,
                    "finish_reason": finish_reason,
                }
            ],
            "usage": {
                "prompt_tokens": usage_raw.get("input_tokens", 0),
                "completion_tokens": usage_raw.get("output_tokens", 0),
                "total_tokens": (
                    usage_raw.get("input_tokens", 0) + usage_raw.get("output_tokens", 0)
                ),
            },
        }

    # ------------------------------------------------------------------
    # HTTP POST with retry (same pattern as AnthropicAdapter)
    # ------------------------------------------------------------------

    def post_json(self, payload: Dict[str, Any], *, task_monitor: Any = None) -> Any:
        """Send request to the Responses API with retry logic.

        Converts the Chat Completions payload to Responses format, sends the
        request, then wraps the converted response in a MockResponse so the
        calling agent code sees the familiar Chat Completions shape.
        """
        from dataclasses import dataclass

        @dataclass
        class HttpResult:
            success: bool
            response: Union[httpx.Response, None] = None
            error: Union[str, None] = None
            interrupted: bool = False

        @dataclass
        class MockResponse:
            status_code: int
            _json_data: Dict[str, Any]
            text: str
            headers: Dict[str, str]

            def json(self) -> Dict[str, Any]:
                return self._json_data

        responses_payload = self.convert_request(payload)
        last_result: Any = None

        for attempt in range(MAX_RETRIES + 1):
            try:
                response = self._client.post(self.api_url, json=responses_payload)

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
                            "OpenAI Responses HTTP %d — retrying in %.1fs (attempt %d/%d)",
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

                responses_data = response.json()
                openai_data = self.convert_response(responses_data)

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
