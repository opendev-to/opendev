"""Runtime service that assembles core dependencies."""

from __future__ import annotations

from dataclasses import dataclass
from typing import Any, Union

from swecli.core.base.factories import AgentFactory, AgentSuite, ToolDependencies, ToolFactory
from swecli.core.base.interfaces import ConfigManagerInterface, ToolRegistryInterface
from swecli.core.runtime import ModeManager


@dataclass
class RuntimeSuite:
    """Aggregates runtime objects used by REPL and CLI layers."""

    tool_registry: ToolRegistryInterface
    agents: AgentSuite
    agent_factory: AgentFactory
    tool_factory: ToolFactory
    env_context: Any = None

    def refresh_agents(self) -> None:
        """Refresh tool schemas/prompts for both agents after tool updates."""
        self.agent_factory.refresh_tools(self.agents)

    def rebuild_agents(self, config: Any, mode_manager: Any, working_dir: Any = None) -> None:
        """Rebuild agents with new configuration (e.g., after model/provider change).

        This is needed when the model provider changes, as the HTTP client
        needs to use the new provider's API key.

        Args:
            config: New AppConfig with updated model/provider
            mode_manager: Mode manager instance
            working_dir: Working directory for file operations
        """
        self.agent_factory = AgentFactory(
            config,
            self.tool_registry,
            mode_manager,
            working_dir,
            env_context=self.env_context,
        )
        self.agents = self.agent_factory.create_agents()


class RuntimeService:
    """Builds the tool registry and agents from high-level dependencies."""

    def __init__(
        self,
        config_manager: ConfigManagerInterface,
        mode_manager: ModeManager,
    ) -> None:
        self._config_manager = config_manager
        self._mode_manager = mode_manager

    def build_suite(
        self,
        *,
        file_ops: Union[Any, None],
        write_tool: Union[Any, None],
        edit_tool: Union[Any, None],
        bash_tool: Union[Any, None],
        web_fetch_tool: Union[Any, None],
        web_search_tool: Union[Any, None] = None,
        notebook_edit_tool: Union[Any, None] = None,
        ask_user_tool: Union[Any, None] = None,
        open_browser_tool: Union[Any, None] = None,
        vlm_tool: Union[Any, None] = None,
        web_screenshot_tool: Union[Any, None] = None,
        mcp_manager: Union[Any, None] = None,
    ) -> RuntimeSuite:
        """Create a runtime suite containing the tool registry and agents."""
        # Collect environment once at startup
        from swecli.core.agents.components.prompts.environment import EnvironmentCollector

        env_context = EnvironmentCollector(
            self._config_manager.working_dir,
            self._config_manager.get_config(),
            self._config_manager,
        ).collect()

        tool_factory = ToolFactory(
            ToolDependencies(
                file_ops=file_ops,
                write_tool=write_tool,
                edit_tool=edit_tool,
                bash_tool=bash_tool,
                web_fetch_tool=web_fetch_tool,
                web_search_tool=web_search_tool,
                notebook_edit_tool=notebook_edit_tool,
                ask_user_tool=ask_user_tool,
                open_browser_tool=open_browser_tool,
                vlm_tool=vlm_tool,
                web_screenshot_tool=web_screenshot_tool,
            )
        )
        tool_registry = tool_factory.create_registry(mcp_manager=mcp_manager)

        agent_factory = AgentFactory(
            self._config_manager.get_config(),
            tool_registry,
            self._mode_manager,
            self._config_manager.working_dir,
            config_manager=self._config_manager,
            env_context=env_context,
        )
        agents = agent_factory.create_agents()

        return RuntimeSuite(
            tool_registry=tool_registry,
            agents=agents,
            agent_factory=agent_factory,
            tool_factory=tool_factory,
            env_context=env_context,
        )

    def rebuild_tool_registry(
        self,
        suite: RuntimeSuite,
        *,
        mcp_manager: Union[Any, None],
    ) -> ToolRegistryInterface:
        """Rebuild the tool registry when MCP connectivity changes."""
        registry = suite.tool_factory.create_registry(mcp_manager=mcp_manager)
        suite.tool_registry = registry
        suite.agent_factory = AgentFactory(
            self._config_manager.get_config(),
            registry,
            self._mode_manager,
            self._config_manager.working_dir,
            config_manager=self._config_manager,
            env_context=suite.env_context,
        )
        suite.agents = suite.agent_factory.create_agents()
        return registry
