"""DisplayLedger — single entry point for all UI message display.

Replaces 6 fragmented display paths with a coordinated entry point that:
- Tracks conversation turns (user -> assistant -> idle)
- Deduplicates cross-path display (ui_callback + render_responses showing same message)
- Provides thread-safe display coordination
"""

from __future__ import annotations

import hashlib
import logging
import threading
from enum import Enum, auto
from typing import Any, Callable, Optional

logger = logging.getLogger(__name__)


class TurnState(Enum):
    IDLE = auto()
    USER_DISPLAYED = auto()
    RESPONDING = auto()
    COMPLETE = auto()


class DisplayLedger:
    """Coordinate all message display through a single entry point.

    All display paths (ui_callback, message_controller, message_processor,
    history_hydrator, render_responses, injection) go through this ledger
    to prevent duplicates and maintain turn structure.

    Attributes:
        conversation: The ConversationLog widget to display messages on.
    """

    def __init__(self, conversation: Any) -> None:
        self._conversation = conversation
        self._lock = threading.Lock()
        self._turn_id = 0
        self._turn_state = TurnState.IDLE
        # Dedup set: (turn_id, role, content_hash)
        self._displayed_hashes: set[tuple[int, str, str]] = set()

    @property
    def turn_state(self) -> TurnState:
        return self._turn_state

    @property
    def turn_id(self) -> int:
        return self._turn_id

    @staticmethod
    def _content_hash(content: str) -> str:
        return hashlib.sha256(content.encode("utf-8", errors="replace")).hexdigest()

    def display_user_message(
        self,
        content: str,
        source: str,
        *,
        call_on_ui: Optional[Callable[..., Any]] = None,
    ) -> bool:
        """Display a user message and open a new turn.

        Args:
            content: The user message text.
            source: Identifier for which code path is calling (for debugging).
            call_on_ui: Optional wrapper to run display on UI thread.

        Returns:
            True if the message was displayed, False if deduplicated.
        """
        with self._lock:
            self._turn_id += 1
            h = self._content_hash(content)
            key = (self._turn_id, "user", h)

            if key in self._displayed_hashes:
                logger.debug(
                    "DisplayLedger: deduped user message from %s (turn=%d)",
                    source,
                    self._turn_id,
                )
                return False

            self._displayed_hashes.add(key)
            self._turn_state = TurnState.USER_DISPLAYED

        # Display outside lock to avoid deadlock with UI thread
        if call_on_ui and hasattr(self._conversation, "add_user_message"):
            call_on_ui(self._conversation.add_user_message, content)
        elif hasattr(self._conversation, "add_user_message"):
            self._conversation.add_user_message(content)

        return True

    def display_assistant_message(
        self,
        content: str,
        source: str,
        *,
        call_on_ui: Optional[Callable[..., Any]] = None,
    ) -> bool:
        """Display an assistant message in the current turn.

        Args:
            content: The assistant message text.
            source: Identifier for which code path is calling.
            call_on_ui: Optional wrapper to run display on UI thread.

        Returns:
            True if the message was displayed, False if deduplicated.
        """
        if not content or not content.strip():
            return False

        with self._lock:
            h = self._content_hash(content)
            key = (self._turn_id, "assistant", h)

            if key in self._displayed_hashes:
                logger.debug(
                    "DisplayLedger: deduped assistant message from %s (turn=%d)",
                    source,
                    self._turn_id,
                )
                return False

            self._displayed_hashes.add(key)
            self._turn_state = TurnState.RESPONDING

        if call_on_ui and hasattr(self._conversation, "add_assistant_message"):
            call_on_ui(self._conversation.add_assistant_message, content)
        elif hasattr(self._conversation, "add_assistant_message"):
            self._conversation.add_assistant_message(content)

        return True

    def display_system_message(
        self,
        content: str,
        source: str,
        *,
        call_on_ui: Optional[Callable[..., Any]] = None,
    ) -> None:
        """Display a system message (no turn tracking, no dedup).

        Args:
            content: The system message text.
            source: Identifier for which code path is calling.
            call_on_ui: Optional wrapper to run display on UI thread.
        """
        if call_on_ui and hasattr(self._conversation, "add_system_message"):
            call_on_ui(self._conversation.add_system_message, content)
        elif hasattr(self._conversation, "add_system_message"):
            self._conversation.add_system_message(content)

    def complete_turn(self, source: str = "") -> None:
        """Close the current turn. State -> IDLE.

        Args:
            source: Identifier for which code path is calling.
        """
        with self._lock:
            self._turn_state = TurnState.IDLE
            # Keep recent hashes for cross-path dedup within the same turn,
            # but clear old turns to prevent unbounded memory growth
            current = self._turn_id
            self._displayed_hashes = {
                (tid, role, h)
                for tid, role, h in self._displayed_hashes
                if tid >= current - 1
            }

    def replay_message(self, role: str, content: str, source: str = "history_hydrator") -> None:
        """Record a message for dedup during history hydration without affecting turn state.

        This allows the hydrator to register what's been displayed so that
        subsequent real-time paths don't re-display the same content.

        Args:
            role: Message role ("user" or "assistant").
            content: Message content.
            source: Identifier for which code path is calling.
        """
        with self._lock:
            if role == "user":
                self._turn_id += 1
            h = self._content_hash(content)
            self._displayed_hashes.add((self._turn_id, role, h))
