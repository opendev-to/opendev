"""System prompt builders for OpenDev agents."""

from __future__ import annotations

from typing import Any, TYPE_CHECKING

from swecli.core.agents.prompts import load_prompt

if TYPE_CHECKING:
    from swecli.core.skills import SkillLoader
    from swecli.core.agents.subagents.manager import SubAgentManager
    from .environment import EnvironmentContext


class BasePromptBuilder:
    """Base class for system prompt builders.

    Subclasses set _core_template to their core prompt template path.
    Override any method to customize behavior for specific modes.
    """

    _core_template: str = ""

    def __init__(
        self,
        tool_registry: Any | None,
        working_dir: Any | None = None,
        skill_loader: "SkillLoader | None" = None,
        subagent_manager: "SubAgentManager | None" = None,
        env_context: "EnvironmentContext | None" = None,
    ) -> None:
        self._tool_registry = tool_registry
        self._working_dir = working_dir
        self._skill_loader = skill_loader
        self._subagent_manager = subagent_manager
        self._env_context = env_context

    def build(self) -> str:
        """Build complete system prompt from components.

        Uses modular composition system for flexible, conditional loading.
        Falls back to core identity if modular files not available.
        """
        # Try modular composition first
        try:
            modular_prompt = self._build_modular_prompt()
            if modular_prompt:
                # Add dynamic sections (environment, project, skills, MCP)
                sections = [
                    modular_prompt,
                    self._build_environment(),
                    self._build_project_instructions(),
                    self._build_skills_index(),
                    self._build_mcp_section() or self._build_mcp_config_section(),
                ]
                return "\n\n".join(filter(None, sections))
        except Exception:
            # Fall back to monolithic if modular fails
            pass

        # Fallback: use monolithic core template
        sections = [
            self._build_core_identity(),
            self._build_environment(),
            self._build_project_instructions(),
            self._build_skills_index(),
            self._build_mcp_section() or self._build_mcp_config_section(),
        ]
        return "\n\n".join(filter(None, sections))

    def build_two_part(self) -> tuple[str, str]:
        """Build system prompt split into (stable, dynamic) for prompt caching.

        The stable part contains core identity, policies, tool descriptions --
        content that does not change between turns and can be cached by
        providers that support it (e.g. Anthropic).

        The dynamic part contains session-specific context like environment
        details, project instructions, and scratchpad that may change.

        Returns:
            Tuple of (stable_prompt, dynamic_prompt).  Falls back to
            ``(self.build(), "")`` if the modular composer is unavailable.
        """
        try:
            stable, dynamic = self._build_modular_two_part()
            if stable or dynamic:
                # Dynamic sections from the builder (environment, project, etc.)
                extra_dynamic = [
                    self._build_environment(),
                    self._build_project_instructions(),
                    self._build_skills_index(),
                    self._build_mcp_section() or self._build_mcp_config_section(),
                ]
                extra = "\n\n".join(filter(None, extra_dynamic))
                if extra:
                    dynamic = f"{dynamic}\n\n{extra}" if dynamic else extra
                return stable, dynamic
        except Exception:
            pass

        # Fallback: everything is stable (single block)
        return self.build(), ""

    def _build_modular_prompt(self) -> str:
        """Build prompt from modular sections using PromptComposer.

        Uses self._core_template to derive the sections directory, so each
        mode (system/main, system/thinking) loads its own purpose-built sections.

        Returns:
            Composed prompt from modular sections, or empty string if not available
        """
        from pathlib import Path
        from swecli.core.agents.prompts.composition import create_composer
        from swecli.core.agents.prompts.loader import load_prompt

        templates_dir = Path(__file__).parent.parent.parent / "prompts/templates"

        # Check if sections directory exists for this mode
        sections_dir = templates_dir / self._core_template
        if not sections_dir.exists():
            return ""

        # Load core template
        try:
            core_prompt = load_prompt(self._core_template)
        except FileNotFoundError:
            return ""

        # Compose modular sections for this mode
        composer = create_composer(templates_dir, self._core_template)

        # Build context for conditional sections
        context = {
            "in_git_repo": self._env_context and self._env_context.is_git_repo,
            "has_subagents": True,  # Subagents always available
            "todo_tracking_enabled": True,  # Todo tracking always available
            "model_provider": (self._env_context.model_provider if self._env_context else ""),
            "model": self._env_context.model if self._env_context else "",
        }

        modular_sections = composer.compose(context)

        # Combine core prompt + modular sections
        return f"{core_prompt}\n\n{modular_sections}" if modular_sections else core_prompt

    def _build_modular_two_part(self) -> tuple[str, str]:
        """Build prompt from modular sections, split into stable/dynamic.

        Returns:
            Tuple of (stable, dynamic). Both may be empty if modular
            composition is not available.
        """
        from pathlib import Path
        from swecli.core.agents.prompts.composition import create_composer
        from swecli.core.agents.prompts.loader import load_prompt

        templates_dir = Path(__file__).parent.parent.parent / "prompts/templates"
        sections_dir = templates_dir / self._core_template
        if not sections_dir.exists():
            return "", ""

        try:
            core_prompt = load_prompt(self._core_template)
        except FileNotFoundError:
            return "", ""

        composer = create_composer(templates_dir, self._core_template)
        context = {
            "in_git_repo": self._env_context and self._env_context.is_git_repo,
            "has_subagents": True,
            "todo_tracking_enabled": True,
            "model_provider": (self._env_context.model_provider if self._env_context else ""),
            "model": self._env_context.model if self._env_context else "",
        }

        stable_sections, dynamic_sections = composer.compose_two_part(context)

        # Core prompt is always stable
        if stable_sections:
            stable = f"{core_prompt}\n\n{stable_sections}"
        else:
            stable = core_prompt

        return stable, dynamic_sections

    def _build_core_identity(self) -> str:
        """Load and return core identity from template.

        This is the fallback when modular composition is not available.
        """
        return load_prompt(self._core_template)

    def _build_environment(self) -> str:
        """Build environment context section."""
        if self._env_context:
            from .environment import (
                build_env_block,
                build_git_status_block,
                build_project_structure_block,
            )

            parts = [
                build_env_block(self._env_context),
                build_git_status_block(self._env_context),
                build_project_structure_block(self._env_context),
            ]
            return "\n\n".join(filter(None, parts))

        # Fallback: minimal working directory text
        if not self._working_dir:
            return ""
        return f"""# Working Directory Context

You are currently working in the directory: `{self._working_dir}`

When processing file paths without explicit directories (like `app.py` or `README.md`), assume they are located in the current working directory unless the user provides a specific path. Use relative paths from the working directory for file operations."""

    def _build_project_instructions(self) -> str:
        """Build project instructions section from SWECLI.md content."""
        if self._env_context:
            from .environment import build_project_instructions_block

            return build_project_instructions_block(self._env_context)
        return ""

    def _build_skills_index(self) -> str:
        """Build available skills section from SkillLoader."""
        # Try to get skill_loader from multiple sources
        loader = self._skill_loader
        if not loader and self._tool_registry:
            loader = getattr(self._tool_registry, "_skill_loader", None)

        if not loader:
            return ""

        return loader.build_skills_index()

    def _build_mcp_section(self) -> str:
        """Render MCP section - shows connected servers, not individual tools.

        Individual tool schemas are NOT loaded by default for token efficiency.
        The agent must use search_tools() to discover and enable tools.
        """
        if not self._tool_registry or not getattr(self._tool_registry, "mcp_manager", None):
            return ""

        mcp_manager = self._tool_registry.mcp_manager
        all_servers = mcp_manager.list_servers()
        connected_servers = [name for name in all_servers if mcp_manager.is_connected(name)]

        if not connected_servers:
            return ""

        lines = ["\n## MCP Servers Connected\n\n"]

        for server_name in connected_servers:
            tools = mcp_manager.get_server_tools(server_name)
            lines.append(f"- **{server_name}**: {len(tools)} tools available\n")

        lines.append("\nUse `search_tools` to discover and enable MCP tools.\n")

        return "".join(lines)

    def _build_mcp_config_section(self) -> str:
        """Render the MCP configuration section when no servers are connected."""
        lines = [
            "\n## MCP Server Configuration\n",
            "You can help users set up MCP (Model Context Protocol) servers "
            "for external integrations.\n\n",
            "When users ask about setting up an MCP server:\n",
            "1. Use `web_search` to find the MCP server package and docs\n",
            "2. Use `fetch_url` to read the server's README/documentation\n",
            "3. Read `~/.opendev/mcp.json` and add the server configuration\n",
            "4. Tell the user to connect with `/mcp connect <name>`\n",
        ]
        return "".join(lines)


