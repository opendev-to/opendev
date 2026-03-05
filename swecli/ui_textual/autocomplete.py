"""Autocomplete system for file mentions (@) and slash commands (/)."""

from pathlib import Path
from typing import List, Optional

from .autocomplete_internal.completers import (
    SwecliCompleter as RefactoredSwecliCompleter,
    FileMentionCompleter as RefactoredFileMentionCompleter,
    SlashCommandCompleter as RefactoredSlashCommandCompleter,
)
from .autocomplete_internal.commands import (
    SlashCommand,
    BUILTIN_COMMANDS,
)


# Backward compatibility - use refactored implementations
class SwecliCompleter(RefactoredSwecliCompleter):
    """Custom completer for OpenDev that handles @ mentions and / commands."""

    def __init__(self, working_dir: Path):
        """Initialize completer.

        Args:
            working_dir: Working directory for file mentions
        """
        super().__init__(working_dir)


class FileMentionCompleter(RefactoredFileMentionCompleter):
    """Simpler file mention completer (@ only)."""

    def __init__(self, working_dir: Path):
        """Initialize file mention completer.

        Args:
            working_dir: Working directory for file mentions
        """
        super().__init__(working_dir)


class SlashCommandCompleter(RefactoredSlashCommandCompleter):
    """Simpler slash command completer (/ only)."""

    def __init__(self, commands: Optional[List[SlashCommand]] = None):
        """Initialize slash command completer.

        Args:
            commands: List of slash commands (uses built-in if None)
        """
        super().__init__(commands=commands)


# Backward compatibility - re-export original constants
SLASH_COMMANDS = BUILTIN_COMMANDS.get_commands()


# Re-export for backward compatibility
__all__ = [
    "SwecliCompleter",
    "FileMentionCompleter",
    "SlashCommandCompleter",
    "SlashCommand",
    "SLASH_COMMANDS",
]