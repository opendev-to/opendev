"""Base command handler for REPL commands."""

from abc import ABC, abstractmethod
from dataclasses import dataclass
from typing import Any, Optional

from rich.console import Console

from swecli.ui_textual.formatters.result_formatter import (
    ToolResultFormatter,
    get_formatter,
    RESULT_PREFIX,
    RESULT_CONTINUATION,
)


@dataclass
class CommandResult:
    """Result of a command execution.

    Attributes:
        success: Whether the command executed successfully
        message: Optional message to display
        data: Optional data returned by the command
    """

    success: bool
    message: Optional[str] = None
    data: Optional[Any] = None


class CommandHandler(ABC):
    """Abstract base class for command handlers.

    Each command handler is responsible for executing a specific
    command or group of related commands.

    Output Formatting:
        All output methods (print_success, print_error, etc.) use
        ToolResultFormatter to ensure consistent `⎿` prefixed display.
    """

    def __init__(self, console: Console):
        """Initialize command handler.

        Args:
            console: Rich console for output
        """
        self.console = console
        self._formatter: ToolResultFormatter = get_formatter()

    @abstractmethod
    def handle(self, args: str) -> CommandResult:
        """Handle the command execution.

        Args:
            args: Command arguments (text after the command name)

        Returns:
            CommandResult with execution status and optional message
        """
        pass

    def print_command_header(self, command_name: str, params: str = "") -> None:
        """Print command header with ⏺ symbol.

        Adds a blank line before the header for visual separation from command input.
        This follows the unified spacing standard: unconditional blank before command headers.

        Args:
            command_name: Name of the command
            params: Optional parameters to display
        """
        self.console.print("")  # Unconditional blank before command header (spacing standard)
        if params:
            self.console.print(f"[cyan]⏺[/cyan] {command_name} ({params})")
        else:
            self.console.print(f"[cyan]⏺[/cyan] {command_name}")

    def print_success(self, message: str) -> None:
        """Print success message with ⎿ prefix.

        Args:
            message: Message to display
        """
        self.console.print(self._formatter.format_success(message))

    def print_error(self, message: str) -> None:
        """Print error message with ⎿ prefix.

        Args:
            message: Error message to display
        """
        self.console.print(self._formatter.format_error(message))

    def print_warning(self, message: str) -> None:
        """Print warning message with ⎿ prefix.

        Args:
            message: Warning message to display
        """
        self.console.print(self._formatter.format_warning(message))

    def print_info(self, message: str) -> None:
        """Print info message with ⎿ prefix.

        Args:
            message: Info message to display
        """
        self.console.print(self._formatter.format_info(message))

    def print_line(self, message: str) -> None:
        """Print a line with standard ⎿ prefix (2 spaces + ⎿ + 2 spaces).

        Args:
            message: Message to display
        """
        self.console.print(f"{RESULT_PREFIX}{message}")

    def print_continuation(self, message: str) -> None:
        """Print a continuation line (5 spaces, aligned with content after ⎿).

        Args:
            message: Message to display
        """
        self.console.print(f"{RESULT_CONTINUATION}{message}")

    def print_result_only(self, message: str) -> None:
        """Print result line without header (for status/info display).

        Use this for commands that show status/info without performing actions.
        Outputs with ⎿ prefix but no preceding ⏺ header.

        Args:
            message: Message to display
        """
        self.console.print(f"{RESULT_PREFIX}{message}")

    def print_spacing(self) -> None:
        """Print a blank line for visual separation.

        This follows the unified spacing standard: adds a trailing blank
        after structural elements (errors, command results, etc.).
        """
        self.console.print("")
