"""HTTP client helpers for agent chat completions."""

from __future__ import annotations

import copy
import logging
import threading
import time
from dataclasses import dataclass
from typing import Any, Dict, Union

import httpx

logger = logging.getLogger(__name__)

# Retry configuration
MAX_RETRIES = 3
RETRY_DELAYS = [1.0, 2.0, 4.0]  # Exponential backoff in seconds
RETRYABLE_STATUS_CODES = {429, 503}


@dataclass
class HttpResult:
    """Container describing the outcome of an HTTP request."""

    success: bool
    response: Union[httpx.Response, None] = None
    error: Union[str, None] = None
    interrupted: bool = False


# Import ProviderAdapter lazily to avoid circular import at module level.
# AgentHttpClient is defined in this module alongside HttpResult (which the
# ABC references), so we use a forward reference string for the base class
# and register it as a virtual subclass after definition.


class AgentHttpClient:
    """Thin wrapper around httpx with interrupt support and retry logic."""

    TIMEOUT = httpx.Timeout(connect=10.0, read=300.0, write=10.0, pool=10.0)

    def __init__(self, api_url: str, headers: dict[str, str]) -> None:
        self._api_url = api_url
        self._headers = headers
        self._client = httpx.Client(headers=headers, timeout=self.TIMEOUT)

    def _get_retry_delay(self, response: httpx.Response, attempt: int) -> float:
        """Determine retry delay from Retry-After header or default backoff.

        Args:
            response: The HTTP response with retryable status code.
            attempt: Zero-based retry attempt index.

        Returns:
            Delay in seconds before the next retry.
        """
        retry_after = response.headers.get("Retry-After")
        if retry_after is not None:
            try:
                return max(0.0, float(retry_after))
            except ValueError:
                pass
        return RETRY_DELAYS[min(attempt, len(RETRY_DELAYS) - 1)]

    @staticmethod
    def _should_interrupt(task_monitor: Any) -> bool:
        """Check if the task monitor signals an interrupt."""
        if task_monitor is None:
            return False
        if hasattr(task_monitor, "should_interrupt"):
            return task_monitor.should_interrupt()
        if hasattr(task_monitor, "is_interrupted"):
            return task_monitor.is_interrupted()
        return False

    @staticmethod
    def _normalize_image_blocks(payload: dict[str, Any]) -> dict[str, Any]:
        """Convert Anthropic-format image blocks to Chat Completions format.

        Scans all messages in *payload* and rewrites:
        - ``{"type": "image", "source": {"type": "base64", "media_type": M, "data": D}}``
          → ``{"type": "image_url", "image_url": {"url": "data:M;base64,D"}}``

        Text blocks (``{"type": "text", ...}``) are left as-is since they are
        already valid in Chat Completions format.

        Returns a shallow copy of the payload with normalised messages (the
        original payload is not mutated).
        """
        messages = payload.get("messages")
        if not messages:
            return payload

        needs_copy = False
        for msg in messages:
            content = msg.get("content")
            if isinstance(content, list):
                for block in content:
                    if isinstance(block, dict) and block.get("type") == "image":
                        needs_copy = True
                        break
            if needs_copy:
                break

        if not needs_copy:
            return payload

        new_payload = copy.copy(payload)
        new_messages = []
        for msg in messages:
            content = msg.get("content")
            if not isinstance(content, list):
                new_messages.append(msg)
                continue

            new_blocks = []
            changed = False
            for block in content:
                if isinstance(block, dict) and block.get("type") == "image":
                    source = block.get("source", {})
                    media_type = source.get("media_type", "image/png")
                    data = source.get("data", "")
                    new_blocks.append(
                        {
                            "type": "image_url",
                            "image_url": {"url": f"data:{media_type};base64,{data}"},
                        }
                    )
                    changed = True
                else:
                    new_blocks.append(block)

            if changed:
                new_msg = dict(msg)
                new_msg["content"] = new_blocks
                new_messages.append(new_msg)
            else:
                new_messages.append(msg)

        new_payload["messages"] = new_messages
        return new_payload

    def post_json(
        self, payload: dict[str, Any], *, task_monitor: Union[Any, None] = None
    ) -> HttpResult:
        """Execute a POST request with retry logic and interrupt support.

        Retries on HTTP 429 (rate limit) and 503 (service unavailable) with
        exponential backoff. Respects the ``Retry-After`` header when present.
        """
        payload = self._normalize_image_blocks(payload)
        last_result: Union[HttpResult, None] = None

        for attempt in range(MAX_RETRIES + 1):
            # Check interrupt before each attempt
            if self._should_interrupt(task_monitor):
                return HttpResult(success=False, error="Interrupted by user", interrupted=True)

            result = self._execute_request(payload, task_monitor=task_monitor)

            # On network/exception failure, don't retry — return immediately
            if not result.success:
                return result

            # Check for retryable HTTP status codes
            response = result.response
            if response is not None and response.status_code in RETRYABLE_STATUS_CODES:
                last_result = result
                if attempt < MAX_RETRIES:
                    delay = self._get_retry_delay(response, attempt)
                    logger.warning(
                        "HTTP %d from %s — retrying in %.1fs (attempt %d/%d)",
                        response.status_code,
                        self._api_url,
                        delay,
                        attempt + 1,
                        MAX_RETRIES,
                    )
                    # Sleep in small increments to stay responsive to interrupts
                    deadline = time.monotonic() + delay
                    while time.monotonic() < deadline:
                        if self._should_interrupt(task_monitor):
                            return HttpResult(
                                success=False,
                                error="Interrupted by user",
                                interrupted=True,
                            )
                        time.sleep(min(0.1, deadline - time.monotonic()))
                    continue
                # Exhausted retries — fall through and return last result
                logger.warning(
                    "HTTP %d from %s — exhausted %d retries",
                    response.status_code,
                    self._api_url,
                    MAX_RETRIES,
                )
                return last_result

            # Non-retryable response (success or client error)
            return result

        # Should not normally reach here, but satisfy the type checker
        return last_result or HttpResult(success=False, error="Unexpected retry exhaustion")

    def _execute_request(
        self, payload: dict[str, Any], *, task_monitor: Union[Any, None] = None
    ) -> HttpResult:
        """Execute a single POST request, optionally with interrupt monitoring."""
        # Fast path when no monitor is provided
        if task_monitor is None:
            try:
                response = self._client.post(self._api_url, json=payload)
                return HttpResult(success=True, response=response)
            except Exception as exc:  # pragma: no cover - propagation handled by caller
                return HttpResult(success=False, error=str(exc))

        # Interrupt-aware execution path
        response_container: dict[str, Any] = {"response": None, "error": None}

        def make_request() -> None:
            try:
                response_container["response"] = self._client.post(self._api_url, json=payload)
            except Exception as exc:  # pragma: no cover - captured for caller
                response_container["error"] = exc

        request_thread = threading.Thread(target=make_request, daemon=True)
        request_thread.start()

        from swecli.ui_textual.debug_logger import debug_log

        poll_count = 0
        while request_thread.is_alive():
            poll_count += 1

            # Log every 10th poll (every ~1 second)
            if poll_count % 10 == 1:
                interrupt_flag = self._should_interrupt(task_monitor)
                debug_log(
                    "HttpClient",
                    f"poll #{poll_count}, should_interrupt={interrupt_flag}, "
                    f"task_monitor={task_monitor}",
                )

            if self._should_interrupt(task_monitor):
                debug_log(
                    "HttpClient",
                    f"INTERRUPT DETECTED at poll #{poll_count}",
                )
                return HttpResult(success=False, error="Interrupted by user", interrupted=True)
            request_thread.join(timeout=0.01)  # 10ms polling for ESC interrupt

        if response_container["error"]:
            return HttpResult(success=False, error=str(response_container["error"]))

        return HttpResult(success=True, response=response_container["response"])

    # ------------------------------------------------------------------
    # ProviderAdapter interface (passthrough for generic OpenAI-compat)
    # ------------------------------------------------------------------

    def convert_request(self, payload: Dict[str, Any]) -> Dict[str, Any]:
        """Passthrough -- payload is already in Chat Completions format."""
        return self._normalize_image_blocks(payload)

    def convert_response(self, response_data: Dict[str, Any]) -> Dict[str, Any]:
        """Passthrough -- response is already in Chat Completions format."""
        return response_data
