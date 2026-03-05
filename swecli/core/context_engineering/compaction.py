"""Auto-compaction of conversation history when approaching context limits.

Implements staged context optimization with proactive reduction:
- 70%: Warning logged, tracking begins
- 80%: Progressive observation masking (old tool results → compact refs)
- 90%: Aggressive masking + trimming of old tool outputs
- 99%: Full LLM-powered compaction (summarize middle messages)

Also provides:
- History archival: writes full messages to file before compacting
- Artifact index: tracks files touched, survives compaction
"""

from __future__ import annotations

import logging
from datetime import datetime
from pathlib import Path
from typing import Any

from swecli.core.agents.components.api.configuration import build_temperature_param
from swecli.core.agents.prompts.loader import load_prompt
from swecli.core.context_engineering.retrieval.token_monitor import ContextTokenMonitor
from swecli.models.config import AppConfig

logger = logging.getLogger(__name__)

# Staged compaction thresholds (fraction of context window)
STAGE_WARNING = 0.70
STAGE_MASK = 0.80
STAGE_PRUNE = 0.85  # Fast pruning: strip old tool outputs before LLM compaction
STAGE_AGGRESSIVE = 0.90
STAGE_COMPACT = 0.99

# Token budget to protect from pruning (recent tool outputs)
PRUNE_PROTECTED_TOKENS = 40_000


class OptimizationLevel:
    """Optimization level returned by check_usage."""

    NONE = "none"
    WARNING = "warning"  # 70%: log warning
    MASK = "mask"  # 80%: progressive observation masking
    PRUNE = "prune"  # 85%: fast pruning of old tool outputs
    AGGRESSIVE = "aggressive"  # 90%: aggressive masking
    COMPACT = "compact"  # 99%: full compaction


class ArtifactIndex:
    """Tracks files touched during a session, surviving compaction.

    Records file operations (create, modify, read, delete) with metadata
    so the agent retains awareness of workspace state post-compaction.
    """

    def __init__(self) -> None:
        self._entries: dict[str, dict[str, Any]] = {}

    def record(
        self,
        file_path: str,
        operation: str,
        details: str = "",
    ) -> None:
        """Record a file operation.

        Args:
            file_path: Absolute or relative file path.
            operation: One of "created", "modified", "read", "deleted".
            details: Optional details (line count, key functions, etc.).
        """
        normalized = str(file_path)
        existing = self._entries.get(normalized)
        now = datetime.now().isoformat()

        if existing:
            existing["last_operation"] = operation
            existing["last_details"] = details
            existing["updated_at"] = now
            existing["operation_count"] = existing.get("operation_count", 1) + 1
            if operation not in existing.get("operations_seen", []):
                existing["operations_seen"].append(operation)
        else:
            self._entries[normalized] = {
                "file_path": normalized,
                "last_operation": operation,
                "last_details": details,
                "created_at": now,
                "updated_at": now,
                "operation_count": 1,
                "operations_seen": [operation],
            }

    def as_summary(self) -> str:
        """Format the artifact index as a compact summary for injection into compaction."""
        if not self._entries:
            return ""

        lines = ["## Artifact Index (files touched this session)"]
        for path, entry in self._entries.items():
            ops = ", ".join(entry["operations_seen"])
            detail = f" — {entry['last_details']}" if entry["last_details"] else ""
            lines.append(f"- `{path}` [{ops}]{detail}")
        return "\n".join(lines)

    def to_dict(self) -> dict[str, Any]:
        """Serialize for session persistence."""
        return {"entries": dict(self._entries)}

    @classmethod
    def from_dict(cls, data: dict[str, Any]) -> ArtifactIndex:
        """Deserialize from session data."""
        idx = cls()
        idx._entries = dict(data.get("entries", {}))
        return idx

    def __len__(self) -> int:
        return len(self._entries)


