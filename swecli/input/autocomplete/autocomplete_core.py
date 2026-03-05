"""Core autocomplete functionality."""

import re
from pathlib import Path
from typing import Iterable, Optional, List
from prompt_toolkit.completion import Completer, Completion
from prompt_toolkit.document import Document

from .commands import SlashCommandManager, SlashCommand
from .file_finder import FileFinder
from .completion_formatters import CompletionFormatter


class SwecliAutocompleteCore(Completer):
    """Core autocomplete functionality that handles @ mentions and / commands."""

    def __init__(self, working_dir: Path, commands: Optional[List[SlashCommand]] = None):
        """Initialize autocomplete core.

        Args:
            working_dir: Working directory for file mentions
            commands: Custom slash commands (uses built-in if None)
        """
        self.working_dir = working_dir
        self.file_finder = FileFinder(working_dir)
        self.command_manager = SlashCommandManager(commands)
        self.formatter = CompletionFormatter(self.file_finder)

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

        # Slash command completion
        if trigger == "/":
            yield from self._get_slash_command_completions(full_word)

        # File mention completion
        elif trigger == "@":
            yield from self._get_file_mention_completions(full_word)

    def _get_slash_command_completions(self, word: str) -> Iterable[Completion]:
        """Get slash command completions.

        Args:
            word: Current word (starts with /)

        Yields:
            Completion objects for matching commands
        """
        query = word[1:].lower()  # Remove leading /
        matching_commands = self.command_manager.find_commands(query)

        for cmd in matching_commands:
            # Calculate start position (negative, relative to cursor)
            start_position = -len(word)

            yield self.formatter.format_slash_command_completion(
                cmd.name, cmd.description, start_position
            )

    def _get_file_mention_completions(self, word: str) -> Iterable[Completion]:
        """Get file mention completions.

        Args:
            word: Current word (starts with @)

        Yields:
            Completion objects for matching files
        """
        query = word[1:]  # Remove leading @
        files = self.file_finder.find_files(query)

        for file_path in files:
            # Calculate start position (negative, relative to cursor)
            start_position = -len(word)

            yield self.formatter.format_file_mention_completion(
                file_path, start_position, show_size=True
            )


class FileMentionCompleter(Completer):
    """Simpler file mention completer (@ only)."""

    def __init__(self, working_dir: Path):
        """Initialize file mention completer.

        Args:
            working_dir: Working directory for file mentions
        """
        self.working_dir = working_dir
        self.file_finder = FileFinder(working_dir)
        self.formatter = CompletionFormatter(self.file_finder)

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

                # Find matching files
                files = self.file_finder.find_files(query)

                for file_path in files:
                    # Calculate start position
                    start_position = -len(query)

                    yield self.formatter.format_simple_file_completion(
                        file_path, start_position
                    )


class SlashCommandCompleter(Completer):
    """Simpler slash command completer (/ only)."""

    def __init__(self, commands: Optional[List[SlashCommand]] = None):
        """Initialize slash command completer.

        Args:
            commands: List of slash commands (uses built-in if None)
        """
        self.command_manager = SlashCommandManager(commands)
        self.formatter = CompletionFormatter(None)

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
                query = text_before_cursor[1:].lower()
                start_position = -len(text_before_cursor)
            else:
                return

            matching_commands = self.command_manager.find_commands(query)

            for cmd in matching_commands:
                yield self.formatter.format_slash_command_completion(
                    cmd.name, cmd.description, start_position
                )