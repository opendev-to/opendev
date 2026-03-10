"""Message schema validation for session history.

Validates messages before saving and repairs/filters on load to prevent
malformed messages from corrupting session history.
"""

import json
import logging
from dataclasses import dataclass
from typing import Optional

from opendev.models.message import ChatMessage, Role, ToolCall

logger = logging.getLogger(__name__)


@dataclass
class ValidationVerdict:
    """Result of message validation."""

    is_valid: bool
    reason: str = ""


def _is_json_serializable(obj: object) -> bool:
    """Check if an object is natively JSON-serializable (without default=str fallback)."""
    try:
        json.dumps(obj)
        return True
    except (TypeError, ValueError, OverflowError):
        return False


def _validate_tool_call(tc: ToolCall, path: str = "") -> Optional[str]:
    """Validate a single tool call. Returns error reason or None if valid."""
    prefix = f"{path}tool_call" if path else "tool_call"

    if not tc.id or not tc.id.strip():
        return f"{prefix} has empty id"

    if not tc.name or not tc.name.strip():
        return f"{prefix} [{tc.id}] has empty name"

    if not isinstance(tc.parameters, dict):
        return f"{prefix} [{tc.id}] has non-dict parameters: {type(tc.parameters).__name__}"

    # A tool call must have result or error (except task_complete)
    if tc.result is None and tc.error is None and tc.name != "task_complete":
        return f"{prefix} [{tc.id}] ({tc.name}) has no result and no error"

    # Check result serializability
    if tc.result is not None and not _is_json_serializable(tc.result):
        return f"{prefix} [{tc.id}] has non-serializable result"

    # Validate nested tool calls recursively
    for i, nested in enumerate(tc.nested_tool_calls):
        reason = _validate_tool_call(nested, path=f"{prefix}[{i}].")
        if reason:
            return reason

    return None


def validate_message(msg: ChatMessage) -> ValidationVerdict:
    """Strict pre-save validation of a message.

    Returns a ValidationVerdict indicating whether the message is valid for saving.
    """
    role = msg.role

    if role == Role.USER:
        if not msg.content or not msg.content.strip():
            return ValidationVerdict(False, "user message has empty content")
        if msg.tool_calls:
            return ValidationVerdict(False, "user message has tool_calls")

    elif role == Role.ASSISTANT:
        has_content = bool(msg.content and msg.content.strip())
        has_tools = bool(msg.tool_calls)
        if not has_content and not has_tools:
            return ValidationVerdict(False, "assistant message has no content and no tool_calls")

        # Validate each tool call
        for tc in msg.tool_calls:
            reason = _validate_tool_call(tc)
            if reason:
                return ValidationVerdict(False, reason)

        # thinking_trace / reasoning_content: reject empty strings
        if msg.thinking_trace is not None and not msg.thinking_trace.strip():
            return ValidationVerdict(False, "assistant message has empty thinking_trace")
        if msg.reasoning_content is not None and not msg.reasoning_content.strip():
            return ValidationVerdict(False, "assistant message has empty reasoning_content")

    elif role == Role.SYSTEM:
        if not msg.content or not msg.content.strip():
            return ValidationVerdict(False, "system message has empty content")

    # Token usage validation
    if msg.token_usage is not None:
        if not isinstance(msg.token_usage, dict):
            return ValidationVerdict(False, "token_usage is not a dict")
        if not _is_json_serializable(msg.token_usage):
            return ValidationVerdict(False, "token_usage is not JSON-serializable")

    # Metadata validation
    if not isinstance(msg.metadata, dict):
        return ValidationVerdict(False, "metadata is not a dict")
    for key, value in msg.metadata.items():
        if not _is_json_serializable(value):
            return ValidationVerdict(False, f"metadata key '{key}' has non-serializable value")

    return ValidationVerdict(True)


def _repair_tool_call(tc: ToolCall) -> ToolCall:
    """Repair a single tool call in-place and return it."""
    # Fix non-dict parameters
    if not isinstance(tc.parameters, dict):
        if isinstance(tc.parameters, str):
            try:
                parsed = json.loads(tc.parameters)
                if isinstance(parsed, dict):
                    tc.parameters = parsed
                else:
                    tc.parameters = {"raw": tc.parameters}
            except (json.JSONDecodeError, TypeError):
                tc.parameters = {"raw": tc.parameters}
        else:
            tc.parameters = {"raw": str(tc.parameters)}

    # Fix incomplete tool calls (no result and no error)
    if tc.result is None and tc.error is None and tc.name != "task_complete":
        tc.error = "Tool execution was interrupted or never completed."

    # Coerce non-serializable result
    if tc.result is not None and not _is_json_serializable(tc.result):
        tc.result = str(tc.result)

    # Repair nested tool calls recursively
    tc.nested_tool_calls = [_repair_tool_call(ntc) for ntc in tc.nested_tool_calls]

    return tc


def repair_message(msg: ChatMessage) -> Optional[ChatMessage]:
    """Attempt to repair a malformed message. Returns None if unrecoverable (drop).

    Used on load to fix common issues in persisted messages.
    """
    has_content = bool(msg.content and msg.content.strip())
    has_tools = bool(msg.tool_calls)

    # Drop completely empty messages
    if not has_content and not has_tools:
        return None

    # Repair tool calls
    msg.tool_calls = [_repair_tool_call(tc) for tc in msg.tool_calls]

    # Normalize empty thinking_trace / reasoning_content to None
    if msg.thinking_trace is not None and not msg.thinking_trace.strip():
        msg.thinking_trace = None
    if msg.reasoning_content is not None and not msg.reasoning_content.strip():
        msg.reasoning_content = None

    # Fix non-serializable token_usage
    if msg.token_usage is not None:
        if not isinstance(msg.token_usage, dict) or not _is_json_serializable(msg.token_usage):
            msg.token_usage = None

    # Fix non-serializable metadata values
    if isinstance(msg.metadata, dict):
        clean_meta = {}
        for key, value in msg.metadata.items():
            if _is_json_serializable(value):
                clean_meta[key] = value
            else:
                clean_meta[key] = str(value)
        msg.metadata = clean_meta

    return msg


def filter_and_repair_messages(messages: list[ChatMessage]) -> list[ChatMessage]:
    """Bulk load-time cleanup: repair what we can, drop what we can't.

    Args:
        messages: Raw messages loaded from disk.

    Returns:
        Cleaned list of valid messages.
    """
    result = []
    dropped = 0
    repaired = 0

    for msg in messages:
        # Take a snapshot to detect repairs
        original_content = msg.content
        original_tools = len(msg.tool_calls)

        fixed = repair_message(msg)
        if fixed is None:
            dropped += 1
            continue

        # Count as repaired if anything changed
        if (
            fixed.thinking_trace != msg.thinking_trace
            or fixed.reasoning_content != msg.reasoning_content
            or fixed.token_usage != msg.token_usage
        ):
            repaired += 1

        result.append(fixed)

    if dropped or repaired:
        logger.warning(
            "Session message cleanup: %d dropped, %d repaired out of %d total",
            dropped,
            repaired,
            len(messages),
        )

    return result
