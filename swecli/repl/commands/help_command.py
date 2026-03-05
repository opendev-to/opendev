"""Help command for REPL."""

from typing import TYPE_CHECKING

from rich.console import Console
from rich.markdown import Markdown
from rich.panel import Panel

from swecli.repl.commands.base import CommandHandler, CommandResult

if TYPE_CHECKING:
    from swecli.core.runtime import ModeManager


class HelpCommand(CommandHandler):
    """Handler for /help command."""

    def __init__(self, console: Console, mode_manager: "ModeManager"):
        """Initialize help command handler.

        Args:
            console: Rich console for output
            mode_manager: Mode manager for showing current mode
        """
        super().__init__(console)
        self.mode_manager = mode_manager

    def handle(self, args: str) -> CommandResult:
        """Show help message."""
        help_text = """
# Available Commands

## Mode & Operations
- `/mode <name>` - Switch mode: normal or plan
- `/init [path]` - Analyze codebase and generate OPENDEV.md

## Session Management
- `/clear` - Clear current session context
- `/compact` - Compact conversation history to reduce context size

## Configuration
- `/models` - Interactive model/provider selector (use ↑/↓ arrows to choose)

## MCP (Model Context Protocol)
- `/mcp list` - List configured MCP servers
- `/mcp status` - Quick status overview
- `/mcp view <name>` - Interactive server viewer (detailed view)
- `/mcp connect <name>` - Connect to an MCP server
- `/mcp disconnect <name>` - Disconnect from a server
- `/mcp enable <name>` - Enable auto-start for a server
- `/mcp disable <name>` - Disable auto-start for a server
- `/mcp tools [<name>]` - Show available tools from server(s)
- `/mcp test <name>` - Test connection to a server
- `/mcp reload` - Reload MCP configuration
- `/mcp debug` - Show debug info (tools in agent)

## Agents & Skills
- `/agents` - Create and manage custom agents
- `/skills` - Create and manage custom skills with AI assistance

## Plugins & Marketplace
- `/plugins marketplace add <url>` - Add a marketplace repository
- `/plugins marketplace list` - List known marketplaces
- `/plugins install <plugin>@<marketplace>` - Install a plugin
- `/plugins list` - List installed plugins
- `/plugins update <plugin>@<marketplace>` - Update a plugin

## General
- `/help` - Show this help message
- `/sound` - Play a test notification sound
- `/exit` - Exit OpenDev

**Current Mode:** {}
{}
        """.format(
            self.mode_manager.current_mode.value.upper(),
            self.mode_manager.get_mode_description()
        )

        self.console.print(Panel(Markdown(help_text), title="Help", border_style="green"))
        self.console.print()
        return CommandResult(success=True)
