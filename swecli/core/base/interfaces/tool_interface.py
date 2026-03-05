"""Interfaces for OpenDev tool execution components."""

from __future__ import annotations

from typing import Any, Mapping, Protocol, Sequence, runtime_checkable


@runtime_checkable
class ToolInterface(Protocol):
    """Protocol for individual executable tools."""

    name: str
    description: str

    def run(self, **kwargs: Any) -> Mapping[str, Any]:
        """Execute the tool with keyword arguments."""


@runtime_checkable
class ToolRegistryInterface(Protocol):
    """Protocol capturing capabilities of the tool registry."""

    def get_schemas(self) -> Sequence[Mapping[str, Any]]:
        """Return JSON schema definitions used for tool calling."""

    def execute_tool(
        self,
        tool_name: str,
        arguments: Mapping[str, Any],
        *,
        mode_manager: Union[Any, None] = None,
        approval_manager: Union[Any, None] = None,
        undo_manager: Union[Any, None] = None,
    ) -> Mapping[str, Any]:
        """Execute a tool with the provided arguments and contextual managers."""

