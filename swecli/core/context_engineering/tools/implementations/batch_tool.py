"""Batch tool for executing multiple tool invocations in parallel or serial."""

from __future__ import annotations

import concurrent.futures
import logging
from typing import Any, Union

from swecli.core.context_engineering.tools.implementations.base import BaseTool

logger = logging.getLogger(__name__)

MAX_PARALLEL_WORKERS = 5


class BatchTool(BaseTool):
    """Execute multiple tool invocations in parallel or serial."""

    @property
    def name(self) -> str:
        return "batch_tool"

    @property
    def description(self) -> str:
        return "Execute multiple tool calls in parallel or serial order."

    def execute(
        self,
        invocations: list[dict[str, Any]],
        mode: str = "parallel",
        tool_registry: Any = None,
        **kwargs: Any,
    ) -> dict[str, Any]:
        """Execute multiple tool calls.

        Args:
            invocations: List of {tool, input} dicts
            mode: "parallel" (concurrent) or "serial" (sequential)
            tool_registry: Registry to dispatch calls to

        Returns:
            Dict with success status and list of results
        """
        if not invocations:
            return {"success": True, "results": []}

        if tool_registry is None:
            return {"success": False, "error": "tool_registry is required", "results": []}

        if mode == "parallel":
            results = self._execute_parallel(invocations, tool_registry, **kwargs)
        else:
            results = self._execute_serial(invocations, tool_registry, **kwargs)

        return {"success": True, "results": results}

    def _execute_parallel(
        self,
        invocations: list[dict[str, Any]],
        tool_registry: Any,
        **kwargs: Any,
    ) -> list[dict[str, Any]]:
        """Execute invocations concurrently with ThreadPoolExecutor."""
        results: list[Union[dict[str, Any], None]] = [None] * len(invocations)

        def _run_one(index: int, inv: dict[str, Any]) -> tuple[int, dict[str, Any]]:
            tool_name = inv.get("tool", "")
            tool_input = inv.get("input", {})
            try:
                result = tool_registry.execute_tool(tool_name, tool_input, **kwargs)
                return index, {
                    "tool": tool_name,
                    "success": result.get("success", False),
                    "output": result.get("output", result.get("error", "")),
                }
            except Exception as exc:
                return index, {"tool": tool_name, "success": False, "output": str(exc)}

        with concurrent.futures.ThreadPoolExecutor(
            max_workers=min(MAX_PARALLEL_WORKERS, len(invocations))
        ) as executor:
            futures = [executor.submit(_run_one, i, inv) for i, inv in enumerate(invocations)]
            for future in concurrent.futures.as_completed(futures):
                idx, result = future.result()
                results[idx] = result

        return [r for r in results if r is not None]

    def _execute_serial(
        self,
        invocations: list[dict[str, Any]],
        tool_registry: Any,
        **kwargs: Any,
    ) -> list[dict[str, Any]]:
        """Execute invocations sequentially in order."""
        results: list[dict[str, Any]] = []
        for inv in invocations:
            tool_name = inv.get("tool", "")
            tool_input = inv.get("input", {})
            try:
                result = tool_registry.execute_tool(tool_name, tool_input, **kwargs)
                results.append(
                    {
                        "tool": tool_name,
                        "success": result.get("success", False),
                        "output": result.get("output", result.get("error", "")),
                    }
                )
            except Exception as exc:
                results.append({"tool": tool_name, "success": False, "output": str(exc)})
        return results
