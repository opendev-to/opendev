"""Web UI callback for agent lifecycle events.

Provides the ui_callback interface that the agent framework expects,
broadcasting events via WebSocket to the React frontend.
"""

from __future__ import annotations

import asyncio
import threading
import uuid
from typing import Any, Dict

from opendev.models.message import ToolCall
from opendev.ui_textual.callback_interface import BaseUICallback
from opendev.web.logging_config import logger


class WebUICallback(BaseUICallback):
    """UI callback for the web path.

    Broadcasts plan approval, subagent lifecycle, and tool events via
    WebSocket. Plan approval follows the same broadcast-wait-resolve
    pattern as WebAskUserManager and WebApprovalManager.
    """

    def __init__(
        self,
        ws_manager: Any,
        loop: asyncio.AbstractEventLoop,
        session_id: str,
        state: Any,
    ) -> None:
        self.ws_manager = ws_manager
        self.loop = loop
        self.session_id = session_id
        self.state = state
        self._pending_nested_calls: list[ToolCall] = []

    # ------------------------------------------------------------------
    # Plan approval (used by PresentPlanTool via registry)
    # ------------------------------------------------------------------

    def display_plan_content(self, plan_content: str) -> None:
        """Broadcast plan content for display before the approval dialog."""
        self._broadcast({
            "type": "plan_content",
            "data": {
                "plan_content": plan_content,
                "session_id": self.session_id,
            },
        })

    def request_plan_approval(
        self, plan_content: str = "", allowed_prompts: Any = None
    ) -> Dict[str, str]:
        """Broadcast plan_approval_required and block until the user responds.

        Returns:
            Dict with 'action' ("approve_auto"|"approve"|"modify"|"reject")
            and optional 'feedback'.
        """
        request_id = str(uuid.uuid4())
        done_event = threading.Event()

        approval_request = {
            "request_id": request_id,
            "plan_content": plan_content,
            "session_id": self.session_id,
        }

        # Store pending approval in shared state
        self.state.add_pending_plan_approval(
            request_id, approval_request, session_id=self.session_id, event=done_event
        )

        # Broadcast to frontend
        logger.info(f"Requesting plan approval: {request_id}")
        self._broadcast({
            "type": "plan_approval_required",
            "data": approval_request,
        })

        # Block until user responds (or timeout)
        wait_timeout = 600  # 10 minutes
        if not done_event.wait(timeout=wait_timeout):
            logger.warning(f"Plan approval {request_id} timed out")
            self.state.clear_plan_approval(request_id)
            return {"action": "reject", "feedback": "Timed out waiting for approval"}

        pending = self.state.get_pending_plan_approval(request_id)
        if not pending:
            return {"action": "reject", "feedback": ""}

        action = pending.get("action", "reject")
        feedback = pending.get("feedback", "")
        self.state.clear_plan_approval(request_id)

        logger.info(f"Plan approval {request_id} resolved: action={action}")

        # Broadcast status_update to reset mode to normal after plan approval
        if action in ("approve_auto", "approve"):
            self._broadcast({
                "type": "status_update",
                "data": {"mode": "normal", "session_id": self.session_id},
            })

        return {"action": action, "feedback": feedback}

    # ------------------------------------------------------------------
    # Tool lifecycle (WebSocketToolBroadcaster handles the main events,
    # but we handle special post-tool events here)
    # ------------------------------------------------------------------

    def on_tool_call(
        self, tool_name: str, tool_args: Dict[str, Any], tool_call_id: str = ""
    ) -> None:
        # No-op: WebSocketToolBroadcaster already handles tool_call broadcasts
        pass

    def on_tool_result(
        self, tool_name: str, tool_args: Dict[str, Any], result: Any, tool_call_id: str = ""
    ) -> None:
        if tool_name == "task_complete":
            self._broadcast({
                "type": "task_completed",
                "data": {
                    "summary": result.get("output", "") if isinstance(result, dict) else str(result),
                    "session_id": self.session_id,
                },
            })

    # ------------------------------------------------------------------
    # Subagent lifecycle (used by SubAgentManager)
    # ------------------------------------------------------------------

    def on_single_agent_start(
        self, agent_type: str, description: str, tool_call_id: str
    ) -> None:
        """Broadcast when a single subagent begins executing."""
        logger.info(f"Subagent start: {agent_type} ({tool_call_id})")
        self._broadcast({
            "type": "subagent_start",
            "data": {
                "agent_type": agent_type,
                "description": description,
                "tool_call_id": tool_call_id,
                "session_id": self.session_id,
            },
        })

    def on_single_agent_complete(
        self, tool_call_id: str, success: bool, failure_reason: str = ""
    ) -> None:
        """Broadcast when a single subagent finishes."""
        logger.info(f"Subagent complete: {tool_call_id} success={success}")
        self._broadcast({
            "type": "subagent_complete",
            "data": {
                "tool_call_id": tool_call_id,
                "success": success,
                "failure_reason": failure_reason,
                "session_id": self.session_id,
            },
        })

    def on_parallel_agents_start(self, agent_infos: list) -> None:
        """Broadcast when parallel subagents begin."""
        self._broadcast({
            "type": "parallel_agents_start",
            "data": {
                "agents": agent_infos,
                "session_id": self.session_id,
            },
        })

    def on_parallel_agent_complete(self, tool_call_id: str, success: bool) -> None:
        """Broadcast when one of the parallel agents finishes."""
        self._broadcast({
            "type": "subagent_complete",
            "data": {
                "tool_call_id": tool_call_id,
                "success": success,
                "session_id": self.session_id,
            },
        })

    def on_parallel_agents_done(self) -> None:
        """Broadcast when all parallel agents have finished."""
        self._broadcast({
            "type": "parallel_agents_done",
            "data": {"session_id": self.session_id},
        })

    # ------------------------------------------------------------------
    # Cost tracking
    # ------------------------------------------------------------------

    def on_cost_update(self, total_cost_usd: float) -> None:
        """Broadcast updated session cost to the frontend."""
        self._broadcast({
            "type": "status_update",
            "data": {
                "session_cost": total_cost_usd,
                "session_id": self.session_id,
            },
        })

    # ------------------------------------------------------------------
    # Context usage
    # ------------------------------------------------------------------

    def on_context_usage(self, usage_pct: float) -> None:
        """Broadcast updated context usage percentage to the frontend."""
        self._broadcast({
            "type": "status_update",
            "data": {
                "context_usage_pct": usage_pct,
                "session_id": self.session_id,
            },
        })

    # ------------------------------------------------------------------
    # Thinking lifecycle
    # ------------------------------------------------------------------

    def on_thinking_start(self) -> None:
        """Broadcast that the agent has started thinking."""
        self._broadcast({
            "type": "status_update",
            "data": {"thinking_active": True, "session_id": self.session_id},
        })

    def on_thinking_complete(self) -> None:
        """Broadcast that the agent has finished thinking."""
        self._broadcast({
            "type": "status_update",
            "data": {"thinking_active": False, "session_id": self.session_id},
        })

    def on_thinking(self, content: str) -> None:
        """Broadcast thinking content as a thinking_block."""
        if not content or not content.strip():
            return
        self._broadcast({
            "type": "thinking_block",
            "data": {
                "content": content.strip(),
                "level": "Medium",
                "session_id": self.session_id,
            },
        })

    # ------------------------------------------------------------------
    # Progress indicators
    # ------------------------------------------------------------------

    def on_progress_start(self, message: str) -> None:
        """Broadcast progress start event."""
        self._broadcast({
            "type": "progress",
            "data": {
                "status": "start",
                "message": message,
                "session_id": self.session_id,
            },
        })

    def on_progress_update(self, message: str) -> None:
        """Broadcast progress update event."""
        self._broadcast({
            "type": "progress",
            "data": {
                "status": "update",
                "message": message,
                "session_id": self.session_id,
            },
        })

    def on_progress_complete(self, message: str = "", success: bool = True) -> None:
        """Broadcast progress complete event."""
        self._broadcast({
            "type": "progress",
            "data": {
                "status": "complete",
                "message": message,
                "success": success,
                "session_id": self.session_id,
            },
        })

    # ------------------------------------------------------------------
    # Nested tool calls (subagent tools)
    # ------------------------------------------------------------------

    def on_nested_tool_call(
        self,
        tool_name: str,
        tool_args: Dict[str, Any],
        depth: int = 1,
        parent: str = "",
    ) -> None:
        """Broadcast nested tool call from a subagent."""
        self._broadcast({
            "type": "nested_tool_call",
            "data": {
                "tool_name": tool_name,
                "arguments": tool_args,
                "depth": depth,
                "parent": parent,
                "session_id": self.session_id,
            },
        })

    def on_nested_tool_result(
        self,
        tool_name: str,
        tool_args: Dict[str, Any],
        result: Any,
        depth: int = 1,
        parent: str = "",
    ) -> None:
        """Broadcast nested tool result from a subagent."""
        # Summarize result to avoid sending huge payloads
        summary = ""
        if isinstance(result, dict):
            summary = result.get("output", str(result))[:200]
        elif isinstance(result, str):
            summary = result[:200]
        else:
            summary = str(result)[:200]

        self._broadcast({
            "type": "nested_tool_result",
            "data": {
                "tool_name": tool_name,
                "depth": depth,
                "parent": parent,
                "success": not (isinstance(result, dict) and result.get("success") is False),
                "summary": summary,
                "session_id": self.session_id,
            },
        })

        # Collect for session persistence (mirrors TUI tool_display.py:508)
        self._pending_nested_calls.append(
            ToolCall(
                id=f"nested_{len(self._pending_nested_calls)}",
                name=tool_name,
                parameters=tool_args,
                result=result,
            )
        )

    def get_and_clear_nested_calls(self) -> list[ToolCall]:
        """Return collected nested calls and clear the buffer.

        Called by SessionPersistenceMixin after spawn_subagent completes
        to attach nested calls to the ToolCall.
        """
        calls = self._pending_nested_calls
        self._pending_nested_calls = []
        return calls

    # ------------------------------------------------------------------
    # Assistant message
    # ------------------------------------------------------------------

    def on_assistant_message(self, content: str) -> None:
        """Broadcast assistant message content as a message_chunk."""
        if not content:
            return
        self._broadcast({
            "type": "message_chunk",
            "data": {
                "content": content,
                "session_id": self.session_id,
            },
        })

    # ------------------------------------------------------------------
    # Debug
    # ------------------------------------------------------------------

    def on_debug(self, message: str, prefix: str = "DEBUG") -> None:
        """Log debug messages (not broadcast to frontend)."""
        logger.debug(f"[{prefix}] {message}")

    # ------------------------------------------------------------------
    # Thinking / Critique
    # ------------------------------------------------------------------

    def on_critique(self, content: str) -> None:
        """Broadcast critique content as a thinking_block to the frontend."""
        if not content or not content.strip():
            return
        self._broadcast({
            "type": "thinking_block",
            "data": {
                "content": content.strip(),
                "level": "High",
                "session_id": self.session_id,
            },
        })

    # ------------------------------------------------------------------
    # Interrupt
    # ------------------------------------------------------------------

    def on_interrupt(self, context: Any = None) -> None:
        self._broadcast({
            "type": "status_update",
            "data": {"interrupted": True, "session_id": self.session_id},
        })

    def mark_interrupt_shown(self) -> None:
        pass

    # ------------------------------------------------------------------
    # Internal helper
    # ------------------------------------------------------------------

    def _broadcast(self, message: Dict[str, Any]) -> None:
        """Schedule a broadcast on the event loop (non-blocking from agent thread)."""
        try:
            future = asyncio.run_coroutine_threadsafe(
                self.ws_manager.broadcast(message),
                self.loop,
            )
            future.result(timeout=5)
        except Exception as e:
            logger.error(f"WebUICallback broadcast failed: {e}")
