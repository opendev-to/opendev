"""LLM-based topic detection for dynamic session titling.

On each user message, fires a lightweight LLM call in a background thread
to detect whether the conversation topic has changed. If it has, updates
the session title via ``SessionManager.set_title()``, which in turn
updates ``sessions-index.json``.

Graceful degradation: no API key → no-op. LLM failure → keep existing title.
Never crashes.
"""

from __future__ import annotations

import json
import logging
import os
import threading
from typing import TYPE_CHECKING, Any, Optional

from swecli.core.agents.prompts.loader import load_prompt

if TYPE_CHECKING:
    from swecli.core.context_engineering.history.session_manager import SessionManager
    from swecli.models.config import AppConfig

logger = logging.getLogger(__name__)

# Cheap models per provider — small, fast, inexpensive
_CHEAP_MODELS: dict[str, str] = {
    "openai": "gpt-4o-mini",
    "anthropic": "claude-3-5-haiku-20241022",
    "fireworks": "accounts/fireworks/models/llama-v3p1-8b-instruct",
}

# Env var names per provider
_ENV_KEYS: dict[str, str] = {
    "openai": "OPENAI_API_KEY",
    "anthropic": "ANTHROPIC_API_KEY",
    "fireworks": "FIREWORKS_API_KEY",
}

# Maximum number of recent messages to send for topic detection
_MAX_RECENT_MESSAGES = 4


class TopicDetector:
    """Fire-and-forget LLM-based topic detector for session titles.

    Usage::

        detector = TopicDetector(config)
        detector.detect(session_manager, session_id, plain_messages)

    The ``detect()`` call is non-blocking — it spawns a daemon thread.
    """

    def __init__(self, config: "AppConfig") -> None:
        self._config = config
        self._system_prompt = load_prompt("memory/topic_detection_prompt")

        # Resolve cheap model and create HTTP client
        resolved = self._resolve_cheap_model(config)
        if resolved is None:
            self._client: Any = None
            self._provider_id: Optional[str] = None
            self._model_id: Optional[str] = None
        else:
            self._provider_id, self._model_id = resolved
            try:
                from swecli.core.agents.components.api.configuration import (
                    create_http_client_for_provider,
                )

                self._client = create_http_client_for_provider(self._provider_id, config)
            except Exception:
                self._client = None

    # ------------------------------------------------------------------
    # Public API
    # ------------------------------------------------------------------

    def detect(
        self,
        session_manager: "SessionManager",
        session_id: str,
        messages: list[dict[str, str]],
    ) -> None:
        """Trigger topic detection in a background thread.

        Args:
            session_manager: Session manager for updating titles.
            session_id: ID of the current session.
            messages: Plain user/assistant messages (``{"role": ..., "content": ...}``).
        """
        if not self._config.topic_detection:
            return
        if self._client is None:
            return

        recent = messages[-_MAX_RECENT_MESSAGES:] if messages else []
        if not recent:
            return

        thread = threading.Thread(
            target=self._detect_and_update,
            args=(session_manager, session_id, recent),
            daemon=True,
        )
        thread.start()

    # ------------------------------------------------------------------
    # Internal
    # ------------------------------------------------------------------

    def _detect_and_update(
        self,
        session_manager: "SessionManager",
        session_id: str,
        recent_messages: list[dict[str, str]],
    ) -> None:
        """Make LLM call and update title if topic changed. Never raises."""
        try:
            result = self._call_llm(recent_messages)
            if result is None:
                return

            if result.get("isNewTopic") is True:
                title = result.get("title")
                if title and isinstance(title, str):
                    title = title.strip()[:50]
                    if title:
                        session_manager.set_title(session_id, title)
        except Exception:
            logger.debug("Topic detection failed", exc_info=True)

    def _call_llm(self, recent_messages: list[dict[str, str]]) -> Optional[dict]:
        """Call the LLM and parse the JSON response.

        Builds the API payload with:
        1. System message: the topic_detection_prompt template
        2. Conversation messages: recent user/assistant messages as proper API messages
        3. Final user message: asking the model to analyze

        Returns:
            Parsed JSON dict with ``isNewTopic`` and ``title``, or ``None`` on failure.
        """
        # Build messages list for the API call
        api_messages: list[dict[str, Any]] = [
            {"role": "system", "content": self._system_prompt},
        ]

        # Add recent conversation messages as proper role-based messages
        for msg in recent_messages:
            api_messages.append({"role": msg["role"], "content": msg["content"]})

        # Final analysis prompt
        api_messages.append(
            {
                "role": "user",
                "content": "Analyze the conversation above. Is the latest message a new topic?",
            }
        )

        # Build payload
        from swecli.core.agents.components.api.configuration import build_temperature_param

        payload: dict[str, Any] = {
            "model": self._model_id,
            "messages": api_messages,
            "max_tokens": 100,
            **build_temperature_param(self._model_id, 0.0),
        }

        result = self._client.post_json(payload)

        if not result.success or result.response is None:
            return None

        if result.response.status_code != 200:
            return None

        try:
            data = result.response.json()
            # OpenAI-compatible format (including AnthropicAdapter output)
            content = data["choices"][0]["message"]["content"]
            return json.loads(content)
        except (KeyError, IndexError, json.JSONDecodeError, TypeError):
            return None

    @staticmethod
    def _resolve_cheap_model(config: "AppConfig") -> Optional[tuple[str, str]]:
        """Resolve a cheap model for topic detection.

        Tries the user's configured provider first, then falls back to
        any provider that has an API key set.

        Returns:
            ``(provider_id, model_id)`` or ``None`` if no key available.
        """
        # Try user's provider first
        provider = config.model_provider
        if provider in _CHEAP_MODELS:
            env_key = _ENV_KEYS.get(provider, "")
            if config.api_key or os.getenv(env_key):
                return provider, _CHEAP_MODELS[provider]

        # Fallback: try each provider in order
        for prov, model in _CHEAP_MODELS.items():
            env_key = _ENV_KEYS.get(prov, "")
            if os.getenv(env_key):
                return prov, model

        return None
