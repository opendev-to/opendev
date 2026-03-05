"""Factory for assembling the tool registry and related primitives."""

from __future__ import annotations

from dataclasses import dataclass
from typing import Any, Union

from swecli.core.context_engineering.tools import ToolRegistry


@dataclass
class ToolDependencies:
    """Value object capturing tool-level dependencies."""

    file_ops: Union[Any, None]
    write_tool: Union[Any, None]
    edit_tool: Union[Any, None]
    bash_tool: Union[Any, None]
    web_fetch_tool: Union[Any, None]
    web_search_tool: Union[Any, None] = None
    notebook_edit_tool: Union[Any, None] = None
    ask_user_tool: Union[Any, None] = None
    open_browser_tool: Union[Any, None] = None
    vlm_tool: Union[Any, None] = None
    web_screenshot_tool: Union[Any, None] = None


class ToolFactory:
    """Creates tool registries with consistent wiring."""

    def __init__(self, dependencies: ToolDependencies) -> None:
        self._deps = dependencies

    def create_registry(self, *, mcp_manager: Union[Any, None] = None) -> ToolRegistry:
        """Instantiate a `ToolRegistry` with the configured dependencies."""
        registry = ToolRegistry(
            file_ops=self._deps.file_ops,
            write_tool=self._deps.write_tool,
            edit_tool=self._deps.edit_tool,
            bash_tool=self._deps.bash_tool,
            web_fetch_tool=self._deps.web_fetch_tool,
            web_search_tool=self._deps.web_search_tool,
            notebook_edit_tool=self._deps.notebook_edit_tool,
            ask_user_tool=self._deps.ask_user_tool,
            open_browser_tool=self._deps.open_browser_tool,
            vlm_tool=self._deps.vlm_tool,
            web_screenshot_tool=self._deps.web_screenshot_tool,
            mcp_manager=mcp_manager,
        )
        return registry
