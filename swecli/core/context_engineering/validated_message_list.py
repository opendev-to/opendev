"""ValidatedMessageList — write-time enforcement of message pair invariants.

A list subclass that wraps ctx.messages and enforces structural invariants on every
mutation. All reads (iteration, indexing, len) work identically to list. Mutations
(append, extend, __setitem__, insert) are intercepted and routed through validated
methods.

State machine:
    EXPECT_ANY ──add_assistant(tc)──→ EXPECT_TOOL_RESULTS{pending_ids}
         ↑                                    │
         │                          add_tool_result(id) removes from pending
         │                                    │
         └────── all pending satisfied ───────┘
"""

from __future__ import annotations

import logging
import threading
from typing import Any, Iterable

logger = logging.getLogger(__name__)

SYNTHETIC_TOOL_RESULT = (
    "Error: Tool execution result was lost. "
    "The tool may have been interrupted or crashed."
)


class ValidatedMessageList(list):
    """Drop-in list replacement that enforces message pair invariants.

    All reads (iteration, indexing, len) work identically to list.
    Mutations (append, extend, __setitem__, insert) are intercepted.
    """

    def __init__(self, initial: list[dict] | None = None, strict: bool = False):
        """
        Args:
            initial: Existing messages to populate from (validated on load).
            strict: If True, raise on violations. If False, auto-repair + warn.
        """
        super().__init__()
        self._strict = strict
        self._pending_tool_ids: set[str] = set()
        self._lock = threading.Lock()

        if initial:
            # Bulk-load without per-message validation (trusts existing data)
            super().extend(initial)
            self._rebuild_pending_state()

    def _rebuild_pending_state(self) -> None:
        """Scan all messages to reconstruct pending tool_call IDs."""
        expected: set[str] = set()
        for msg in self:
            role = msg.get("role", "")
            if role == "assistant":
                for tc in msg.get("tool_calls") or []:
                    tc_id = tc.get("id", "")
                    if tc_id:
                        expected.add(tc_id)
            elif role == "tool":
                tc_id = msg.get("tool_call_id", "")
                expected.discard(tc_id)
        self._pending_tool_ids = expected

    # --- Validated mutation methods (preferred API) ---

    def add_user(self, content: str) -> None:
        """Append a user message. Auto-completes pending tool results if any."""
        with self._lock:
            self._auto_complete_pending("add_user")
            super().append({"role": "user", "content": content})

    def add_assistant(
        self, content: str | None, tool_calls: list[dict] | None = None
    ) -> None:
        """Append assistant message. If tool_calls present, enters EXPECT_TOOL_RESULTS."""
        with self._lock:
            self._auto_complete_pending("add_assistant")
            msg: dict[str, Any] = {"role": "assistant", "content": content or ""}
            if tool_calls:
                msg["tool_calls"] = tool_calls
                for tc in tool_calls:
                    tc_id = tc.get("id", "")
                    if tc_id:
                        self._pending_tool_ids.add(tc_id)
            super().append(msg)

    def add_tool_result(self, tool_call_id: str, content: str) -> None:
        """Append tool result. Rejects orphaned IDs not in pending set."""
        with self._lock:
            if tool_call_id not in self._pending_tool_ids:
                detail = f"Orphaned tool result for id={tool_call_id}"
                if self._strict:
                    raise ValueError(detail)
                logger.warning("ValidatedMessageList: %s (permissive mode, accepting)", detail)
            else:
                self._pending_tool_ids.discard(tool_call_id)
            super().append(
                {"role": "tool", "tool_call_id": tool_call_id, "content": content}
            )

    def add_tool_results_batch(
        self, tool_calls: list[dict], results_by_id: dict[str, Any]
    ) -> None:
        """Batch-add tool results. Fills missing with synthetic errors."""
        with self._lock:
            for tc in tool_calls:
                tc_id = tc.get("id", "")
                if not tc_id:
                    continue
                if tc_id in results_by_id:
                    result = results_by_id[tc_id]
                    content = str(result) if not isinstance(result, str) else result
                else:
                    tool_name = tc.get("function", {}).get("name", "unknown")
                    logger.warning(
                        "ValidatedMessageList: Missing result for %s (id=%s), "
                        "inserting synthetic error",
                        tool_name,
                        tc_id,
                    )
                    content = SYNTHETIC_TOOL_RESULT
                self._pending_tool_ids.discard(tc_id)
                super().append(
                    {"role": "tool", "tool_call_id": tc_id, "content": content}
                )

    # --- Intercepted list mutations (backward compat) ---

    def append(self, msg: dict) -> None:  # type: ignore[override]
        """Intercept raw appends. Route through validated methods."""
        role = msg.get("role", "")
        if role == "user":
            with self._lock:
                self._auto_complete_pending("append[user]")
                super().append(msg)
        elif role == "assistant":
            with self._lock:
                tool_calls = msg.get("tool_calls")
                if tool_calls:
                    # Don't auto-complete pending — the caller may be building a sequence
                    # where tool results are still being added. Only auto-complete if
                    # a NEW assistant message with NO tool_calls arrives (pure text response).
                    self._auto_complete_pending("append[assistant+tc]")
                    for tc in tool_calls:
                        tc_id = tc.get("id", "")
                        if tc_id:
                            self._pending_tool_ids.add(tc_id)
                else:
                    self._auto_complete_pending("append[assistant]")
                super().append(msg)
        elif role == "tool":
            tc_id = msg.get("tool_call_id", "")
            with self._lock:
                if tc_id not in self._pending_tool_ids:
                    detail = f"Orphaned tool result for id={tc_id}"
                    if self._strict:
                        raise ValueError(detail)
                    logger.warning(
                        "ValidatedMessageList: %s (permissive mode, accepting)", detail
                    )
                else:
                    self._pending_tool_ids.discard(tc_id)
                super().append(msg)
        elif role == "system":
            # System messages are always allowed (typically first message)
            super().append(msg)
        else:
            # Unknown role — pass through with warning
            logger.warning("ValidatedMessageList: Unknown role '%s', passing through", role)
            super().append(msg)

    def extend(self, msgs: Iterable[dict]) -> None:  # type: ignore[override]
        """Intercept raw extends. Route each message through append."""
        for msg in msgs:
            self.append(msg)

    def __setitem__(self, key, value) -> None:
        """Intercept slice assignment (e.g. ctx.messages[:] = compacted)."""
        super().__setitem__(key, value)
        # After bulk replacement, rebuild state from scratch
        self._rebuild_pending_state()

    def insert(self, index: int, msg: dict) -> None:  # type: ignore[override]
        """Intercept raw inserts. Validate then rebuild state."""
        super().insert(index, msg)
        self._rebuild_pending_state()

    # --- State queries ---

    @property
    def pending_tool_ids(self) -> frozenset[str]:
        """Tool call IDs still awaiting results."""
        return frozenset(self._pending_tool_ids)

    @property
    def has_pending_tools(self) -> bool:
        """True if in EXPECT_TOOL_RESULTS state."""
        return len(self._pending_tool_ids) > 0

    # --- Internal helpers ---

    def _auto_complete_pending(self, source: str) -> None:
        """Insert synthetic error results for any pending tool calls.

        Must be called under self._lock.
        """
        if not self._pending_tool_ids:
            return

        logger.warning(
            "ValidatedMessageList: Auto-completing %d pending tool results "
            "before %s: %s",
            len(self._pending_tool_ids),
            source,
            self._pending_tool_ids,
        )
        for tc_id in list(self._pending_tool_ids):
            super().append(
                {
                    "role": "tool",
                    "tool_call_id": tc_id,
                    "content": SYNTHETIC_TOOL_RESULT,
                }
            )
        self._pending_tool_ids.clear()
