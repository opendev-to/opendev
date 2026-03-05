"""Error handling and recovery for operations."""

from enum import Enum
from typing import Optional

from rich.console import Console
from prompt_toolkit import prompt

from swecli.models.operation import Operation


class ErrorAction(str, Enum):
    """Actions user can take on error."""

    RETRY = "r"
    SKIP = "s"
    CANCEL = "c"
    EDIT = "e"


class ErrorResult:
    """Result of error handling."""

    def __init__(
        self,
        action: ErrorAction,
        should_retry: bool = False,
        should_cancel: bool = False,
        edited_params: Optional[dict] = None,
    ):
        """Initialize error result.

        Args:
            action: Action chosen by user
            should_retry: Whether to retry the operation
            should_cancel: Whether to cancel remaining operations
            edited_params: Edited parameters if user chose to edit
        """
        self.action = action
        self.should_retry = should_retry
        self.should_cancel = should_cancel
        self.edited_params = edited_params


class ErrorHandler:
    """Handler for operation errors with recovery options."""

    def __init__(self, console: Optional[Console] = None):
        """Initialize error handler.

        Args:
            console: Rich console for output
        """
        self.console = console or Console()

    def handle_error(
        self,
        error: Exception,
        operation: Operation,
        allow_retry: bool = True,
        allow_edit: bool = False,
    ) -> ErrorResult:
        """Handle an operation error interactively.

        Args:
            error: Exception that occurred
            operation: Operation that failed
            allow_retry: Whether to allow retry
            allow_edit: Whether to allow editing parameters

        Returns:
            ErrorResult with user's chosen action
        """
        # Display error
        self.console.print(f"\n[bold red]✗ Error:[/bold red] {str(error)}")
        self.console.print(f"[dim]Operation: {operation.type.value}[/dim]")
        self.console.print(f"[dim]Target: {operation.target}[/dim]")

        # Build options
        options = []
        if allow_retry:
            options.append("r - Retry")
        if allow_edit:
            options.append("e - Edit parameters and retry")
        options.append("s - Skip this operation")
        options.append("c - Cancel all remaining operations")

        # Prompt user
        self.console.print(f"\n[yellow]What would you like to do?[/yellow]")
        for option in options:
            self.console.print(f"  {option}")

        try:
            choice = prompt("Choice: ").lower()

            if choice == "r" and allow_retry:
                return ErrorResult(
                    action=ErrorAction.RETRY,
                    should_retry=True,
                )
            elif choice == "e" and allow_edit:
                # TODO: Implement parameter editing
                self.console.print("[yellow]Parameter editing not yet implemented[/yellow]")
                return self.handle_error(error, operation, allow_retry, allow_edit)
            elif choice == "s":
                return ErrorResult(
                    action=ErrorAction.SKIP,
                )
            elif choice == "c":
                return ErrorResult(
                    action=ErrorAction.CANCEL,
                    should_cancel=True,
                )
            else:
                self.console.print("[red]Invalid choice[/red]")
                return self.handle_error(error, operation, allow_retry, allow_edit)

        except KeyboardInterrupt:
            return ErrorResult(
                action=ErrorAction.CANCEL,
                should_cancel=True,
            )

    def display_error(self, error: Exception, context: Optional[str] = None) -> None:
        """Display an error message.

        Args:
            error: Exception to display
            context: Optional context information
        """
        self.console.print(f"\n[bold red]✗ Error:[/bold red] {str(error)}")
        if context:
            self.console.print(f"[dim]{context}[/dim]")

    def confirm_dangerous_operation(self, operation: Operation) -> bool:
        """Confirm a dangerous operation with the user.

        Args:
            operation: Dangerous operation to confirm

        Returns:
            True if user confirms
        """
        self.console.print(f"\n[bold yellow]⚠️  Warning:[/bold yellow] This is a potentially dangerous operation")
        self.console.print(f"[dim]Operation: {operation.type.value}[/dim]")
        self.console.print(f"[dim]Target: {operation.target}[/dim]")

        try:
            response = prompt("Are you sure you want to proceed? [y/N]: ").lower()
            return response in ["y", "yes"]
        except KeyboardInterrupt:
            return False
