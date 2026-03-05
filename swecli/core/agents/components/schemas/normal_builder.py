"""Tool schema builder for NORMAL mode agents.

This module contains the ToolSchemaBuilder which provides full tool access
for the main agent in NORMAL mode.
"""

from __future__ import annotations

from copy import deepcopy
from typing import Any, Sequence, Union

from .definitions import _BUILTIN_TOOL_SCHEMAS


class ToolSchemaBuilder:
    """Assemble tool schemas for NORMAL mode agents."""

    def __init__(
        self,
        tool_registry: Union[Any, None],
        allowed_tools: Union[list[str], None] = None,
    ) -> None:
        """Initialize the tool schema builder.

        Args:
            tool_registry: The tool registry for MCP and task tool schemas
            allowed_tools: Optional list of allowed tool names for filtering.
                          If None, all tools are allowed. Used by subagents
                          to restrict available tools.
        """
        self._tool_registry = tool_registry
        self._allowed_tools = allowed_tools

    def build(self, thinking_visible: bool = True) -> list[dict[str, Any]]:
        """Return tool schema definitions including MCP and task tool extensions.

        Args:
            thinking_visible: Deprecated parameter (kept for API compatibility).
                             Thinking is now a separate pre-processing phase,
                             not a tool the model calls.

        Returns:
            List of tool schemas. If allowed_tools was set, only returns
            schemas for tools in that list.
        """
        # Get all builtin tool schemas
        schemas: list[dict[str, Any]] = deepcopy(_BUILTIN_TOOL_SCHEMAS)

        # Filter to allowed tools if specified
        if self._allowed_tools is not None:
            schemas = [
                schema for schema in schemas if schema["function"]["name"] in self._allowed_tools
            ]

        # Add task tool schema if subagent manager is configured
        # Only add if spawn_subagent is in allowed_tools or no filter
        if self._allowed_tools is None or "spawn_subagent" in self._allowed_tools:
            task_schema = self._build_task_schema()
            if task_schema:
                schemas.append(task_schema)

        # Add MCP tool schemas (only those matching allowed_tools)
        mcp_schemas = self._build_mcp_schemas()
        if mcp_schemas:
            if self._allowed_tools is not None:
                # Filter MCP schemas to only allowed tools
                allowed_set = set(self._allowed_tools)
                mcp_schemas = [
                    schema for schema in mcp_schemas if schema["function"]["name"] in allowed_set
                ]
            schemas.extend(mcp_schemas)
        return schemas

    def _build_task_schema(self) -> dict[str, Any] | None:
        """Build task tool schema with available subagent types."""
        if not self._tool_registry:
            return None

        subagent_manager = getattr(self._tool_registry, "_subagent_manager", None)
        if not subagent_manager:
            return None

        from swecli.core.agents.subagents.task_tool import create_task_tool_schema

        return create_task_tool_schema(subagent_manager)

    def _build_mcp_schemas(self) -> Sequence[dict[str, Any]]:
        """Build MCP tool schemas - only returns discovered tools for token efficiency.

        MCP tools are NOT loaded by default. The agent must use search_tools() to
        discover them first, which adds them to the discovered set.
        """
        if not self._tool_registry or not getattr(self._tool_registry, "mcp_manager", None):
            return []

        # Only return schemas for tools that have been "discovered" via search_tools
        discovered_tools = self._tool_registry.get_discovered_mcp_tools()
        schemas: list[dict[str, Any]] = []
        for tool in discovered_tools:
            schemas.append(
                {
                    "type": "function",
                    "function": {
                        "name": tool.get("name"),
                        "description": tool.get("description"),
                        "parameters": tool.get("input_schema", {}),
                    },
                }
            )
        return schemas
