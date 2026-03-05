"""Hook manager — orchestrates hook execution for lifecycle events."""

from __future__ import annotations

import logging
from concurrent.futures import ThreadPoolExecutor
from dataclasses import dataclass, field
from typing import Any, Optional

from swecli.core.hooks.executor import HookCommandExecutor, HookResult
from swecli.core.hooks.models import HookConfig, HookEvent

logger = logging.getLogger(__name__)


@dataclass
class HookOutcome:
    """Aggregated outcome from running all hooks for an event."""

    blocked: bool = False
    block_reason: str = ""
    results: list[HookResult] = field(default_factory=list)
    additional_context: Optional[str] = None
    updated_input: Optional[dict[str, Any]] = None
    permission_decision: Optional[str] = None
    decision: Optional[str] = None


class HookManager:
    """Orchestrates hook execution for lifecycle events.

    Takes a snapshot of HookConfig at init. Mid-session changes to
    settings.json are not reflected (security: prevents config TOCTOU).
    """

    def __init__(
        self,
        config: HookConfig,
        session_id: str = "",
        cwd: str = "",
    ) -> None:
        self._config = config
        self._session_id = session_id
        self._cwd = cwd
        self._executor = HookCommandExecutor()
        self._async_pool: Optional[ThreadPoolExecutor] = None

    def has_hooks_for(self, event: HookEvent) -> bool:
        """Fast check: are there hooks registered for this event?"""
        return self._config.has_hooks_for(event)

    def run_hooks(
        self,
        event: HookEvent,
        match_value: Optional[str] = None,
        event_data: Optional[dict[str, Any]] = None,
    ) -> HookOutcome:
        """Run all matching hooks for an event.

        Hooks execute sequentially. Short-circuits on block (exit code 2).

        Args:
            event: The lifecycle event.
            match_value: Value to test against matcher regex (e.g., tool name).
            event_data: Additional event-specific data for stdin payload.

        Returns:
            HookOutcome aggregating all results.
        """
        outcome = HookOutcome()

        matchers = self._config.get_matchers(event)
        if not matchers:
            return outcome

        for matcher in matchers:
            if not matcher.matches(match_value):
                continue

            stdin_data = self._build_stdin(event, match_value, event_data)

            for command in matcher.hooks:
                result = self._executor.execute(command, stdin_data)
                outcome.results.append(result)

                if result.should_block:
                    outcome.blocked = True
                    parsed = result.parse_json_output()
                    outcome.block_reason = parsed.get(
                        "reason", result.stderr.strip() or "Blocked by hook"
                    )
                    outcome.decision = parsed.get("decision")
                    return outcome

                if result.success:
                    parsed = result.parse_json_output()
                    if parsed.get("additionalContext"):
                        outcome.additional_context = parsed["additionalContext"]
                    if parsed.get("updatedInput"):
                        outcome.updated_input = parsed["updatedInput"]
                    if parsed.get("permissionDecision"):
                        outcome.permission_decision = parsed["permissionDecision"]
                    if parsed.get("decision"):
                        outcome.decision = parsed["decision"]

                elif not result.success and result.error:
                    logger.warning(
                        "Hook command error (event=%s): %s", event.value, result.error
                    )

        return outcome

    def run_hooks_async(
        self,
        event: HookEvent,
        match_value: Optional[str] = None,
        event_data: Optional[dict[str, Any]] = None,
    ) -> None:
        """Fire-and-forget hook execution in background thread.

        Used for events where we don't need to wait for the result
        (e.g., PostToolUse logging hooks).

        Args:
            event: The lifecycle event.
            match_value: Value to test against matcher regex.
            event_data: Additional event-specific data.
        """
        if not self.has_hooks_for(event):
            return

        if self._async_pool is None:
            self._async_pool = ThreadPoolExecutor(max_workers=2, thread_name_prefix="hook-async")

        self._async_pool.submit(self.run_hooks, event, match_value, event_data)

    def shutdown(self) -> None:
        """Shut down the async thread pool."""
        if self._async_pool is not None:
            self._async_pool.shutdown(wait=False)
            self._async_pool = None

    def _build_stdin(
        self,
        event: HookEvent,
        match_value: Optional[str],
        event_data: Optional[dict[str, Any]],
    ) -> dict[str, Any]:
        """Build the JSON payload sent to hook commands on stdin.

        Follows the Claude Code hook protocol:
        - session_id: Current session ID
        - cwd: Current working directory
        - hook_event_name: The event name (e.g., "PreToolUse")
        - tool_name: Tool name (for tool events)
        - tool_input: Tool input arguments (for PreToolUse)
        - tool_response: Tool response (for PostToolUse/PostToolUseFailure)

        Args:
            event: The lifecycle event.
            match_value: The matcher value (tool name, agent type, etc.).
            event_data: Additional event-specific data.

        Returns:
            Dict to be JSON-serialized and piped to the hook command.
        """
        payload: dict[str, Any] = {
            "session_id": self._session_id,
            "cwd": self._cwd,
            "hook_event_name": event.value,
        }

        # Tool events include tool_name
        if event in (
            HookEvent.PRE_TOOL_USE,
            HookEvent.POST_TOOL_USE,
            HookEvent.POST_TOOL_USE_FAILURE,
        ):
            payload["tool_name"] = match_value or ""

        # Subagent events include agent type
        if event in (HookEvent.SUBAGENT_START, HookEvent.SUBAGENT_STOP):
            payload["agent_type"] = match_value or ""

        # Session start includes startup type
        if event == HookEvent.SESSION_START:
            payload["startup_type"] = match_value or "startup"

        # PreCompact includes trigger type
        if event == HookEvent.PRE_COMPACT:
            payload["trigger"] = match_value or "auto"

        # Merge event-specific data
        if event_data:
            # Standard fields from event_data
            if "tool_input" in event_data:
                payload["tool_input"] = event_data["tool_input"]
            if "tool_response" in event_data:
                payload["tool_response"] = event_data["tool_response"]
            if "user_prompt" in event_data:
                payload["user_prompt"] = event_data["user_prompt"]
            if "agent_task" in event_data:
                payload["agent_task"] = event_data["agent_task"]
            if "agent_result" in event_data:
                payload["agent_result"] = event_data["agent_result"]
            # Pass through any other data
            for key, value in event_data.items():
                if key not in payload:
                    payload[key] = value

        return payload
