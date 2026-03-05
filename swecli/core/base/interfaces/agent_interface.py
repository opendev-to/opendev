"""Interfaces describing agent behavior within OpenDev core."""

from __future__ import annotations

from typing import Any, Mapping, Protocol, Sequence, Union, runtime_checkable


Message = Mapping[str, Any]
Response = Mapping[str, Any]


@runtime_checkable
class AgentInterface(Protocol):
    """Protocol that all conversational agents should satisfy."""

    system_prompt: str
    tool_schemas: Sequence[Mapping[str, Any]]

    def refresh_tools(self) -> None:
        """Reload tool metadata (schemas and prompts)."""

    def call_llm(
        self,
        messages: Sequence[Message],
        *,
        task_monitor: Union[Any, None] = None,
    ) -> Response:
        """Execute an LLM call with the provided chat history."""

    def run_sync(self, message: str, *, deps: Union[Any, None] = None) -> Response:
        """Convenience wrapper used by synchronous command handlers."""
