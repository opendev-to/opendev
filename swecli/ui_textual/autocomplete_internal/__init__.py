"""Autocomplete system for file mentions (@) and slash commands (/)."""

from .commands import SlashCommand, CommandRegistry, BUILTIN_COMMANDS
from .utils import FileFinder, FileSizeFormatter
from .completion_strategies import CompletionStrategy, SlashCommandStrategy, FileMentionStrategy
from .completers import SwecliCompleter, FileMentionCompleter, SlashCommandCompleter

# Backward compatibility - re-export original constants
SLASH_COMMANDS = BUILTIN_COMMANDS.get_commands()

__all__ = [
    # New modular classes
    "SlashCommand",
    "CommandRegistry",
    "BUILTIN_COMMANDS",
    "FileFinder",
    "FileSizeFormatter",
    "CompletionStrategy",
    "SlashCommandStrategy",
    "FileMentionStrategy",
    "SwecliCompleter",
    "FileMentionCompleter",
    "SlashCommandCompleter",

    # Backward compatibility
    "SLASH_COMMANDS",
]