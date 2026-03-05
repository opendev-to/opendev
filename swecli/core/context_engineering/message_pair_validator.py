"""Message pair integrity validator for API message lists.

Ensures structural invariants:
- Every assistant tool_call has a corresponding tool result message
- No orphaned tool results without matching tool_calls
- Detects consecutive same-role violations (warning only)
"""

from __future__ import annotations

import logging
from dataclasses import dataclass, field
from enum import Enum, auto
from typing import Any

logger = logging.getLogger(__name__)


class ViolationType(Enum):
    MISSING_TOOL_RESULT = auto()
    ORPHANED_TOOL_RESULT = auto()
    CONSECUTIVE_SAME_ROLE = auto()


@dataclass
class Violation:
    violation_type: ViolationType
    index: int
    detail: str


@dataclass
class ValidationResult:
    violations: list[Violation] = field(default_factory=list)
    repair_actions: list[str] = field(default_factory=list)
    repaired: bool = False

    @property
    def is_valid(self) -> bool:
        return len(self.violations) == 0


class MessagePairValidator:
    """Validates and repairs structural integrity of flat API message lists."""

    SYNTHETIC_TOOL_RESULT = (
        "Error: Tool execution result was lost. "
        "The tool may have been interrupted or crashed."
    )

    @staticmethod
    def validate_tool_results_complete(
        tool_calls: list[dict[str, Any]],
        tool_results_by_id: dict[str, dict[str, Any]],
    ) -> dict[str, dict[str, Any]]:
        """Ensure every tool_call has an entry in results. Fill missing with synthetic errors.

        This is the pre-batch-add guard for ReactExecutor. Call BEFORE iterating
        tool_calls to add results to history.

        Args:
            tool_calls: List of tool call dicts with 'id' and 'function' keys.
            tool_results_by_id: Mutable dict mapping tool_call_id -> result dict.

        Returns:
            The same tool_results_by_id dict, with missing entries filled.
        """
        for tc in tool_calls:
            tc_id = tc.get("id", "")
            if tc_id and tc_id not in tool_results_by_id:
                tool_name = tc.get("function", {}).get("name", "unknown")
                logger.warning(
                    "Missing tool result for %s (id=%s), inserting synthetic error",
                    tool_name,
                    tc_id,
                )
                tool_results_by_id[tc_id] = {
                    "success": False,
                    "error": (
                        f"Tool '{tool_name}' execution was interrupted or never started."
                    ),
                    "output": "",
                    "synthetic": True,
                }
        return tool_results_by_id

    @staticmethod
    def validate(messages: list[dict[str, Any]]) -> ValidationResult:
        """Validate structural integrity of an API message list.

        Single forward pass checking:
        - Every tool_call ID from assistant messages has a matching tool result
        - No orphaned tool results without a preceding tool_call
        - Consecutive same-role messages (warning only)

        Args:
            messages: Flat list of API messages (role, content, tool_calls, etc.)

        Returns:
            ValidationResult with any violations found.
        """
        result = ValidationResult()
        expected_tool_results: dict[str, int] = {}  # tc_id -> assistant msg index
        prev_role: str | None = None

        for i, msg in enumerate(messages):
            role = msg.get("role", "")

            # Check consecutive same role (warning only, skip tool role)
            if role == prev_role and role not in ("tool",):
                result.violations.append(
                    Violation(
                        ViolationType.CONSECUTIVE_SAME_ROLE,
                        i,
                        f"Consecutive '{role}' at index {i}",
                    )
                )

            if role == "assistant":
                tool_calls = msg.get("tool_calls") or []
                for tc in tool_calls:
                    tc_id = tc.get("id", "")
                    if tc_id:
                        expected_tool_results[tc_id] = i

            elif role == "tool":
                tc_id = msg.get("tool_call_id", "")
                if tc_id in expected_tool_results:
                    del expected_tool_results[tc_id]
                elif tc_id:
                    result.violations.append(
                        Violation(
                            ViolationType.ORPHANED_TOOL_RESULT,
                            i,
                            f"Orphaned tool result for id={tc_id} at index {i}",
                        )
                    )

            prev_role = role

        # Remaining expected IDs are missing
        for tc_id, assistant_idx in expected_tool_results.items():
            result.violations.append(
                Violation(
                    ViolationType.MISSING_TOOL_RESULT,
                    assistant_idx,
                    f"Missing tool result for id={tc_id} (assistant at index {assistant_idx})",
                )
            )

        return result

    @classmethod
    def repair(
        cls, messages: list[dict[str, Any]]
    ) -> tuple[list[dict[str, Any]], ValidationResult]:
        """Validate and repair an API message list.

        - MISSING_TOOL_RESULT: Insert synthetic tool result after the assistant message's
          existing tool results.
        - ORPHANED_TOOL_RESULT: Remove the orphaned message.

        Args:
            messages: Flat list of API messages.

        Returns:
            Tuple of (repaired message list, ValidationResult with repair actions).
        """
        vr = cls.validate(messages)
        if vr.is_valid:
            return list(messages), vr

        # Collect indices to remove (orphaned tool results)
        orphan_indices: set[int] = set()
        for v in vr.violations:
            if v.violation_type == ViolationType.ORPHANED_TOOL_RESULT:
                orphan_indices.add(v.index)
                vr.repair_actions.append(f"Removed orphaned tool result at index {v.index}")

        # Collect missing tool result IDs grouped by assistant message index
        missing_by_assistant: dict[int, list[str]] = {}
        for v in vr.violations:
            if v.violation_type == ViolationType.MISSING_TOOL_RESULT:
                # Extract tc_id from detail string
                tc_id = v.detail.split("id=")[1].split(" ")[0] if "id=" in v.detail else ""
                if tc_id:
                    missing_by_assistant.setdefault(v.index, []).append(tc_id)

        # Build repaired list
        repaired: list[dict[str, Any]] = []
        for i, msg in enumerate(messages):
            if i in orphan_indices:
                continue
            repaired.append(msg)

            # After an assistant message, check if we need to insert synthetic results
            if i in missing_by_assistant:
                # Find the insertion point: after all existing tool results for this assistant
                # Since we're building linearly, the synthetic results go right after
                # any tool results that follow this assistant message in the original list
                # But we need to insert after the LAST existing tool result for this assistant.
                # Since we process linearly, we'll handle this by checking ahead.
                pass

        # The simple linear approach above doesn't handle insertion well.
        # Let's rebuild more carefully.
        repaired = []
        i = 0
        while i < len(messages):
            msg = messages[i]

            if i in orphan_indices:
                i += 1
                continue

            repaired.append(msg)

            if i in missing_by_assistant:
                # Skip ahead past any existing tool results for this assistant
                j = i + 1
                while j < len(messages) and messages[j].get("role") == "tool":
                    if j not in orphan_indices:
                        repaired.append(messages[j])
                    j += 1

                # Insert synthetic results for missing IDs
                for tc_id in missing_by_assistant[i]:
                    repaired.append(
                        {
                            "role": "tool",
                            "tool_call_id": tc_id,
                            "content": cls.SYNTHETIC_TOOL_RESULT,
                        }
                    )
                    vr.repair_actions.append(
                        f"Inserted synthetic tool result for id={tc_id}"
                    )

                i = j
                continue

            i += 1

        vr.repaired = len(vr.repair_actions) > 0
        return repaired, vr
