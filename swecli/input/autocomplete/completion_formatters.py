"""Completion formatting utilities."""

from pathlib import Path
from typing import List
from prompt_toolkit.completion import Completion
from prompt_toolkit.formatted_text import FormattedText

from .file_finder import FileFinder


class CompletionFormatter:
    """Formats completion suggestions for display."""

    def __init__(self, file_finder: FileFinder):
        """Initialize completion formatter.

        Args:
            file_finder: File finder instance
        """
        self.file_finder = file_finder

    def format_slash_command_completion(
        self,
        command_name: str,
        command_description: str,
        start_position: int,
    ) -> Completion:
        """Format a slash command completion.

        Args:
            command_name: Name of the command
            command_description: Description of the command
            start_position: Cursor position for completion

        Returns:
            Formatted completion object
        """
        display = FormattedText([
            ("cyan", f"/{command_name:<16}"),
            ("", " "),
            ("class:completion-menu.meta", command_description),
        ])

        return Completion(
            text=f"/{command_name}",
            start_position=start_position,
            display=display,
        )

    def format_file_mention_completion(
        self,
        file_path: Path,
        start_position: int,
        show_size: bool = True,
    ) -> Completion:
        """Format a file mention completion.

        Args:
            file_path: File path to complete
            start_position: Cursor position for completion
            show_size: Whether to include file size

        Returns:
            Formatted completion object
        """
        # Get relative path for display
        rel_path = self.file_finder.get_relative_path(file_path)

        # Get file size for display
        size_str = ""
        if show_size:
            try:
                size = file_path.stat().st_size
                size_str = self.file_finder.format_file_size(size)
            except (OSError, FileNotFoundError):
                pass

        # Elegant formatted display (no @ prefix, with file size)
        if size_str:
            display = FormattedText([
                ("", f"{str(rel_path):<50}"),
                ("class:completion-menu.meta", f"{size_str:>10}"),
            ])
        else:
            display = FormattedText([
                ("", str(rel_path)),
            ])

        return Completion(
            text=f"@{rel_path}",
            start_position=start_position,
            display=display,
        )

    def format_simple_file_completion(
        self,
        file_path: Path,
        start_position: int,
    ) -> Completion:
        """Format a simple file completion (without @ prefix).

        Args:
            file_path: File path to complete
            start_position: Cursor position for completion

        Returns:
            Formatted completion object
        """
        rel_path = self.file_finder.get_relative_path(file_path)

        return Completion(
            text=str(rel_path),
            start_position=start_position,
            display=str(rel_path),
            display_meta="file",
        )

    def format_multiple_file_completions(
        self,
        file_paths: List[Path],
        start_position: int,
        show_size: bool = True
    ) -> List[Completion]:
        """Format multiple file completions.

        Args:
            file_paths: List of file paths
            start_position: Cursor position for completion
            show_size: Whether to include file sizes

        Returns:
            List of formatted completion objects
        """
        completions = []
        for file_path in file_paths:
            if show_size:
                completion = self.format_file_mention_completion(
                    file_path, start_position, show_size=True
                )
            else:
                completion = self.format_simple_file_completion(
                    file_path, start_position
                )
            completions.append(completion)
        return completions