"""Completion strategies for different types of autocomplete."""

from abc import ABC, abstractmethod
from pathlib import Path
from typing import Iterable

from prompt_toolkit.completion import Completion
from prompt_toolkit.document import Document
from prompt_toolkit.formatted_text import FormattedText

from swecli.ui_textual.style_tokens import GREY

from .commands import CommandRegistry
from .utils import FileFinder, FileSizeFormatter


def get_file_icon(file_path: Path) -> tuple[str, str]:
    """Get professional icon for file based on extension or type.

    Args:
        file_path: Path to the file

    Returns:
        Tuple of (icon_text, color_class) for display
    """
    if file_path.is_dir():
        return "/", ""

    suffix = file_path.suffix.lower()

    # Programming languages - show extension
    if suffix == ".py":
        return "py", "cyan"
    elif suffix == ".js":
        return "js", "yellow"
    elif suffix == ".jsx":
        return "jsx", "yellow"
    elif suffix == ".ts":
        return "ts", "blue"
    elif suffix == ".tsx":
        return "tsx", "blue"
    elif suffix == ".rs":
        return "rs", "red"
    elif suffix == ".go":
        return "go", "cyan"
    elif suffix == ".java":
        return "java", "red"
    elif suffix in [".c", ".h"]:
        return "c", "blue"
    elif suffix in [".cpp", ".cc", ".hpp"]:
        return "cpp", "blue"
    elif suffix == ".cs":
        return "cs", "green"
    elif suffix == ".rb":
        return "rb", "red"
    elif suffix == ".php":
        return "php", "purple"
    elif suffix == ".swift":
        return "swift", "orange"
    elif suffix == ".kt":
        return "kt", "purple"
    elif suffix == ".kts":
        return "kts", "purple"

    # Web files
    elif suffix in [".html", ".htm"]:
        return "html", "orange"
    elif suffix == ".css":
        return "css", "blue"
    elif suffix in [".scss", ".sass"]:
        return "scss", "pink"
    elif suffix == ".less":
        return "less", "blue"
    elif suffix == ".json":
        return "json", "yellow"
    elif suffix == ".xml":
        return "xml", "green"
    elif suffix in [".yaml", ".yml"]:
        return "yaml", "purple"
    elif suffix == ".toml":
        return "toml", "purple"

    # Documentation
    elif suffix in [".md", ".markdown"]:
        return "md", "blue"
    elif suffix == ".txt":
        return "txt", "GREY"
    elif suffix == ".pdf":
        return "pdf", "red"
    elif suffix in [".doc", ".docx"]:
        return "doc", "blue"

    # Images
    elif suffix in [".png", ".jpg", ".jpeg"]:
        return "img", "magenta"
    elif suffix == ".gif":
        return "gif", "magenta"
    elif suffix == ".svg":
        return "svg", "magenta"
    elif suffix in [".ico", ".webp"]:
        return "img", "magenta"

    # Data files
    elif suffix == ".csv":
        return "csv", "green"
    elif suffix == ".sql":
        return "sql", "orange"
    elif suffix in [".db", ".sqlite"]:
        return "db", "cyan"

    # Archives
    elif suffix in [".zip", ".tar", ".gz", ".bz2", ".7z", ".rar"]:
        return "zip", "red"

    # Shell scripts
    elif suffix in [".sh", ".bash", ".zsh"]:
        return "sh", "green"

    # Configuration
    elif suffix == ".env":
        return "env", "yellow"
    elif suffix in [".ini", ".conf", ".config"]:
        return "cfg", "GREY"

    # Build files
    elif file_path.name == "Makefile":
        return "make", "red"
    elif file_path.name == "Dockerfile":
        return "dock", "blue"
    elif file_path.name == "Vagrantfile":
        return "vag", "blue"
    elif suffix == ".lock":
        return "lock", "GREY"

    # Default - show extension without dot, max 4 chars
    else:
        ext = suffix[1:] if suffix else "file"
        return ext[:4], "GREY"


class CompletionStrategy(ABC):
    """Abstract base class for completion strategies."""

    @abstractmethod
    def get_completions(
        self, word: str, document: Document
    ) -> Iterable[Completion]:
        """Get completions for the given word.

        Args:
            word: Current word to complete
            document: Current document

        Yields:
            Completion objects
        """
        pass


class SlashCommandStrategy(CompletionStrategy):
    """Completion strategy for slash commands."""

    def __init__(self, command_registry: CommandRegistry):
        """Initialize slash command strategy.

        Args:
            command_registry: Registry of available commands
        """
        self.command_registry = command_registry

    def get_completions(
        self, word: str, document: Document
    ) -> Iterable[Completion]:
        """Get slash command completions.

        Args:
            word: Current word (starts with /)
            document: Current document

        Yields:
            Completion objects for matching commands
        """
        query = word[1:].lower()  # Remove leading /

        commands = self.command_registry.find_matching(query)

        for cmd in commands:
            start_position = -len(word)

            display = FormattedText([
                ("cyan", f"/{cmd.name:<16}"),
                ("", " "),
                ("class:completion-menu.meta", cmd.description),
            ])

            yield Completion(
                text=f"/{cmd.name}",
                start_position=start_position,
                display=display,
            )


class FileMentionStrategy(CompletionStrategy):
    """Completion strategy for file mentions (@)."""

    def __init__(self, working_dir: Path, file_finder: FileFinder):
        """Initialize file mention strategy.

        Args:
            working_dir: Working directory for file mentions
            file_finder: File finder utility
        """
        self.working_dir = working_dir
        self.file_finder = file_finder

    def get_completions(
        self, word: str, document: Document
    ) -> Iterable[Completion]:
        """Get file mention completions.

        Args:
            word: Current word (starts with @)
            document: Current document

        Yields:
            Completion objects for matching files
        """
        query = word[1:]  # Remove leading @

        paths = self.file_finder.find_files(query, include_dirs=True)

        for file_path in paths:
            start_position = -len(word)

            # Display relative path
            try:
                rel_path = file_path.relative_to(self.working_dir)
            except ValueError:
                rel_path = file_path

            is_dir = file_path.is_dir()
            rel_display = f"{rel_path}{'/' if is_dir else ''}"

            # Get file icon and color
            icon_text, icon_color = get_file_icon(file_path)

            # Use clean colored dot for files, keep folder emoji for directories
            icon_display = icon_text if is_dir else "•"

            # Get file size for display
            size_str = "" if is_dir else FileSizeFormatter.get_file_size(file_path)

            # Elegant formatted display with professional icon
            if size_str:
                display = FormattedText([
                    (icon_color, icon_display),
                    ("", " "),
                    ("", f"{str(rel_display):<46}"),
                    ("class:completion-menu.meta", f"{size_str:>10}"),
                ])
            else:
                display = FormattedText([
                    (icon_color, icon_display),
                    ("", " "),
                    ("", str(rel_display)),
                ])

            yield Completion(
                text=f"@{rel_display}",
                start_position=start_position,
                display=display,
            )
