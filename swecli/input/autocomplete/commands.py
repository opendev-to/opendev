"""Slash command definitions and management."""

from typing import List


class SlashCommand:
    """Represents a slash command."""

    def __init__(self, name: str, description: str):
        """Initialize slash command.

        Args:
            name: Command name (without /)
            description: Command description
        """
        self.name = name
        self.description = description


# Built-in slash commands (only implemented ones)
DEFAULT_SLASH_COMMANDS = [
    # Session management
    SlashCommand("help", "show available commands and help"),
    SlashCommand("exit", "exit OpenDev"),
    SlashCommand("quit", "exit OpenDev (alias for /exit)"),
    SlashCommand("clear", "clear current session and start fresh"),
    SlashCommand("models", "switch active AI model"),
    # Execution
    SlashCommand("mode", "switch between NORMAL and PLAN mode"),
    # Advanced
    SlashCommand("init", "analyze codebase and generate OPENDEV.md"),
    SlashCommand("mcp", "manage MCP servers and tools"),
]


class SlashCommandManager:
    """Manages slash commands."""

    def __init__(self, commands: List[SlashCommand] = None):
        """Initialize slash command manager.

        Args:
            commands: List of commands (uses default if None)
        """
        self.commands = commands or DEFAULT_SLASH_COMMANDS

    def get_commands(self) -> List[SlashCommand]:
        """Get all available commands."""
        return self.commands

    def find_commands(self, query: str) -> List[SlashCommand]:
        """Find commands matching query.

        Args:
            query: Search query (without leading /)

        Returns:
            List of matching commands
        """
        query_lower = query.lower()
        return [cmd for cmd in self.commands if cmd.name.startswith(query_lower)]

    def get_command(self, name: str) -> SlashCommand:
        """Get a specific command by name.

        Args:
            name: Command name

        Returns:
            Command object or None if not found
        """
        for cmd in self.commands:
            if cmd.name == name:
                return cmd
        return None
