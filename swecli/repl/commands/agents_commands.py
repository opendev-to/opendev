"""Command handler for /agents command to manage custom agents."""

from pathlib import Path
from typing import Any

from rich.console import Console
from rich.table import Table
from rich.prompt import Prompt, Confirm

from swecli.repl.commands.base import CommandHandler, CommandResult
from swecli.core.paths import get_paths, APP_DIR_NAME


# Default template for new agents (Claude Code style)
AGENT_TEMPLATE = '''---
name: {name}
description: "{description}"
model: sonnet
tools: "*"
---

You are a specialized agent for {purpose}.

## Your Mission

{mission}

## Guidelines

- Be thorough and provide clear explanations
- Use available tools to gather information and complete tasks
- Ask clarifying questions if requirements are unclear
'''


class AgentsCommands(CommandHandler):
    """Handler for /agents command to create and manage custom agents."""

    def __init__(
        self,
        console: Console,
        config_manager: Any,
        subagent_manager: Any = None,
    ):
        """Initialize agents command handler.

        Args:
            console: Rich console for output
            config_manager: Configuration manager
            subagent_manager: SubAgentManager instance (optional)
        """
        super().__init__(console)
        self.config_manager = config_manager
        self.subagent_manager = subagent_manager

    def handle(self, args: str) -> CommandResult:
        """Handle /agents command and subcommands.

        Args:
            args: Command arguments

        Returns:
            CommandResult with execution status
        """
        if not args:
            return self._show_menu()

        parts = args.split(maxsplit=1)
        subcmd = parts[0].lower()
        subcmd_args = parts[1] if len(parts) > 1 else ""

        if subcmd == "create":
            return self._create_agent(subcmd_args)
        elif subcmd == "list":
            return self._list_agents()
        elif subcmd == "edit":
            return self._edit_agent(subcmd_args)
        elif subcmd == "delete":
            return self._delete_agent(subcmd_args)
        else:
            return self._show_menu()

    def _show_menu(self) -> CommandResult:
        """Show available agent commands."""
        self.print_line("[cyan]/agents create[/cyan]  Create a new custom agent")
        self.print_continuation("[cyan]/agents list[/cyan]    List all available agents")
        self.console.print()

        return CommandResult(success=True)

    def _create_agent(self, args: str) -> CommandResult:
        """Create a new custom agent with interactive prompts.

        Args:
            args: Optional agent name

        Returns:
            CommandResult with creation status
        """
        try:
            # Ask for location
            paths = get_paths(self.config_manager.working_dir)
            self.print_info("Where should the agent be created?")
            self.console.print(f"  [cyan]1[/cyan]. Personal (~/{APP_DIR_NAME}/agents/)")
            self.console.print(f"  [cyan]2[/cyan]. Project ({APP_DIR_NAME}/agents/)")

            choice = Prompt.ask("Select location", choices=["1", "2"], default="1")
            is_personal = choice == "1"

            if is_personal:
                agents_dir = paths.global_agents_dir
            else:
                agents_dir = paths.project_agents_dir

            # Ask for agent name
            if args:
                name = args.strip().replace(" ", "-").lower()
            else:
                name = Prompt.ask("Agent name (e.g., code-reviewer)")
                name = name.strip().replace(" ", "-").lower()

            if not name:
                self.print_error("Agent name is required")
                return CommandResult(success=False, message="Agent name required")

            # Check if agent already exists
            agent_file = agents_dir / f"{name}.md"
            if agent_file.exists():
                if not Confirm.ask(f"Agent '{name}' already exists. Overwrite?"):
                    self.print_info("Cancelled")
                    return CommandResult(success=False, message="Cancelled")

            # Ask for description
            description = Prompt.ask(
                "Description",
                default=f"A specialized agent for {name.replace('-', ' ')}"
            )

            # Ask for purpose (for template)
            purpose = Prompt.ask(
                "What is this agent's purpose?",
                default=name.replace("-", " ")
            )

            # Generate agent content
            content = AGENT_TEMPLATE.format(
                name=name,
                description=description,
                purpose=purpose,
                mission=f"Complete tasks related to {purpose} efficiently and accurately.",
            )

            # Create directory and write file
            agents_dir.mkdir(parents=True, exist_ok=True)
            agent_file.write_text(content, encoding="utf-8")

            self.print_success(f"Created agent: {agent_file}")
            self.console.print(f"  Edit the file to customize the system prompt.")

            return CommandResult(
                success=True,
                message=f"Agent created: {name}",
                data={"path": str(agent_file), "name": name}
            )

        except Exception as e:
            self.print_error(f"Failed to create agent: {e}")
            return CommandResult(success=False, message=str(e))

    def _list_agents(self) -> CommandResult:
        """List all available agents (builtin and custom).

        Returns:
            CommandResult with agent list
        """
        from swecli.core.agents.subagents.agents import ALL_SUBAGENTS

        table = Table(show_header=True, header_style="bold")
        table.add_column("Name", style="cyan")
        table.add_column("Description")
        table.add_column("Source", style="dim")

        # Add builtin agents
        for spec in ALL_SUBAGENTS:
            desc = spec.get("description", "")
            if len(desc) > 60:
                desc = desc[:60] + "..."
            table.add_row(spec["name"], desc, "built-in")

        # Add custom agents from config
        custom_agents = self.config_manager.load_custom_agents()
        for agent in custom_agents:
            desc = agent.get("description", "")
            if len(desc) > 60:
                desc = desc[:60] + "..."
            source = agent.get("_source", "custom")
            table.add_row(agent.get("name", ""), desc, source)

        self.console.print(table)

        return CommandResult(success=True)

    def _edit_agent(self, name: str) -> CommandResult:
        """Open an agent file for editing.

        Args:
            name: Agent name to edit

        Returns:
            CommandResult with edit status
        """
        if not name:
            self.print_error("Agent name required: /agents edit <name>")
            return CommandResult(success=False, message="Agent name required")

        # Find the agent file
        agent_file = self._find_agent_file(name)
        if not agent_file:
            self.print_error(f"Agent not found: {name}")
            self.print_info("Use '/agents list' to see available agents")
            return CommandResult(success=False, message=f"Agent not found: {name}")

        # Try to open in editor
        import os
        import subprocess

        editor = os.environ.get("EDITOR", "nano")
        try:
            subprocess.run([editor, str(agent_file)])
            self.print_success(f"Edited: {agent_file}")
            return CommandResult(success=True, message=f"Edited: {name}")
        except Exception as e:
            self.print_error(f"Failed to open editor: {e}")
            self.print_info(f"File location: {agent_file}")
            return CommandResult(success=False, message=str(e))

    def _delete_agent(self, name: str) -> CommandResult:
        """Delete a custom agent.

        Args:
            name: Agent name to delete

        Returns:
            CommandResult with deletion status
        """
        if not name:
            self.print_error("Agent name required: /agents delete <name>")
            return CommandResult(success=False, message="Agent name required")

        # Find the agent file
        agent_file = self._find_agent_file(name)
        if not agent_file:
            self.print_error(f"Agent not found: {name}")
            return CommandResult(success=False, message=f"Agent not found: {name}")

        # Confirm deletion
        if not Confirm.ask(f"Delete agent '{name}'?"):
            self.print_info("Cancelled")
            return CommandResult(success=False, message="Cancelled")

        try:
            agent_file.unlink()
            self.print_success(f"Deleted: {name}")
            return CommandResult(success=True, message=f"Deleted: {name}")
        except Exception as e:
            self.print_error(f"Failed to delete: {e}")
            return CommandResult(success=False, message=str(e))

    def _find_agent_file(self, name: str) -> Path | None:
        """Find the markdown file for a custom agent.

        Args:
            name: Agent name (with or without .md extension)

        Returns:
            Path to the agent file or None if not found
        """
        # Normalize name (remove .md if present)
        name = name.replace(".md", "").strip()

        # Search in project first, then user global
        paths = get_paths(self.config_manager.working_dir)
        search_dirs = []
        if self.config_manager.working_dir:
            search_dirs.append(paths.project_agents_dir)
        search_dirs.append(paths.global_agents_dir)

        for agents_dir in search_dirs:
            agent_file = agents_dir / f"{name}.md"
            if agent_file.exists():
                return agent_file

        return None