class ContextCompactor:
    """Auto-compacts conversation history when approaching context limits.

    Implements staged optimization that activates progressively as context fills:
    1. Warning at 70% — logs and starts tracking
    2. Observation masking at 80% — replaces old tool results with compact refs
    3. Aggressive masking at 90% — minimal refs for all but recent tool results
    4. Full compaction at 99% — LLM-powered summarization of old messages
    """

    def __init__(
        self,
        config: AppConfig,
        http_client: Any,
    ) -> None:
        self._config = config
        self._http_client = http_client
        self._token_monitor = ContextTokenMonitor()
        self._last_token_count = 0
        self._api_prompt_tokens: int = 0
        self._msg_count_at_calibration: int = 0

        self._max_context = getattr(config, "max_context_tokens", 100_000)
        logger.info(
            "ContextCompactor: max_context=%d tokens (model=%s)",
            self._max_context,
            getattr(config, "model", "unknown"),
        )

        # Artifact index survives compaction
        self.artifact_index = ArtifactIndex()

        # Track whether we've already warned at each stage (avoid log spam)
        self._warned_70 = False
        self._warned_80 = False
        self._warned_90 = False

        # Session ID for scratch file paths (set by react executor)
        self._session_id: str | None = None

        # Hook manager for PreCompact event
        self._hook_manager = None

    def set_hook_manager(self, hook_manager: Any) -> None:
        """Set the hook manager for PreCompact hooks.

        Args:
            hook_manager: HookManager instance
        """
        self._hook_manager = hook_manager

    # ------------------------------------------------------------------
    # Public: staged usage check
    # ------------------------------------------------------------------
    def check_usage(
        self,
        messages: list[dict[str, Any]],
        system_prompt: str,
    ) -> str:
        """Check context usage and return the appropriate optimization level.

        Returns:
            One of OptimizationLevel constants.
        """
        self._update_token_count(messages, system_prompt)
        pct = self.usage_pct / 100.0  # Convert to 0-1 range

        if pct >= STAGE_COMPACT:
            return OptimizationLevel.COMPACT
        if pct >= STAGE_AGGRESSIVE:
            if not self._warned_90:
                logger.warning(
                    "Context at %.1f%% — aggressive optimization active", pct * 100
                )
                self._warned_90 = True
            return OptimizationLevel.AGGRESSIVE
        if pct >= STAGE_PRUNE:
            return OptimizationLevel.PRUNE
        if pct >= STAGE_MASK:
            if not self._warned_80:
                logger.warning(
                    "Context at %.1f%% — observation masking active", pct * 100
                )
                self._warned_80 = True
            return OptimizationLevel.MASK
        if pct >= STAGE_WARNING:
            if not self._warned_70:
                logger.info("Context at %.1f%% — approaching limits", pct * 100)
                self._warned_70 = True
            return OptimizationLevel.WARNING
        return OptimizationLevel.NONE

    def should_compact(
        self,
        messages: list[dict[str, Any]],
        system_prompt: str,
    ) -> bool:
        """Check if conversation exceeds the compaction threshold.

        Backwards-compatible: returns True only when full compaction is needed.
        """
        self._update_token_count(messages, system_prompt)
        return self._last_token_count > int(self._max_context * STAGE_COMPACT)

    # ------------------------------------------------------------------
    # Public: observation masking
    # ------------------------------------------------------------------
    def mask_old_observations(
        self,
        messages: list[dict[str, Any]],
        level: str,
    ) -> list[dict[str, Any]]:
        """Replace old tool result messages with compact references.

        Tool outputs are ~80% of context tokens. This replaces tool result
        messages that are N+ turns old with minimal placeholders, dramatically
        reducing token usage without losing the tool call structure.

        Args:
            messages: Current API-format messages (mutated in-place).
            level: OptimizationLevel.MASK or AGGRESSIVE.

        Returns:
            The messages list (same reference, mutated).
        """
        if level == OptimizationLevel.MASK:
            # Keep recent 6 tool results intact, mask older ones
            recent_threshold = 6
        elif level == OptimizationLevel.AGGRESSIVE:
            # Keep only last 3 tool results intact
            recent_threshold = 3
        else:
            return messages

        # Find all tool result message indices (walk backwards)
        tool_indices: list[int] = []
        for i, msg in enumerate(messages):
            if msg.get("role") == "tool":
                tool_indices.append(i)

        if len(tool_indices) <= recent_threshold:
            return messages

        # Mask old tool results (all except the last `recent_threshold`)
        old_indices = set(tool_indices[: -recent_threshold])
        masked_count = 0
        for i in old_indices:
            msg = messages[i]
            content = msg.get("content", "")
            # Skip already-masked messages
            if content.startswith("[ref:"):
                continue
            # Replace with compact reference
            tool_call_id = msg.get("tool_call_id", "?")
            msg["content"] = f"[ref: tool result {tool_call_id} — see history]"
            masked_count += 1

        if masked_count > 0:
            logger.info(
                "Masked %d old tool results (level=%s, kept recent %d)",
                masked_count,
                level,
                recent_threshold,
            )

        return messages

    # ------------------------------------------------------------------
    # Public: fast pruning (cheaper than LLM compaction)
    # ------------------------------------------------------------------
    def prune_old_tool_outputs(
        self,
        messages: list[dict[str, Any]],
    ) -> list[dict[str, Any]]:
        """Strip old tool outputs while protecting the most recent ones.

        This is a fast, zero-cost alternative to LLM compaction. Walks
        backwards through messages, protects the last ~40K tokens worth
        of tool results, and replaces older ones with a `[pruned]` marker.

        Much cheaper than LLM summarization and often sufficient to stay
        under the context limit.

        Args:
            messages: Current API-format messages (mutated in-place).

        Returns:
            The messages list (same reference, mutated).
        """
        # Collect all tool result message indices (in reverse order)
        tool_indices: list[int] = []
        for i in range(len(messages) - 1, -1, -1):
            if messages[i].get("role") == "tool":
                tool_indices.append(i)

        if not tool_indices:
            return messages

        # Walk backwards, protecting recent tokens up to the budget
        protected_tokens = 0
        protected_indices: set[int] = set()
        for idx in tool_indices:
            content = messages[idx].get("content", "")
            # Skip already-pruned/masked messages
            if content.startswith("[ref:") or content == "[pruned]":
                continue
            # Rough token estimate: ~4 chars per token
            token_estimate = len(content) // 4
            if protected_tokens + token_estimate <= PRUNE_PROTECTED_TOKENS:
                protected_tokens += token_estimate
                protected_indices.add(idx)
            # Once budget exhausted, remaining are candidates for pruning

        # Prune unprotected tool results
        pruned_count = 0
        for idx in tool_indices:
            if idx in protected_indices:
                continue
            content = messages[idx].get("content", "")
            if content.startswith("[ref:") or content == "[pruned]":
                continue
            messages[idx]["content"] = "[pruned]"
            pruned_count += 1

        if pruned_count > 0:
            logger.info(
                "Pruned %d old tool outputs (protected %d, ~%dK tokens kept)",
                pruned_count,
                len(protected_indices),
                protected_tokens // 1000,
            )

        return messages

    # ------------------------------------------------------------------
    # Public: history archival
    # ------------------------------------------------------------------
    def archive_history(
        self,
        messages: list[dict[str, Any]],
        session_id: str | None = None,
    ) -> str | None:
        """Write full conversation to a file before compaction.

        The agent can grep this file to recover details lost in compaction.

        Args:
            messages: Messages about to be compacted.
            session_id: Session ID for file path scoping.

        Returns:
            Path to the archive file, or None if archival failed.
        """
        sid = session_id or self._session_id or "unknown"
        scratch_dir = Path.home() / ".opendev" / "scratch" / sid
        try:
            scratch_dir.mkdir(parents=True, exist_ok=True)
        except OSError:
            logger.warning("Failed to create scratch dir: %s", scratch_dir)
            return None

        timestamp = datetime.now().strftime("%Y%m%d_%H%M%S")
        archive_path = scratch_dir / f"history_archive_{timestamp}.md"

        try:
            lines: list[str] = [
                f"# Conversation Archive — {timestamp}",
                f"Session: {sid}",
                f"Messages: {len(messages)}",
                "",
            ]
            for i, msg in enumerate(messages):
                role = msg.get("role", "unknown")
                content = msg.get("content", "")
                lines.append(f"## Message {i} [{role}]")
                if content:
                    lines.append(content[:2000])
                # Include tool call info
                for tc in msg.get("tool_calls", []):
                    func = tc.get("function", {})
                    name = func.get("name", "?")
                    args_str = func.get("arguments", "")
                    lines.append(f"  Tool: {name}")
                    if args_str:
                        lines.append(f"  Args: {args_str[:500]}")
                lines.append("")

            archive_path.write_text("\n".join(lines), encoding="utf-8")
            logger.info("Archived %d messages to %s", len(messages), archive_path)
            return str(archive_path)
        except OSError:
            logger.warning("Failed to write history archive", exc_info=True)
            return None

    @property
    def usage_pct(self) -> float:
        """Context usage as percentage of the model's full context window (0-100+)."""
        if self._max_context <= 0:
            return 0.0
        if self._last_token_count == 0:
            return 0.0
        return (self._last_token_count / self._max_context) * 100

    def update_from_api_usage(self, prompt_tokens: int, message_count: int = 0) -> None:
        """Calibrate with real API token count."""
        if prompt_tokens > 0:
            self._api_prompt_tokens = prompt_tokens
            self._msg_count_at_calibration = message_count
            self._last_token_count = prompt_tokens
        else:
            logger.debug(
                "update_from_api_usage: prompt_tokens=0, skipping calibration "
                "(max_context=%d, last_token_count=%d)",
                self._max_context,
                self._last_token_count,
            )

    @property
    def pct_until_compact(self) -> float:
        """Percentage points remaining before full compaction triggers."""
        threshold_pct = STAGE_COMPACT * 100
        return max(0.0, threshold_pct - self.usage_pct)

    def compact(
        self,
        messages: list[dict[str, Any]],
        system_prompt: str,
        *,
        trigger: str = "auto",
    ) -> list[dict[str, Any]]:
        """Compact older messages into a summary, preserving recent context.

        Strategy:
            1. Archive full history to scratch file for post-compaction grep.
            2. Keep system prompt message (index 0).
            3. Keep last N messages intact.
            4. Summarize everything between into a single user message.
            5. Inject artifact index into the summary.

        Args:
            messages: Current conversation messages.
            system_prompt: System prompt string.
            trigger: What triggered compaction ("auto" or "manual").
        """
        # Fire PreCompact hook
        if self._hook_manager:
            from swecli.core.hooks.models import HookEvent

            if self._hook_manager.has_hooks_for(HookEvent.PRE_COMPACT):
                self._hook_manager.run_hooks(
                    HookEvent.PRE_COMPACT,
                    match_value=trigger,
                )

        if len(messages) <= 4:
            return messages

        # Step 1: Archive history before compaction
        archive_path = self.archive_history(messages)

        # Determine how many recent messages to preserve
        keep_recent = min(5, max(2, len(messages) // 3))

        head = messages[:1]
        middle = messages[1:-keep_recent]
        tail = messages[-keep_recent:]

        if not middle:
            return messages

        summary_text = self._summarize(middle)
        if not summary_text:
            summary_text = "[Previous conversation context was compacted.]"

        # Inject artifact index so file awareness survives compaction
        artifact_summary = self.artifact_index.as_summary()
        if artifact_summary:
            summary_text = f"{summary_text}\n\n{artifact_summary}"

        # Add archive reference so agent knows where to find full history
        if archive_path:
            summary_text += (
                f"\n\n**Note:** Full conversation history archived at "
                f"`{archive_path}`. Use read_file to recover details if needed."
            )

        summary_msg: dict[str, Any] = {
            "role": "user",
            "content": f"[CONVERSATION SUMMARY]\n{summary_text}",
        }

        compacted = head + [summary_msg] + tail

        logger.info(
            "Compacted %d messages → %d (removed %d, kept %d recent)",
            len(messages),
            len(compacted),
            len(middle),
            keep_recent,
        )

        # Invalidate API calibration (message list changed)
        self._api_prompt_tokens = 0
        self._msg_count_at_calibration = 0

        # Reset stage warnings so they fire again if context grows back
        self._warned_70 = False
        self._warned_80 = False
        self._warned_90 = False

        return compacted

    # ------------------------------------------------------------------
    # Internal
    # ------------------------------------------------------------------
    def _update_token_count(
        self,
        messages: list[dict[str, Any]],
        system_prompt: str,
    ) -> None:
        """Update _last_token_count using API calibration or tiktoken."""
        if self._api_prompt_tokens > 0:
            new_msg_count = len(messages) - self._msg_count_at_calibration
            if new_msg_count > 0:
                delta = self._count_message_tokens(messages[-new_msg_count:], "")
                total = self._api_prompt_tokens + delta
            else:
                total = self._api_prompt_tokens
        else:
            total = self._count_message_tokens(messages, system_prompt)
        self._last_token_count = total

    def _sanitize_for_summarization(self, messages: list[dict[str, Any]]) -> list[dict[str, Any]]:
        """Replace full tool results with summaries before sending to LLM.

        Prevents sensitive data from leaking into the summarization LLM calls.
        """
        sanitized = []
        for msg in messages:
            msg_copy = msg.copy()

            if "tool_calls" in msg_copy and msg_copy["tool_calls"]:
                sanitized_tool_calls = []
                for tc in msg_copy["tool_calls"]:
                    tc_copy = tc.copy()

                    if "result" in tc_copy:
                        if tc_copy.get("result_summary"):
                            tc_copy["result"] = tc_copy["result_summary"]
                        else:
                            result_str = str(tc_copy["result"])
                            if result_str:
                                tc_copy["result"] = (
                                    result_str[:200]
                                    + ("..." if len(result_str) > 200 else "")
                                )
                            else:
                                tc_copy["result"] = "[result omitted]"

                    sanitized_tool_calls.append(tc_copy)

                msg_copy["tool_calls"] = sanitized_tool_calls

            sanitized.append(msg_copy)

        return sanitized

    def _summarize(self, messages: list[dict[str, Any]]) -> str:
        """Use the configured LLM to summarize a block of messages."""
        sanitized = self._sanitize_for_summarization(messages)

        parts: list[str] = []
        for msg in sanitized:
            role = msg.get("role", "unknown")
            content = msg.get("content", "")
            if content:
                parts.append(f"[{role}] {content[:500]}")

        conversation_text = "\n".join(parts)

        compact_info = (
            self._config.get_compact_model_info()
            if hasattr(self._config, "get_compact_model_info")
            else None
        )
        if compact_info:
            _, model_id, _ = compact_info
        else:
            model_id = getattr(self._config, "model", "gpt-4o-mini")

        payload = {
            "model": model_id,
            "messages": [
                {"role": "system", "content": load_prompt("system/compaction")},
                {"role": "user", "content": conversation_text},
            ],
            "max_tokens": 1024,
            **build_temperature_param(model_id, 0.2),
        }

        try:
            result = self._http_client.post_json(payload)
            if result.success and result.response is not None:
                data = result.response.json()
                return data["choices"][0]["message"]["content"]
        except Exception:
            logger.warning("LLM summarization failed, using fallback", exc_info=True)

        return self._fallback_summary(messages)

    @staticmethod
    def _fallback_summary(messages: list[dict[str, Any]]) -> str:
        """Create a basic summary without an LLM call."""
        parts: list[str] = []
        total = 0
        for msg in messages:
            content = msg.get("content", "")
            role = msg.get("role", "")
            if content and role in ("user", "assistant"):
                snippet = content[:200]
                parts.append(f"- [{role}] {snippet}")
                total += len(snippet)
                if total > 2000:
                    parts.append(f"... ({len(messages) - len(parts)} more messages)")
                    break
        return "\n".join(parts)

    def _count_message_tokens(
        self,
        messages: list[dict[str, Any]],
        system_prompt: str,
    ) -> int:
        """Estimate total tokens across all messages and system prompt."""
        total = self._token_monitor.count_tokens(system_prompt)
        for msg in messages:
            content = msg.get("content", "")
            if isinstance(content, list):
                for block in content:
                    if isinstance(block, dict) and block.get("type") == "text":
                        total += self._token_monitor.count_tokens(block.get("text", ""))
            elif content:
                total += self._token_monitor.count_tokens(content)
            for tc in msg.get("tool_calls", []):
                func = tc.get("function", {})
                total += self._token_monitor.count_tokens(func.get("name", ""))
                total += self._token_monitor.count_tokens(func.get("arguments", ""))
        total += len(messages) * 4
        return total
