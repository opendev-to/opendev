"""Handler for MCP tool invocations."""

from __future__ import annotations

from typing import Any, Optional, TYPE_CHECKING

if TYPE_CHECKING:
    from swecli.core.runtime.task_monitor import TaskMonitor


class McpToolHandler:
    """Executes MCP-backed tools via the manager."""

    def __init__(self, mcp_manager: Any) -> None:
        self._mcp_manager = mcp_manager

    def execute(
        self,
        tool_name: str,
        args: dict[str, Any],
        task_monitor: Optional["TaskMonitor"] = None,
    ) -> dict[str, Any]:
        """Execute an MCP tool.

        Args:
            tool_name: The full MCP tool name (mcp__server__tool)
            args: Tool arguments
            task_monitor: Optional task monitor for interrupt checking

        Returns:
            Result dict with success, output, and optionally error keys
        """
        # Check interrupt before execution
        if task_monitor and task_monitor.should_interrupt():
            return {
                "success": False,
                "interrupted": True,
                "error": "Interrupted",
                "output": None,
            }

        if not self._mcp_manager:
            return {
                "success": False,
                "error": "MCP manager not initialized",
                "output": None,
            }

        parts = tool_name.split("__")
        if len(parts) < 3:
            return {
                "success": False,
                "error": f"Invalid MCP tool name format: {tool_name}",
                "output": None,
            }

        server_name = parts[1]
        mcp_tool_name = "__".join(parts[2:])

        if not self._mcp_manager.is_connected(server_name):
            return {
                "success": False,
                "error": f"MCP server '{server_name}' is not connected",
                "output": None,
            }

        try:
            return self._mcp_manager.call_tool_sync(
                server_name, mcp_tool_name, args, task_monitor=task_monitor
            )
        except Exception as exc:  # noqa: BLE001
            return {
                "success": False,
                "error": f"MCP tool execution failed: {exc}",
                "output": None,
            }
