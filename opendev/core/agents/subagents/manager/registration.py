"""Registration mixin for SubAgentManager."""

from __future__ import annotations

import logging
from typing import TYPE_CHECKING, Any

from opendev.core.agents.prompts import get_reminder
from opendev.models.config import AppConfig

from opendev.core.agents.subagents.specs import CompiledSubAgent, SubAgentSpec

if TYPE_CHECKING:
    from opendev.core.agents.subagents.manager.manager import AgentConfig

logger = logging.getLogger(__name__)


class RegistrationMixin:
    """Mixin providing subagent registration and config building."""

    # Declared for type checking — set by SubAgentManager.__init__
    _config: AppConfig
    _tool_registry: Any
    _mode_manager: Any
    _working_dir: Any
    _env_context: Any
    _agents: dict[str, CompiledSubAgent]
    _all_tool_names: list[str]

    def register_subagent(self, spec: SubAgentSpec) -> None:
        """Register a subagent from specification.

        Args:
            spec: The subagent specification
        """
        from opendev.core.agents import MainAgent

        # Create a filtered tool registry if tools are specified
        tool_names = spec.get("tools", self._all_tool_names)

        # Create the subagent instance with tool filtering
        agent = MainAgent(
            config=self._get_subagent_config(spec),
            tool_registry=self._tool_registry,
            mode_manager=self._mode_manager,
            working_dir=self._working_dir,
            allowed_tools=tool_names,  # Pass tool filtering to agent
            env_context=self._env_context,
        )

        # Override system prompt for subagent
        agent._subagent_system_prompt = spec["system_prompt"]

        self._agents[spec["name"]] = CompiledSubAgent(
            name=spec["name"],
            description=spec["description"],
            agent=agent,
            tool_names=tool_names,
        )

    def _get_subagent_config(self, spec: SubAgentSpec) -> AppConfig:
        """Create config for subagent, potentially with model override."""
        if "model" in spec and spec["model"]:
            # Create a copy with model override
            return AppConfig(
                model=spec["model"],
                temperature=self._config.temperature,
                max_tokens=self._config.max_tokens,
                api_key=self._config.api_key,
                api_base_url=self._config.api_base_url,
            )
        return self._config

    def register_defaults(self) -> None:
        """Register all default subagents."""
        from opendev.core.agents.subagents.agents import ALL_SUBAGENTS

        for spec in ALL_SUBAGENTS:
            self.register_subagent(spec)

    def get_agent_configs(self) -> list[AgentConfig]:
        """Get all agent configurations for Task tool description.

        Returns:
            List of AgentConfig for all registered agents (builtin and custom)
        """
        from opendev.core.agents.subagents.agents import ALL_SUBAGENTS
        from opendev.core.agents.subagents.manager.manager import AgentConfig, AgentSource

        configs = []
        for spec in ALL_SUBAGENTS:
            config = AgentConfig(
                name=spec["name"],
                description=spec["description"],
                tools=spec.get("tools", []),
                system_prompt=spec.get("system_prompt"),
                source=AgentSource.BUILTIN,
                model=spec.get("model"),
            )
            configs.append(config)

        # Include custom agents (added via register_custom_agents)
        for name, compiled in self._agents.items():
            # Skip if already in configs (builtin)
            if any(c.name == name for c in configs):
                continue
            # This is a custom agent
            config = AgentConfig(
                name=name,
                description=compiled["description"],
                tools=compiled.get("tool_names", []),
                source=AgentSource.USER_GLOBAL,  # Will be updated by register_custom_agents
            )
            configs.append(config)

        return configs

    def build_task_tool_description(self) -> str:
        """Build spawn_subagent tool description from registered agents.

        Returns:
            Formatted description string for the spawn_subagent tool
        """
        lines = [
            "Spawn a specialized subagent to handle a specific task.",
            "",
            "Available agent types:",
        ]
        for config in self.get_agent_configs():
            lines.append(f"- **{config.name}**: {config.description}")
        lines.append("")
        lines.append("Use this tool when you need specialized capabilities or ")
        lines.append("want to delegate complex tasks to a focused agent.")
        return "\n".join(lines)

    def get_subagent(self, name: str) -> CompiledSubAgent | None:
        """Get a registered subagent by name.

        Args:
            name: The subagent name

        Returns:
            The compiled subagent or None if not found
        """
        return self._agents.get(name)

    def get_available_types(self) -> list[str]:
        """Get list of available subagent type names.

        Returns:
            List of registered subagent names
        """
        return list(self._agents.keys())

    def get_descriptions(self) -> dict[str, str]:
        """Get descriptions for all registered subagents.

        Returns:
            Dict mapping subagent name to description
        """
        return {name: agent["description"] for name, agent in self._agents.items()}

    def register_custom_agents(self, custom_agents: list[dict]) -> None:
        """Register custom agents from config files.

        Custom agents can be defined in:
        - ~/.opendev/agents.json or <project>/.opendev/agents.json (JSON format)
        - ~/.opendev/agents/*.md or <project>/.opendev/agents/*.md (Claude Code markdown format)

        Each agent definition can specify:
        - name: Unique agent name (required)
        - description: Human-readable description (optional)
        - tools: List of tool names, "*" for all, or {"exclude": [...]} (optional)
        - skillPath: Path to skill file to use as system prompt (optional, JSON format)
        - _system_prompt: Direct system prompt content (markdown format)
        - model: Model override for this agent (optional)

        Args:
            custom_agents: List of agent definitions from config files
        """
        from opendev.core.agents.subagents.manager.manager import AgentConfig, AgentSource

        for agent_def in custom_agents:
            name = agent_def.get("name")
            if not name:
                logger.warning("Skipping custom agent without name")
                continue

            # Skip if already registered (builtin takes priority)
            if name in self._agents:
                logger.debug(f"Custom agent '{name}' shadows builtin agent, skipping")
                continue

            # Build AgentConfig from definition
            config = AgentConfig(
                name=name,
                description=agent_def.get("description", f"Custom agent: {name}"),
                tools=agent_def.get("tools", "*"),
                skill_path=agent_def.get("skillPath"),
                source=(
                    AgentSource.USER_GLOBAL
                    if agent_def.get("_source") == "user-global"
                    else AgentSource.PROJECT
                ),
                model=agent_def.get("model"),
            )

            # Check for direct system prompt (from markdown agent files)
            # or build from skill file
            if "_system_prompt" in agent_def:
                system_prompt = agent_def["_system_prompt"]
            else:
                system_prompt = self._build_custom_agent_prompt(config)

            # Create SubAgentSpec for registration
            spec: SubAgentSpec = {
                "name": name,
                "description": config.description,
                "system_prompt": system_prompt,
                "tools": config.get_tool_list(self._all_tool_names),
            }

            if config.model:
                spec["model"] = config.model

            # Register the agent
            self.register_subagent(spec)
            logger.info(f"Registered custom agent: {name} (source: {config.source.value})")

    def _build_custom_agent_prompt(self, config: AgentConfig) -> str:
        """Build system prompt for a custom agent.

        Args:
            config: AgentConfig with skill_path or other config

        Returns:
            System prompt string
        """
        if config.skill_path:
            # Load skill content from file
            from pathlib import Path

            skill_path = Path(config.skill_path).expanduser()
            if skill_path.exists():
                try:
                    content = skill_path.read_text(encoding="utf-8")
                    # Strip YAML frontmatter if present
                    if content.startswith("---"):
                        import re

                        content = re.sub(r"^---\n.*?\n---\n*", "", content, flags=re.DOTALL)
                    return content
                except Exception as e:
                    logger.warning(f"Failed to load skill file {skill_path}: {e}")

        # Default prompt for custom agents
        return get_reminder(
            "generators/custom_agent_default", name=config.name, description=config.description
        )
