"""Completer implementations using strategy pattern."""

import re
from pathlib import Path
from typing import Iterable, Optional, List

from prompt_toolkit.completion import Completer, Completion
from prompt_toolkit.document import Document

from .commands import BUILTIN_COMMANDS, CommandRegistry, SlashCommand
from .utils import FileFinder
from .completion_strategies import SlashCommandStrategy, FileMentionStrategy


class SwecliCompleter(Completer):
    """Custom completer for OpenDev that handles @ mentions and / commands."""

    def __init__(self, working_dir: Path, command_registry: Optional[CommandRegistry] = None):
        """Initialize completer.

        Args:
            working_dir: Working directory for file mentions
            command_registry: Command registry (uses built-in if None)
        """
        self.working_dir = working_dir
        self.command_registry = command_registry or BUILTIN_COMMANDS

        # Initialize strategies
        self.slash_strategy = SlashCommandStrategy(self.command_registry)
        self.file_finder = FileFinder(working_dir)
        self.file_strategy = FileMentionStrategy(working_dir, self.file_finder)

    def get_completions(
        self, document: Document, complete_event
    ) -> Iterable[Completion]:
        """Get completions based on current input.

        Args:
            document: Current document
            complete_event: Completion event

        Yields:
            Completion objects
        """
        text = document.text_before_cursor

        # Find the word to complete (slash command or file mention)
        # Look for patterns like "/word" or "@word"
        match = re.search(r'([@/])([^\s@/]*)$', text)

        if not match:
            return

        trigger, word_part = match.groups()
        full_word = trigger + word_part

        # Delegate to appropriate strategy
        if trigger == "/":
            yield from self.slash_strategy.get_completions(full_word, document)
        elif trigger == "@":
            yield from self.file_strategy.get_completions(full_word, document)


class FileMentionCompleter(Completer):
    """Simpler file mention completer (@ only)."""

    def __init__(self, working_dir: Path):
        """Initialize file mention completer.

        Args:
            working_dir: Working directory for file mentions
        """
        self.working_dir = working_dir
        self.file_finder = FileFinder(working_dir)
        self.file_strategy = FileMentionStrategy(working_dir, self.file_finder)

    def get_completions(
        self, document: Document, complete_event
    ) -> Iterable[Completion]:
        """Get file mention completions.

        Args:
            document: Current document
            complete_event: Completion event

        Yields:
            Completion objects
        """
        text_before_cursor = document.text_before_cursor

        # Check if we're in a file mention context
        if "@" in text_before_cursor:
            # Get the text after the last @
            parts = text_before_cursor.split("@")
            if len(parts) > 1:
                query = parts[-1]
                word = "@" + query

                yield from self.file_strategy.get_completions(word, document)


class SlashCommandCompleter(Completer):
    """Simpler slash command completer (/ only)."""

    def __init__(self, commands: Optional[List[SlashCommand]] = None,
                 command_registry: Optional[CommandRegistry] = None):
        """Initialize slash command completer.

        Args:
            commands: List of slash commands (legacy, for backward compatibility)
            command_registry: Command registry (preferred)
        """
        if command_registry:
            self.command_registry = command_registry
        elif commands:
            # Create registry from list for backward compatibility
            self.command_registry = CommandRegistry()
            for cmd in commands:
                self.command_registry.register(cmd)
        else:
            self.command_registry = BUILTIN_COMMANDS

        self.slash_strategy = SlashCommandStrategy(self.command_registry)

    def get_completions(
        self, document: Document, complete_event
    ) -> Iterable[Completion]:
        """Get slash command completions.

        Args:
            document: Current document
            complete_event: Completion event

        Yields:
            Completion objects
        """
        text_before_cursor = document.text_before_cursor

        # Only complete if at start of line or after whitespace
        if text_before_cursor.startswith("/") or (
            len(text_before_cursor) > 0
            and text_before_cursor[-1].isspace()
            and "/" in text_before_cursor
        ):
            # Get the command query
            if text_before_cursor.startswith("/"):
                word = text_before_cursor
                yield from self.slash_strategy.get_completions(word, document)