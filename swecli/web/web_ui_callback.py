"""Web UI callback for agent lifecycle events.

Provides the ui_callback interface that the agent framework expects,
broadcasting events via WebSocket to the React frontend.
"""

from __future__ import annotations

import asyncio
import threading
import uuid
from typing import Any, Dict, List, Optional

from swecli.ui_textual.callback_interface import BaseUICallback
from swecli.web.logging_config import logger


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

    def on_single_agent_complete(self, tool_call_id: str, success: bool) -> None:
        """Broadcast when a single subagent finishes."""
        logger.info(f"Subagent complete: {tool_call_id} success={success}")
        self._broadcast({
            "type": "subagent_complete",
            "data": {
                "tool_call_id": tool_call_id,
                "success": success,
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

    def on_context_update(self, usage_pct: float) -> None:
        """Broadcast updated context usage percentage to the frontend."""
        self._broadcast({
            "type": "status_update",
            "data": {
                "context_usage_pct": usage_pct,
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
