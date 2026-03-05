"""Abstract base class providing shared agent behavior."""

from __future__ import annotations

from abc import ABC, abstractmethod
from typing import Any, Mapping, Sequence

from swecli.core.base.interfaces.tool_interface import ToolRegistryInterface
from swecli.models.config import AppConfig

Message = Mapping[str, Any]
Response = Mapping[str, Any]


class BaseAgent(ABC):
    """Reusable foundation for agents orchestrating LLM calls."""

    def __init__(
        self,
        config: AppConfig,
        tool_registry: Union[ToolRegistryInterface, None],
        mode_manager: Any,
    ) -> None:
        self.config = config
        self.tool_registry = tool_registry
        self.mode_manager = mode_manager
        self.system_prompt = self.build_system_prompt()
        self.tool_schemas = self.build_tool_schemas()

    @abstractmethod
    def build_system_prompt(self) -> str:
        """Assemble the system prompt for downstream model calls."""

    @abstractmethod
    def build_tool_schemas(self) -> Sequence[Mapping[str, Any]]:
        """Return tool call schemas for the managed registry."""

    def refresh_tools(self) -> None:
        """Refresh prompt and tool metadata when registry contents change."""
        self.tool_schemas = self.build_tool_schemas()
        self.system_prompt = self.build_system_prompt()

    @abstractmethod
    def call_llm(
        self,
        messages: Sequence[Message],
        *,
        task_monitor: Union[Any, None] = None,
    ) -> Response:
        """Execute a language model call using the supplied messages."""

    @abstractmethod
    def run_sync(self, message: str, *, deps: Union[Any, None] = None, ui_callback: Union[Any, None] = None) -> Response:
        """Run a synchronous interaction for CLI commands."""
