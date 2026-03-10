"""SubAgent manager for creating and executing subagents."""

from __future__ import annotations

import logging
from dataclasses import dataclass, field
from enum import Enum
from typing import Any

from opendev.models.config import AppConfig

from opendev.core.agents.subagents.manager.docker import DockerMixin
from opendev.core.agents.subagents.manager.execution import ExecutionMixin
from opendev.core.agents.subagents.manager.registration import RegistrationMixin
from opendev.core.agents.subagents.specs import CompiledSubAgent

logger = logging.getLogger(__name__)


class AgentSource(str, Enum):
    """Source of an agent definition."""

    BUILTIN = "builtin"
    USER_GLOBAL = "user-global"
    PROJECT = "project"


@dataclass
class AgentConfig:
    """Configuration for an agent (builtin or custom).

    Used for building Task tool descriptions and on-demand prompt assembly.
    """

    name: str
    description: str
    tools: list[str] | str | dict[str, list[str]] = field(default_factory=list)
    system_prompt: str | None = None
    skill_path: str | None = None  # For custom agents
    source: AgentSource = AgentSource.BUILTIN
    model: str | None = None

    def get_tool_list(self, all_tools: list[str]) -> list[str]:
        """Resolve tool specification to concrete list.

        Args:
            all_tools: List of all available tool names

        Returns:
            Resolved list of tool names for this agent
        """
        if self.tools == "*":
            return all_tools
        if isinstance(self.tools, list):
            return self.tools if self.tools else all_tools
        if isinstance(self.tools, dict) and "exclude" in self.tools:
            excluded = set(self.tools["exclude"])
            return [t for t in all_tools if t not in excluded]
        return all_tools


@dataclass
class SubAgentDeps:
    """Dependencies for subagent execution."""

    mode_manager: Any
    approval_manager: Any
    undo_manager: Any
    session_manager: Any = None


class SubAgentManager(RegistrationMixin, DockerMixin, ExecutionMixin):
    """Manages subagent creation and execution.

    SubAgents are ephemeral agents that handle isolated tasks.
    They receive a task description, execute with their own context,
    and return a single result.
    """

    def __init__(
        self,
        config: AppConfig,
        tool_registry: Any,
        mode_manager: Any,
        working_dir: Any = None,
        env_context: Any = None,
    ) -> None:
        """Initialize the SubAgentManager.

        Args:
            config: Application configuration
            tool_registry: The tool registry for tool execution
            mode_manager: Mode manager for operation mode
            working_dir: Working directory for file operations
            env_context: Optional EnvironmentContext for rich system prompt
        """
        self._config = config
        self._tool_registry = tool_registry
        self._mode_manager = mode_manager
        self._working_dir = working_dir
        self._env_context = env_context
        self._hook_manager = None
        self._agents: dict[str, CompiledSubAgent] = {}
        self._all_tool_names: list[str] = self._get_all_tool_names()

    def set_hook_manager(self, hook_manager: Any) -> None:
        """Set the hook manager for SubagentStart/SubagentStop hooks.

        Args:
            hook_manager: HookManager instance
        """
        self._hook_manager = hook_manager

    def _get_all_tool_names(self) -> list[str]:
        """Get list of all available tool names from registry.

        Note: Todo tools (write_todos, update_todo, etc.) are intentionally
        excluded. Only the main agent manages task tracking - subagents
        focus purely on execution.
        """
        return [
            "read_file",
            "write_file",
            "edit_file",
            "list_files",
            "search",
            "run_command",
            "list_processes",
            "get_process_output",
            "kill_process",
            "fetch_url",
            "analyze_image",
            "capture_screenshot",
            "list_screenshots",
            "capture_web_screenshot",
            "read_pdf",
        ]
