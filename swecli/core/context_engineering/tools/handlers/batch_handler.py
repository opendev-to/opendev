"""Handler for batch tool execution."""

from __future__ import annotations

from typing import Any

from swecli.core.context_engineering.tools.implementations.batch_tool import BatchTool


class BatchToolHandler:
    """Handler for batch tool execution."""

    def __init__(self, tool_registry: Any) -> None:
        self._registry = tool_registry
        self._batch_tool = BatchTool()

    def handle(self, tool_args: dict[str, Any], **kwargs: Any) -> dict[str, Any]:
        """Execute batch_tool with registry dispatch.

        Args:
            tool_args: Dict with 'invocations' list and optional 'mode'
            **kwargs: Additional kwargs passed to execute_tool

        Returns:
            Result dict with success and results list
        """
        invocations = tool_args.get("invocations", [])
        mode = tool_args.get("mode", "parallel")

        return self._batch_tool.execute(
            invocations=invocations,
            mode=mode,
            tool_registry=self._registry,
            **kwargs,
        )
