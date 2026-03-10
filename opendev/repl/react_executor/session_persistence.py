"""Step persistence, artifact recording, and context usage tracking for ReactExecutor."""

from __future__ import annotations

import json
import logging
from typing import TYPE_CHECKING, Dict, Optional

if TYPE_CHECKING:
    pass

from opendev.models.message import ChatMessage, Role, ToolCall as ToolCallModel
from opendev.core.utils.tool_result_summarizer import summarize_tool_result

logger = logging.getLogger(__name__)


def _debug_log(message: str) -> None:
    """Write debug message to /tmp/swecli_react_debug.log."""
    from datetime import datetime

    log_file = "/tmp/swecli_react_debug.log"
    timestamp = datetime.now().strftime("%H:%M:%S.%f")[:-3]
    with open(log_file, "a") as f:
        f.write(f"[{timestamp}] {message}\n")


class SessionPersistenceMixin:
    """Mixin providing step persistence, artifact recording, and assistant message handling.

    Expects the host class to provide:
        - self.session_manager
        - self.config
        - self._compactor
        - self._tool_executor
        - self._current_thinking_trace
        - self._current_reasoning_content
        - self._current_token_usage
    """

    def _persist_step(
        self,
        ctx,
        tool_calls: list,
        results: Dict[str, dict],
        content: str,
        raw_content: Optional[str],
    ):
        """Persist the step to session manager and record learnings."""
        tool_call_objects = []

        for tc in tool_calls:
            tool_name = tc["function"]["name"]
            _debug_log(f"[PERSIST] Processing tool call: {tool_name}")
            if tool_name == "task_complete":
                continue

            full_result = results.get(tc["id"], {})
            _debug_log(f"[PERSIST] full_result keys: {list(full_result.keys())}")
            tool_error = full_result.get("error") if not full_result.get("success", True) else None
            tool_result_str = (
                full_result.get("output", "") if full_result.get("success", True) else None
            )
            result_summary = summarize_tool_result(tool_name, tool_result_str, tool_error)
            _debug_log(
                f"[PERSIST] result_summary: {result_summary[:100] if result_summary else None}"
            )

            nested_calls = []
            if (
                tool_name == "spawn_subagent"
                and ctx.ui_callback
                and hasattr(ctx.ui_callback, "get_and_clear_nested_calls")
            ):
                nested_calls = ctx.ui_callback.get_and_clear_nested_calls()

            _debug_log("[PERSIST] Creating ToolCallModel")
            tool_call_objects.append(
                ToolCallModel(
                    id=tc["id"],
                    name=tool_name,
                    parameters=json.loads(tc["function"]["arguments"]),
                    result=full_result,
                    result_summary=result_summary,
                    error=tool_error,
                    approved=True,
                    nested_tool_calls=nested_calls,
                )
            )
            _debug_log("[PERSIST] ToolCallModel created")

            # Record artifact in compactor's artifact index
            self._record_artifact(tool_name, tc, full_result)

        if tool_call_objects or content:
            _debug_log(f"[PERSIST] Creating msg with {len(tool_call_objects)} tool calls")
            _debug_log(
                f"[PERSIST] content={content[:50] if content else None}, raw_content={raw_content[:50] if raw_content else None}"
            )
            metadata = {"raw_content": raw_content} if raw_content is not None else {}
            _debug_log("[PERSIST] About to create ChatMessage")
            try:
                assistant_msg = ChatMessage(
                    role=Role.ASSISTANT,
                    content=content or "",
                    metadata=metadata,
                    tool_calls=tool_call_objects,
                    # Include tracked iteration data for session persistence
                    thinking_trace=self._current_thinking_trace,
                    reasoning_content=self._current_reasoning_content,
                    token_usage=self._current_token_usage,
                )
                _debug_log("[PERSIST] ChatMessage created successfully")
            except Exception as e:
                _debug_log(f"[PERSIST] ChatMessage creation failed: {e}")
                raise

            _debug_log("[PERSIST] Calling add_message")
            self.session_manager.add_message(assistant_msg, self.config.auto_save_interval)

            _debug_log("[PERSIST] Clearing tracked values")
            # Clear tracked values after persistence
            self._current_thinking_trace = None
            self._current_reasoning_content = None
            self._current_token_usage = None

        _debug_log("[PERSIST] Completed")

        if tool_call_objects and self._tool_executor:
            outcome = "error" if any(tc.error for tc in tool_call_objects) else "success"
            self._tool_executor.record_tool_learnings(
                ctx.query, tool_call_objects, outcome, ctx.agent
            )

    def _record_artifact(
        self,
        tool_name: str,
        tool_call: dict,
        full_result: dict,
    ) -> None:
        """Record file operations in the compactor's artifact index."""
        if self._compactor is None:
            return

        try:
            args = json.loads(tool_call["function"]["arguments"])
        except (json.JSONDecodeError, KeyError):
            return

        file_path = args.get("file_path", "")
        success = full_result.get("success", False)
        if not success or not file_path:
            return

        if tool_name in ("read_file", "Read"):
            output = full_result.get("output", "")
            line_count = output.count("\n") + 1 if output else 0
            self._compactor.artifact_index.record(file_path, "read", f"{line_count} lines")
        elif tool_name in ("write_file", "Write"):
            content = args.get("content", "")
            line_count = content.count("\n") + 1 if content else 0
            self._compactor.artifact_index.record(file_path, "created", f"{line_count} lines")
        elif tool_name in ("edit_file", "Edit"):
            added = full_result.get("lines_added", 0)
            removed = full_result.get("lines_removed", 0)
            self._compactor.artifact_index.record(file_path, "modified", f"+{added}/-{removed}")

    def _add_assistant_message(self, content: str, raw_content: Optional[str]):
        """Add assistant message to session."""
        metadata = {"raw_content": raw_content} if raw_content is not None else {}
        assistant_msg = ChatMessage(
            role=Role.ASSISTANT,
            content=content,
            metadata=metadata,
            # Include tracked iteration data for session persistence
            thinking_trace=self._current_thinking_trace,
            reasoning_content=self._current_reasoning_content,
            token_usage=self._current_token_usage,
        )
        self.session_manager.add_message(assistant_msg, self.config.auto_save_interval)

        # Clear tracked values after persistence
        self._current_thinking_trace = None
        self._current_reasoning_content = None
        self._current_token_usage = None