class SystemPromptBuilder(BasePromptBuilder):
    """Constructs the NORMAL mode system prompt with optional MCP tooling."""

    _core_template = "system/main"


class ThinkingPromptBuilder(BasePromptBuilder):
    """Constructs the THINKING mode system prompt for reasoning tasks."""

    _core_template = "system/thinking"


class PlanningPromptBuilder:
    """Constructs the PLAN mode strategic planning prompt."""

    def __init__(
        self,
        working_dir: Any | None = None,
        env_context: "EnvironmentContext | None" = None,
    ) -> None:
        self._working_dir = working_dir
        self._env_context = env_context

    def build(self) -> str:
        """Return the planning prompt with working directory context."""
        prompt = load_prompt("system/planner")

        # Add environment context
        env_section = self._build_environment()
        if env_section:
            prompt += "\n\n" + env_section

        # Add project instructions
        instructions = self._build_project_instructions()
        if instructions:
            prompt += "\n\n" + instructions

        return prompt

    def _build_environment(self) -> str:
        """Build environment context section."""
        if self._env_context:
            from .environment import (
                build_env_block,
                build_git_status_block,
                build_project_structure_block,
            )

            parts = [
                build_env_block(self._env_context),
                build_git_status_block(self._env_context),
                build_project_structure_block(self._env_context),
            ]
            return "\n\n".join(filter(None, parts))

        if not self._working_dir:
            return ""
        return (
            f"# Working Directory Context\n\n"
            f"You are currently exploring the codebase in: `{self._working_dir}`\n\n"
            f"Use this as the base directory for all file operations and searches.\n"
        )

    def _build_project_instructions(self) -> str:
        """Build project instructions section from SWECLI.md content."""
        if self._env_context:
            from .environment import build_project_instructions_block

            return build_project_instructions_block(self._env_context)
        return ""
